//! Kafka 的配置布局、生效值与字段读写。

use super::{Kafka, CONTROLLER_PORT, NAME, NODE_ID};
use crate::app::paths;
use crate::component::fields::{param_bool, param_port, param_positive_int};
use crate::component::ports;
use crate::component::{
    self, ConfigFieldUpdate, ConfigFieldValue, ConfigLayout, ConfigLifecycle, FieldSchema,
    InstallParam, InstallParams,
};
use crate::config::ConfigPlan;

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

/// kafka 的生效配置：安装时由安装选项算出，启动前由配置文件精确读回。
#[derive(Debug, Clone, PartialEq, Eq)]
struct Effective {
    broker: u16,
    num_partitions: u64,
    retention_hours: u64,
    message_max_mb: u64,
    auto_create_topics: bool,
}

impl Effective {
    /// 安装时：参数缺省（前端未提交）时全部走默认值，与 `install_params` 声明一致。
    fn from_install(params: &InstallParams) -> Result<Self, String> {
        // 避让只作用于「默认端口被占/撞保留端口」；用户显式指定的端口原样采纳，
        // 是否合法由 ensure_broker_port_ok 明确拦下（不静默改用户填的值）。
        let broker = ports::pick_free_excluding(
            param_port(params, P_BROKER_PORT, BROKER_DEFAULT)?,
            BROKER_DEFAULT,
            &[CONTROLLER_PORT],
        );
        ensure_broker_port_ok(broker)?;
        Ok(Effective {
            broker,
            num_partitions: param_positive_int(params, P_NUM_PARTITIONS, 1, "默认分区数")?,
            retention_hours: param_positive_int(params, P_RETENTION_HOURS, 168, "消息保留时长")?,
            message_max_mb: param_positive_int(params, P_MESSAGE_MAX_MB, 1, "单条消息上限")?,
            auto_create_topics: param_bool(params, P_AUTO_CREATE, true)?,
        })
    }

    /// 启动前 / 配置页：从配置文件精确读回，单项缺失用默认值兜底。
    ///
    /// server.properties **只读一次**：该路径在状态轮询里高频执行，逐键读盘代价高。
    fn from_config(version: &str) -> Self {
        let props = read_all(version);
        let num = |key: &str, default: u64| {
            props
                .get(key)
                .and_then(|v| v.trim().parse::<u64>().ok())
                .unwrap_or(default)
        };
        Effective {
            broker: props
                .get(K_LISTENERS)
                .and_then(|v| broker_port(v))
                .and_then(|p| p.parse::<u16>().ok())
                .unwrap_or(BROKER_DEFAULT),
            num_partitions: num(K_NUM_PARTITIONS, 1),
            retention_hours: num(K_RETENTION_HOURS, 168),
            message_max_mb: mb_from_bytes(num(K_MESSAGE_MAX_BYTES, MESSAGE_MAX_DEFAULT)),
            auto_create_topics: props.get(K_AUTO_CREATE).map(|v| v.trim()) != Some("false"),
        }
    }
}

impl ConfigLifecycle for Kafka {
    fn component(&self) -> &'static str {
        NAME
    }

    fn config_layout(&self) -> ConfigLayout {
        ConfigLayout {
            // Kafka 4.x 起 KRaft 是唯一模式，官方已把配置铺平在 config/ 下
            dir: "config",
            files: &[F_PROPS],
        }
    }

    fn detect_ports(&self, version: &str) -> Vec<u16> {
        vec![read_broker(version).unwrap_or(BROKER_DEFAULT)]
    }

    fn install_params(&self, _version: &str) -> Vec<InstallParam> {
        vec![
            InstallParam::new(P_BROKER_PORT, BROKER_DEFAULT),
            InstallParam::new(P_NUM_PARTITIONS, 1),
            InstallParam::new(P_RETENTION_HOURS, 168),
            InstallParam::new(P_MESSAGE_MAX_MB, 1),
            InstallParam::new(P_AUTO_CREATE, true),
        ]
    }

    fn apply_install_config(&self, version: &str, params: &InstallParams) -> Result<(), String> {
        write_config(version, &Effective::from_install(params)?)
    }

    fn ensure_config(&self, version: &str) -> Result<(), String> {
        component::validate_layout(NAME, version, &self.config_layout())?;
        write_config(version, &Effective::from_config(version))
    }

    fn java_env_file(&self) -> Option<&'static str> {
        Some(F_ENV)
    }
}

