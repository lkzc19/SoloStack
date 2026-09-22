//! Kafka 的配置布局、生效值与字段读写。
//!
//! | 文件 | 职责 |
//! |---|---|
//! | `mod.rs` | 常量（文件名 / 键 / 参数 / 默认值）+ re-export + 集成测试 |
//! | `effective.rs` | 生效值 `Effective`：安装计算与配置读回、端口校验、单位换算 |
//! | `schema.rs` | `ConfigLifecycle` / `FieldSchema` 实现 |
//! | `generate.rs` | 生成落盘（`write_config`）与监听串构建 |
//! | `read.rs` | 精确读取、监听串解析与路径解析 |

mod effective;
mod generate;
mod read;
mod schema;

pub use read::{configured_log_dir, read_broker};

pub(super) const F_PROPS: &str = "server.properties";
/// Kafka 没有官方环境文件（`bin/kafka-run-class.sh` 只认环境变量 JAVA_HOME），
/// 故由 SoloStack 生成一个，启动时读它注入 JAVA_HOME。见 docs/Config-File-Design.md §7.3。
pub(super) const F_ENV: &str = "solostack-env.sh";

pub(super) const BROKER_DEFAULT: u16 = 9092;

// 安装参数 id（组件自己声明的参数表；前端提交同名键）
const P_BROKER_PORT: &str = "broker_port";
const P_NUM_PARTITIONS: &str = "num_partitions";
const P_RETENTION_HOURS: &str = "retention_hours";
const P_MESSAGE_MAX_MB: &str = "message_max_mb";
const P_AUTO_CREATE: &str = "auto_create_topics";

// 配置键
const K_ROLES: &str = "process.roles";
const K_NODE_ID: &str = "node.id";
/// 4.x 的 quorum 引导地址（旧键 `controller.quorum.voters` 已被 KIP-853 取代）。
const K_BOOTSTRAP: &str = "controller.quorum.bootstrap.servers";
const K_LISTENERS: &str = "listeners";
const K_ADVERTISED: &str = "advertised.listeners";
const K_INTER_BROKER: &str = "inter.broker.listener.name";
const K_CONTROLLER_NAMES: &str = "controller.listener.names";
const K_LOG_DIRS: &str = "log.dirs";
const K_NUM_PARTITIONS: &str = "num.partitions";
const K_OFFSETS_RF: &str = "offsets.topic.replication.factor";
const K_TXN_RF: &str = "transaction.state.log.replication.factor";
const K_TXN_MIN_ISR: &str = "transaction.state.log.min.isr";
const K_RETENTION_HOURS: &str = "log.retention.hours";
const K_MESSAGE_MAX_BYTES: &str = "message.max.bytes";
const K_AUTO_CREATE: &str = "auto.create.topics.enable";

/// Kafka 官方默认 `message.max.bytes`（1 MB + 12 字节开销）。
const MESSAGE_MAX_DEFAULT: u64 = 1_048_588;

#[cfg(test)]
mod tests {
    use super::super::{Kafka, CONTROLLER_PORT, NAME};
    use super::effective::{mb_from_bytes, mb_to_bytes, Effective};
    use super::generate::write_config;
    use super::read::{broker_port, configured_log_dir, managed_log_dir, replace_broker_port};
    use super::*;
    use crate::component::{self, ConfigFieldUpdate, ConfigLifecycle, FieldSchema, InstallParams};

    const ENV_ID: &str = "Env00001";

    fn save_field(id: &str, value: &str) -> Result<(), String> {
        let updates = [ConfigFieldUpdate {
            id: id.to_string(),
            value: value.to_string(),
        }];
        let plan = Kafka.plan_field_updates(ENV_ID, "4.3.1", &updates)?;
        crate::config::apply_plan(&plan)
    }