impl FieldSchema for Kafka {
    fn field_values(&self, version: &str) -> Vec<ConfigFieldValue> {
        let eff = Effective::from_config(version);
        vec![
            ConfigFieldValue {
                id: "broker_port".into(),
                value: eff.broker.to_string(),
            },
            ConfigFieldValue {
                id: "num_partitions".into(),
                value: eff.num_partitions.to_string(),
            },
            ConfigFieldValue {
                id: "retention_hours".into(),
                value: eff.retention_hours.to_string(),
            },
            ConfigFieldValue {
                id: "message_max_mb".into(),
                value: eff.message_max_mb.to_string(),
            },
            ConfigFieldValue {
                id: "auto_create_topics".into(),
                value: if eff.auto_create_topics {
                    "true".into()
                } else {
                    "false".into()
                },
            },
        ]
    }

    fn plan_field_updates(
        &self,
        version: &str,
        updates: &[ConfigFieldUpdate],
    ) -> Result<ConfigPlan, String> {
        let mut next = Effective::from_config(version);
        let mut broker_port = None;
        let mut num_partitions = None;
        let mut retention_hours = None;
        let mut message_max_mb = None;
        let mut auto_create_topics = None;

        for update in updates {
            match update.id.as_str() {
                "broker_port" => {
                    let port = component::fields::parse_port(&update.value)?;
                    ensure_broker_port_ok(port)?;
                    broker_port = Some(port);
                    next.broker = port;
                }
                "num_partitions" => {
                    let value = component::fields::parse_positive_int(&update.value, "默认分区数")?;
                    if value > 100_000 {
                        return Err("默认分区数过大".to_string());
                    }
                    num_partitions = Some(value);
                    next.num_partitions = value;
                }
                "retention_hours" => {
                    let value =
                        component::fields::parse_positive_int(&update.value, "消息保留时长")?;
                    retention_hours = Some(value);
                    next.retention_hours = value;
                }
                "message_max_mb" => {
                    let value =
                        component::fields::parse_positive_int(&update.value, "单条消息上限")?;
                    message_max_mb = Some(value);
                    next.message_max_mb = value;
                }
                "auto_create_topics" => {
                    let value = match update.value.trim() {
                        "true" => true,
                        "false" => false,
                        _ => return Err("自动创建 Topic 取值无效".to_string()),
                    };
                    auto_create_topics = Some(value);
                    next.auto_create_topics = value;
                }
                _ => return Err(format!("未知配置字段: {}", update.id)),
            }
        }
        ensure_broker_port_ok(next.broker)?;

        let path = component::config_path(NAME, version, F_PROPS)?;
        let mut plan = ConfigPlan::new();
        if let Some(port) = broker_port {
            let current_listeners = read_prop(version, K_LISTENERS, &listeners(BROKER_DEFAULT));
            let current_advertised =
                read_prop(version, K_ADVERTISED, &advertised_listeners(BROKER_DEFAULT));
            plan.set(
                path.clone(),
                K_LISTENERS,
                replace_broker_port(&current_listeners, port),
            )?;
            plan.set(
                path.clone(),
                K_ADVERTISED,
                replace_broker_port(&current_advertised, port),
            )?;
        }
        if let Some(value) = num_partitions {
            plan.set(path.clone(), K_NUM_PARTITIONS, value.to_string())?;
        }
        if let Some(value) = retention_hours {
            plan.set(path.clone(), K_RETENTION_HOURS, value.to_string())?;
        }
        if let Some(value) = message_max_mb {
            plan.set(
                path.clone(),
                K_MESSAGE_MAX_BYTES,
                mb_to_bytes(value).to_string(),
            )?;
        }
        if let Some(value) = auto_create_topics {
            plan.set(path, K_AUTO_CREATE, value.to_string())?;
        }
        Ok(plan)
    }
}

// ── 配置生成与读写 ──────────────────────────────────────

/// 校验 broker 端口不与内置 controller 端口相同。
///
/// 组合模式（broker+controller）会把两个监听器写进同一个 `listeners`，
/// 撞端口时 Kafka 起不来且报错含糊，故在这里指名道姓地拦下。
fn ensure_broker_port_ok(broker: u16) -> Result<(), String> {
    ports::ensure_distinct(&[
        ("Broker 端口", broker),
        ("内置 controller 端口", CONTROLLER_PORT),
    ])
}

/// 把生效配置合并写进官方 server.properties（含 KRaft 必需项）。
fn write_config(version: &str, eff: &Effective) -> Result<(), String> {
    ensure_broker_port_ok(eff.broker)?;
    let data_root = paths::var_data_instance_dir(NAME, version).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&data_root).map_err(|e| e.to_string())?;
    let path = component::config_path(NAME, version, F_PROPS)?;
    let mut plan = ConfigPlan::new();

    let entries = [
        (K_ROLES, "broker,controller".to_string()),
        (K_NODE_ID, NODE_ID.to_string()),
        (K_BOOTSTRAP, format!("localhost:{CONTROLLER_PORT}")),
        (K_LISTENERS, listeners(eff.broker)),
        (K_ADVERTISED, advertised_listeners(eff.broker)),
        (K_INTER_BROKER, "PLAINTEXT".to_string()),
        (K_CONTROLLER_NAMES, "CONTROLLER".to_string()),
        (K_LOG_DIRS, managed_log_dir(version)?.display().to_string()),
        (K_NUM_PARTITIONS, eff.num_partitions.to_string()),
        (K_OFFSETS_RF, "1".to_string()),
        (K_TXN_RF, "1".to_string()),
        (K_TXN_MIN_ISR, "1".to_string()),
        (K_RETENTION_HOURS, eff.retention_hours.to_string()),
        (
            K_MESSAGE_MAX_BYTES,
            mb_to_bytes(eff.message_max_mb).to_string(),
        ),
        (K_AUTO_CREATE, eff.auto_create_topics.to_string()),
    ];
    for (key, value) in entries {
        plan.set(path.clone(), key, value)?;
    }
    crate::config::apply_plan(&plan)
}

fn read_prop(version: &str, key: &str, default: &str) -> String {
    component::open_config(NAME, version, F_PROPS)
        .and_then(|f| f.get(key))
        .ok()
        .flatten()
        .unwrap_or_else(|| default.to_string())
}

/// SoloStack 受管的 KRaft 日志目录（**写进配置的值**）。
///
/// 与 hadoop 同一条规则：数据目录键由 SoloStack 拥有，始终写入受管路径，
/// 绝不把官方模板的占位值（`/tmp/kraft-combined-logs`）回声回配置里 ——
/// 否则数据（以及格式化标记 `meta.properties`）会留在 /tmp，被系统清理后
/// 下次启动会重新格式化并丢掉 topic。
pub(super) fn managed_log_dir(version: &str) -> Result<std::path::PathBuf, String> {
    Ok(paths::var_data_instance_dir(NAME, version)
        .map_err(|e| e.to_string())?
        .join("kafka"))
}

/// KRaft 日志目录**实际所在位置**：从配置精确读 `log.dirs`（未设置则受管路径）。
/// 供「是否已格式化」判断使用，必须与 Kafka 实际落盘位置一致。
pub(super) fn configured_log_dir(version: &str) -> Result<std::path::PathBuf, String> {
    let fallback = managed_log_dir(version)?;
    let Some(raw) = read_raw_opt(version, K_LOG_DIRS) else {
        return Ok(fallback);
    };
    let base = paths::instance_dir(NAME, version).unwrap_or_else(|_| fallback.clone());
    Ok(crate::config::resolve_path(&raw, &base).unwrap_or(fallback))
}

/// 一次读入 server.properties 的全部键值（键 → 值）。
fn read_all(version: &str) -> std::collections::HashMap<String, String> {
    component::open_config(NAME, version, F_PROPS)
        .and_then(|f| f.read_entries())
        .map(|entries| {
            entries
                .into_iter()
                .map(|e| (e.key, e.value.trim().to_string()))
                .collect()
        })
        .unwrap_or_default()
}

/// 精确读配置项原始值（空值视为未设置）。
fn read_raw_opt(version: &str, key: &str) -> Option<String> {
    let f = component::open_config(NAME, version, F_PROPS).ok()?;
    f.get_trimmed(key).ok()?
}

/// broker 端口：从 `listeners` 里精确读。
pub(super) fn read_broker(version: &str) -> Option<u16> {
    let listeners = read_prop(version, K_LISTENERS, "");
    let port = broker_port(&listeners)?;
    port.parse().ok()
}

/// 监听地址（`host` 留空 = 监听所有网卡）。
fn listeners(broker: u16) -> String {
    format!("PLAINTEXT://:{broker},CONTROLLER://:{CONTROLLER_PORT}")
}

/// 对外公布的地址：必须与 `listeners` 的端口保持一致，否则改端口后客户端会被
/// 告知连旧端口。本地单机固定用 localhost。
fn advertised_listeners(broker: u16) -> String {
    format!("PLAINTEXT://localhost:{broker},CONTROLLER://localhost:{CONTROLLER_PORT}")
}

/// 取 PLAINTEXT 那条监听项的端口数字串。
///
/// host 可为空（`PLAINTEXT://:9092`，官方 `listeners` 的写法）也可非空
/// （`PLAINTEXT://localhost:9092`，官方 `advertised.listeners` 的写法），
/// 故一律按**最后一个冒号**定位端口，不能写死 `://:`。
fn broker_port(listeners: &str) -> Option<&str> {
    let text = listeners
        .split(',')
        .find(|l| l.trim_start().starts_with("PLAINTEXT"))?;
    let colon = text.rfind(':')?;
    let digits = &text[colon + 1..];
    let len = digits.chars().take_while(|c| c.is_ascii_digit()).count();
    (len > 0).then(|| &digits[..len])
}