    fn setup_instance(tmp: &std::path::Path) {
        std::env::set_var("HOME", tmp);
        let dir = component::config_io::config_dir(ENV_ID, NAME, "4.3.1").unwrap();
        std::fs::create_dir_all(&dir).unwrap();
        // 模拟官方模板（4.x 同形）：带注释 + 官方默认值（quorum 用 bootstrap.servers）
        std::fs::write(
            dir.join(F_PROPS),
            "# Licensed to the Apache Software Foundation\n\
             process.roles=broker,controller\n\
             node.id=1\n\
             controller.quorum.bootstrap.servers=localhost:9093\n\
             listeners=PLAINTEXT://:9092,CONTROLLER://:9093\n\
             advertised.listeners=PLAINTEXT://localhost:9092,CONTROLLER://localhost:9093\n\
             log.dirs=/tmp/kraft-combined-logs\n\
             num.partitions=1\n\
             log.retention.hours=168\n\
             message.max.bytes=1048588\n\
             auto.create.topics.enable=true\n",
        )
        .unwrap();
    }

    /// 安装参数：全部走默认值（等价于前端什么都没改）。
    fn install_params() -> InstallParams {
        InstallParams::new()
    }

    /// 契约：`install_params` 声明的默认值必须与「空参数集安装」实际采用的一致。
    #[test]
    fn declared_install_params_match_applied_defaults() {
        let _guard = crate::test_util::HOME_LOCK.lock().unwrap();
        let tmp = std::env::temp_dir().join("solostack-kafka-install-params");
        let _ = std::fs::remove_dir_all(&tmp);
        setup_instance(&tmp);

        let declared: std::collections::HashMap<String, String> = Kafka
            .install_params("4.3.1")
            .into_iter()
            .map(|p| (p.id, p.default))
            .collect();
        assert_eq!(declared[P_BROKER_PORT], BROKER_DEFAULT.to_string());
        assert_eq!(declared[P_NUM_PARTITIONS], "1");
        assert_eq!(declared[P_RETENTION_HOURS], "168");
        assert_eq!(declared[P_MESSAGE_MAX_MB], "1");
        assert_eq!(declared[P_AUTO_CREATE], "true");

        Kafka
            .apply_install_config(ENV_ID, "4.3.1", &InstallParams::new())
            .unwrap();
        let eff = Effective::from_config(ENV_ID, "4.3.1");
        assert!(eff.broker >= BROKER_DEFAULT, "端口可能因占用避让而前移");
        assert_eq!(eff.num_partitions, 1);
        assert_eq!(eff.retention_hours, 168);
        assert_eq!(eff.message_max_mb, 1);
        assert!(eff.auto_create_topics);

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn broker_port_is_read_back_exactly() {
        let _guard = crate::test_util::HOME_LOCK.lock().unwrap();
        let tmp = std::env::temp_dir().join("solostack-kafka-roundtrip");
        let _ = std::fs::remove_dir_all(&tmp);
        setup_instance(&tmp);

        let eff = Effective::from_install(&install_params()).unwrap();
        write_config(ENV_ID, "4.3.1", &eff).unwrap();
        assert_eq!(Kafka.detect_ports(ENV_ID, "4.3.1"), vec![eff.broker]);
        assert_eq!(Effective::from_config(ENV_ID, "4.3.1"), eff);

        // 改端口：写入 listeners 并能精确读回
        save_field("broker_port", "9095").unwrap();
        assert_eq!(Kafka.detect_ports(ENV_ID, "4.3.1"), vec![9095]);

        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// 回归：`log.dirs` 必须写成 SoloStack 受管路径，绝不把官方模板的
    /// `/tmp/kraft-combined-logs` 占位值回声回去（否则数据与格式化标记都留在 /tmp，
    /// 被系统清理后下次启动会重新格式化并丢掉 topic）。
    #[test]
    fn log_dirs_is_written_to_managed_path() {
        let _guard = crate::test_util::HOME_LOCK.lock().unwrap();
        let tmp = std::env::temp_dir().join("solostack-kafka-logdirs");
        let _ = std::fs::remove_dir_all(&tmp);
        setup_instance(&tmp);

        write_config(
            ENV_ID,
            "4.3.1",
            &Effective::from_install(&install_params()).unwrap(),
        )
        .unwrap();

        let managed = managed_log_dir(ENV_ID, "4.3.1").unwrap();
        assert!(
            managed.ends_with("var/data/kafka/kafka-4.3.1/kafka"),
            "受管路径应在 var/data 下，实际 {managed:?}"
        );
        let conf = component::config_io::config_path(ENV_ID, NAME, "4.3.1", F_PROPS).unwrap();
        let content = std::fs::read_to_string(&conf).unwrap();
        assert!(
            content.contains(&format!("log.dirs={}", managed.display())),
            "log.dirs 应写成受管路径，实际配置：{}",
            content
                .lines()
                .find(|l| l.starts_with("log.dirs"))
                .unwrap_or("")
        );
        assert!(
            !content.contains("/tmp/kraft-combined-logs"),
            "不应保留官方模板的 /tmp 占位值"
        );

        // 手改配置后：判断跟着配置走（否则会每次启动都重复格式化）
        let custom = tmp.join("custom-logs");
        let f = component::config_io::open_config(ENV_ID, NAME, "4.3.1", F_PROPS).unwrap();
        f.set(K_LOG_DIRS, &custom.display().to_string()).unwrap();
        assert_eq!(configured_log_dir(ENV_ID, "4.3.1").unwrap(), custom);
        assert_eq!(
            managed_log_dir(ENV_ID, "4.3.1").unwrap(),
            managed,
            "受管路径是稳定的，不随配置变化"
        );

        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// broker 端口不得等于内置 controller 端口（否则同一端口绑两个监听器，起不来）；
    /// 端口 0 与特权端口由 parse_port 统一拒绝。
    #[test]
    fn broker_port_is_validated() {
        let _guard = crate::test_util::HOME_LOCK.lock().unwrap();
        let tmp = std::env::temp_dir().join("solostack-kafka-port-check");
        let _ = std::fs::remove_dir_all(&tmp);
        setup_instance(&tmp);
        write_config(
            ENV_ID,
            "4.3.1",
            &Effective::from_install(&install_params()).unwrap(),
        )
        .unwrap();

        // 安装期：显式填 controller 端口 → 报错
        let bad = InstallParams::from([(P_BROKER_PORT.to_string(), CONTROLLER_PORT.to_string())]);
        let err = Effective::from_install(&bad).unwrap_err();
        assert!(err.contains("controller"), "实际报错: {err}");

        // 配置页：同样拦下
        let err = save_field("broker_port", &CONTROLLER_PORT.to_string()).unwrap_err();
        assert!(err.contains("controller"), "实际报错: {err}");

        // 0 与特权端口被拒
        assert!(save_field("broker_port", "0").is_err());
        assert!(save_field("broker_port", "80").is_err());
        // 端口未被改动
        assert_eq!(Kafka.detect_ports(ENV_ID, "4.3.1"), vec![BROKER_DEFAULT]);

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn official_template_comments_survive() {
        let _guard = crate::test_util::HOME_LOCK.lock().unwrap();
        let tmp = std::env::temp_dir().join("solostack-kafka-comments");
        let _ = std::fs::remove_dir_all(&tmp);
        setup_instance(&tmp);

        write_config(
            ENV_ID,
            "4.3.1",
            &Effective::from_install(&install_params()).unwrap(),
        )
        .unwrap();
        let content = std::fs::read_to_string(
            component::config_io::config_dir(ENV_ID, NAME, "4.3.1")
                .unwrap()
                .join(F_PROPS),
        )
        .unwrap();
        assert!(
            content.contains("# Licensed to the Apache Software Foundation"),
            "官方模板注释必须保留（这是改成合并写的理由）"
        );
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn broker_port_change_keeps_advertised_in_sync() {
        let _guard = crate::test_util::HOME_LOCK.lock().unwrap();
        let tmp = std::env::temp_dir().join("solostack-kafka-advertised");
        let _ = std::fs::remove_dir_all(&tmp);
        setup_instance(&tmp);

        write_config(
            ENV_ID,
            "4.3.1",
            &Effective::from_install(&install_params()).unwrap(),
        )
        .unwrap();
        save_field("broker_port", "9095").unwrap();

        let content = std::fs::read_to_string(
            component::config_io::config_dir(ENV_ID, NAME, "4.3.1")
                .unwrap()
                .join(F_PROPS),
        )
        .unwrap();
        assert!(
            content.contains(
                "advertised.listeners=PLAINTEXT://localhost:9095,CONTROLLER://localhost:9093"
            ),
            "advertised.listeners 必须跟着 broker 端口一起改，否则客户端会连旧端口"
        );
        assert!(
            !content.contains("controller.quorum.voters"),
            "不应写 4.x 已取代的旧键"
        );
        assert!(content.contains("controller.quorum.bootstrap.servers=localhost:9093"));

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn java_env_file_is_solo_managed_shell_file() {
        let _guard = crate::test_util::HOME_LOCK.lock().unwrap();
        let tmp = std::env::temp_dir().join("solostack-kafka-env");
        let _ = std::fs::remove_dir_all(&tmp);
        setup_instance(&tmp);

        assert_eq!(
            Kafka.java_env_file(),
            Some(F_ENV),
            "Kafka 应由 SoloStack 生成 env 文件"
        );
        // 写入 JAVA_HOME 后能被通用链路精确读回（启动注入走的正是这条路径）
        let f = component::config_io::open_config(ENV_ID, NAME, "4.3.1", F_ENV).unwrap();
        f.set("JAVA_HOME", "/opt/jdk-17").unwrap();
        assert_eq!(
            crate::component::fields::read_java_home(ENV_ID, &Kafka, "4.3.1").as_deref(),
            Some("/opt/jdk-17")
        );

        let content = std::fs::read_to_string(
            component::config_io::config_dir(ENV_ID, NAME, "4.3.1")
                .unwrap()
                .join(F_ENV),
        )
        .unwrap();
        assert!(
            content.starts_with("# SoloStack:begin"),
            "生成的 env 文件应带受管说明头"
        );
        assert!(content.contains("export JAVA_HOME=/opt/jdk-17"));

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn listeners_helpers_keep_other_listeners() {
        assert_eq!(
            broker_port("PLAINTEXT://:9092,CONTROLLER://:9093"),
            Some("9092")
        );
        assert_eq!(
            replace_broker_port("PLAINTEXT://:9092,CONTROLLER://:9093", 9099),
            "PLAINTEXT://:9099,CONTROLLER://:9093"
        );
        // 占位 listener 不应被误改
        assert_eq!(
            replace_broker_port("CONTROLLER://:9093,PLAINTEXT://:9092", 9099),
            "CONTROLLER://:9093,PLAINTEXT://:9099"
        );
        // host 非空的形式（advertised.listeners 的官方写法）同样要能读能改
        assert_eq!(
            broker_port("PLAINTEXT://localhost:9092,CONTROLLER://localhost:9093"),
            Some("9092")
        );
        assert_eq!(
            replace_broker_port(
                "PLAINTEXT://localhost:9092,CONTROLLER://localhost:9093",
                9095
            ),
            "PLAINTEXT://localhost:9095,CONTROLLER://localhost:9093"
        );
        // 无端口定义时不乱改
        assert_eq!(
            replace_broker_port("PLAINTEXT://localhost", 9095),
            "PLAINTEXT://localhost"
        );
        assert_eq!(broker_port("PLAINTEXT://localhost"), None);
    }

    #[test]
    fn message_size_converts_both_ways() {
        assert_eq!(mb_to_bytes(1), MESSAGE_MAX_DEFAULT);
        assert_eq!(mb_to_bytes(10), 10 * 1_048_576);
        assert_eq!(mb_from_bytes(MESSAGE_MAX_DEFAULT), 1);
        assert_eq!(mb_from_bytes(10 * 1_048_576), 10);
    }
}