/// 替换监听项里 PLAINTEXT 的端口（保留 host 部分与其余监听器定义）。
fn replace_broker_port(listeners: &str, port: u16) -> String {
    let parts: Vec<String> = listeners
        .split(',')
        .map(|l| {
            if !l.trim_start().starts_with("PLAINTEXT") {
                return l.to_string();
            }
            let Some(colon) = l.rfind(':') else {
                return l.to_string();
            };
            let digits = l[colon + 1..]
                .chars()
                .take_while(|c| c.is_ascii_digit())
                .count();
            if digits == 0 {
                return l.to_string();
            }
            format!("{}{}{}", &l[..colon + 1], port, &l[colon + 1 + digits..])
        })
        .collect();
    parts.join(",")
}

/// MB 档位 → 字节（1 MB 用 Kafka 官方默认值，含 12 字节消息头开销）。
fn mb_to_bytes(mb: u64) -> u64 {
    if mb <= 1 {
        MESSAGE_MAX_DEFAULT
    } else {
        mb * 1_048_576
    }
}

/// 字节 → MB 档位（向上取整，1 MB 以下归 1）。
fn mb_from_bytes(bytes: u64) -> u64 {
    let mb = (bytes + 524_288) / 1_048_576;
    mb.max(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn save_field(id: &str, value: &str) -> Result<(), String> {
        let updates = [ConfigFieldUpdate {
            id: id.to_string(),
            value: value.to_string(),
        }];
        let plan = Kafka.plan_field_updates("4.3.1", &updates)?;
        crate::config::apply_plan(&plan)
    }

    fn setup_instance(tmp: &std::path::Path) {
        std::env::set_var("HOME", tmp);
        let dir = component::config_dir(NAME, "4.3.1").unwrap();
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
            .apply_install_config("4.3.1", &InstallParams::new())
            .unwrap();
        let eff = Effective::from_config("4.3.1");
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
        write_config("4.3.1", &eff).unwrap();
        assert_eq!(Kafka.detect_ports("4.3.1"), vec![eff.broker]);
        assert_eq!(Effective::from_config("4.3.1"), eff);

        // 改端口：写入 listeners 并能精确读回
        save_field("broker_port", "9095").unwrap();
        assert_eq!(Kafka.detect_ports("4.3.1"), vec![9095]);

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
            "4.3.1",
            &Effective::from_install(&install_params()).unwrap(),
        )
        .unwrap();

        let managed = managed_log_dir("4.3.1").unwrap();
        assert!(
            managed.ends_with("var/data/kafka/kafka-4.3.1/kafka"),
            "受管路径应在 var/data 下，实际 {managed:?}"
        );
        let conf = component::config_path(NAME, "4.3.1", F_PROPS).unwrap();
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
        let f = component::open_config(NAME, "4.3.1", F_PROPS).unwrap();
        f.set(K_LOG_DIRS, &custom.display().to_string()).unwrap();
        assert_eq!(configured_log_dir("4.3.1").unwrap(), custom);
        assert_eq!(
            managed_log_dir("4.3.1").unwrap(),
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
        assert_eq!(Kafka.detect_ports("4.3.1"), vec![BROKER_DEFAULT]);

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn official_template_comments_survive() {
        let _guard = crate::test_util::HOME_LOCK.lock().unwrap();
        let tmp = std::env::temp_dir().join("solostack-kafka-comments");
        let _ = std::fs::remove_dir_all(&tmp);
        setup_instance(&tmp);

        write_config(
            "4.3.1",
            &Effective::from_install(&install_params()).unwrap(),
        )
        .unwrap();
        let content =
            std::fs::read_to_string(component::config_dir(NAME, "4.3.1").unwrap().join(F_PROPS))
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
            "4.3.1",
            &Effective::from_install(&install_params()).unwrap(),
        )
        .unwrap();
        save_field("broker_port", "9095").unwrap();

        let content =
            std::fs::read_to_string(component::config_dir(NAME, "4.3.1").unwrap().join(F_PROPS))
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
        let f = component::open_config(NAME, "4.3.1", F_ENV).unwrap();
        f.set("JAVA_HOME", "/opt/jdk-17").unwrap();
        assert_eq!(
            crate::component::fields::read_java_home(&Kafka, "4.3.1").as_deref(),
            Some("/opt/jdk-17")
        );

        let content =
            std::fs::read_to_string(component::config_dir(NAME, "4.3.1").unwrap().join(F_ENV))
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
