//! Kafka 的运行期：首启 KRaft 格式化、启停序列。
//! Kafka 无 WebUI（用 `Runtime` 的默认空实现）。

use std::time::Duration;

use super::config::{read_broker, BROKER_DEFAULT, F_PROPS};
use super::{Kafka, NAME};
use crate::app::paths;
use crate::component::exec;
use crate::component::{self, Runtime};
use crate::platform::process::ServiceSpec;

const COMMAND_TIMEOUT: Duration = Duration::from_secs(60);
const FORMAT_TIMEOUT: Duration = Duration::from_secs(300);

impl Runtime for Kafka {
    fn init(&self, environment_id: &str, version: &str) -> Result<(), String> {
        format_storage_if_needed(environment_id, version)?;
        Ok(())
    }

    fn start(&self, environment_id: &str, version: &str) -> Result<(), String> {
        let conf = component::config_path(environment_id, NAME, version, F_PROPS)?;
        exec::run_checked(
            environment_id,
            &Kafka,
            version,
            "bin/kafka-server-start.sh",
            &["-daemon", conf.to_str().unwrap_or_default()],
            &[],
            COMMAND_TIMEOUT,
        )?;
        Ok(())
    }

    fn stop(&self, environment_id: &str, version: &str) -> Result<(), String> {
        exec::run_checked(
            environment_id,
            &Kafka,
            version,
            "bin/kafka-server-stop.sh",
            &[],
            &[],
            COMMAND_TIMEOUT,
        )?;
        Ok(())
    }

    fn service_specs(&self, environment_id: &str, version: &str) -> Vec<ServiceSpec> {
        let root = paths::instance_dir(environment_id, NAME, version)
            .map(|path| path.display().to_string())
            .unwrap_or_else(|_| NAME.to_string());
        let needles = vec![root, "kafka.Kafka".to_string()];
        let broker = read_broker(environment_id, version).unwrap_or(BROKER_DEFAULT);
        vec![
            ServiceSpec::new(environment_id, "broker", needles.clone(), [broker]),
            ServiceSpec::new(
                environment_id,
                "controller",
                needles,
                [super::CONTROLLER_PORT],
            ),
        ]
    }
}

/// 首次启动前格式化 KRaft 存储目录（幂等，与 hadoop 的 `namenode -format` 同一套路）。
///
/// - **幂等信号用官方产物**：格式化完成后官方会在 `log.dirs` 写下 `meta.properties`，
///   它存在即已初始化 —— 不需要我们自己造标记文件（hadoop 同样改用官方产物判断）。
/// - **必须带 `--standalone`**：组合模式（`process.roles=broker,controller`）单节点若
///   没配 `controller.quorum.voters`，StorageTool 会因「未指定初始 quorum」直接报错，
///   必须由 `--standalone` 显式声明自举（见 Kafka `StorageTool.scala`）。
fn format_storage_if_needed(environment_id: &str, version: &str) -> Result<(), String> {
    // 判断用的是「配置里 log.dirs 实际指向的位置」，而非受管默认路径：
    // 两者不一致时若按默认路径判断，会每次启动都尝试格式化同一目录
    let log_dir = super::config::configured_log_dir(environment_id, version)?;
    if log_dir.join("meta.properties").exists() {
        return Ok(());
    }

    let conf = component::config_path(environment_id, NAME, version, F_PROPS)?;
    println!("首次使用，格式化 Kafka KRaft 存储...");
    let _ = crate::app::app_log::info(&format!(
        "首次启动 {NAME} v{version}，格式化 KRaft 存储目录"
    ));

    let cluster_id = exec::run_to_string(
        environment_id,
        &Kafka,
        version,
        "bin/kafka-storage.sh",
        &["random-uuid"],
        &[],
        FORMAT_TIMEOUT,
    )?;
    let cluster_id = cluster_id.trim();
    if cluster_id.is_empty() {
        return Err(
            "获取 Kafka cluster id 失败（kafka-storage.sh random-uuid 无输出）".to_string(),
        );
    }

    exec::run_to_string(
        environment_id,
        &Kafka,
        version,
        "bin/kafka-storage.sh",
        &[
            "format",
            "--standalone",
            "-t",
            cluster_id,
            "-c",
            conf.to_str().unwrap_or_default(),
        ],
        &[],
        FORMAT_TIMEOUT,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const ENV_ID: &str = "00000000-0000-4000-8000-000000000001";

    fn setup_fake_instance(tmp: &std::path::Path) {
        std::env::set_var("HOME", tmp);
        let instance = crate::app::paths::instance_dir(ENV_ID, NAME, "4.3.1").unwrap();
        let config = component::config_dir(ENV_ID, NAME, "4.3.1").unwrap();
        let bin = instance.join("bin");
        let jdk = tmp.join("fake-jdk");
        let log_dir = super::super::config::managed_log_dir(ENV_ID, "4.3.1").unwrap();

        std::fs::create_dir_all(&config).unwrap();
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::create_dir_all(&jdk).unwrap();
        std::fs::create_dir_all(&log_dir).unwrap();
        std::fs::write(log_dir.join("meta.properties"), "version=1\n").unwrap();
        std::fs::write(
            config.join("solostack-env.sh"),
            format!("export JAVA_HOME={}\n", jdk.display()),
        )
        .unwrap();
        std::fs::write(
            bin.join("kafka-server-start.sh"),
            "printf 'start stdout\\n'\nprintf 'start failed\\n' >&2\nexit 7\n",
        )
        .unwrap();
        std::fs::write(
            bin.join("kafka-server-stop.sh"),
            "printf 'stop stdout\\n'\nprintf 'stop failed\\n' >&2\nexit 9\n",
        )
        .unwrap();
    }

    #[test]
    fn format_is_skipped_when_meta_properties_exists() {
        use crate::test_util::HOME_LOCK;
        let _guard = HOME_LOCK.lock().unwrap();
        let tmp = std::env::temp_dir().join("solostack-kafka-format-skip");
        std::env::set_var("HOME", &tmp);
        let _ = std::fs::remove_dir_all(&tmp);

        // 官方产物 meta.properties 存在 → 视为已格式化，不执行任何脚本
        // （此环境下 kafka 二进制并不存在，能返回 Ok 就证明它真的没跑脚本）
        let log_dir = super::super::config::managed_log_dir(ENV_ID, "4.3.1").unwrap();
        std::fs::create_dir_all(&log_dir).unwrap();
        std::fs::write(log_dir.join("meta.properties"), "version=1\n").unwrap();
        assert!(format_storage_if_needed(ENV_ID, "4.3.1").is_ok());

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn start_and_stop_return_script_failure_output() {
        use crate::test_util::HOME_LOCK;
        let _guard = HOME_LOCK.lock().unwrap();
        let tmp = std::env::temp_dir().join("solostack-kafka-runtime-failure");
        let _ = std::fs::remove_dir_all(&tmp);
        setup_fake_instance(&tmp);

        let start_err = Kafka.start(ENV_ID, "4.3.1").unwrap_err();
        assert!(start_err.contains("执行失败"), "{start_err}");
        assert!(start_err.contains("start stdout"), "{start_err}");
        assert!(start_err.contains("start failed"), "{start_err}");

        let stop_err = Kafka.stop(ENV_ID, "4.3.1").unwrap_err();
        assert!(stop_err.contains("执行失败"), "{stop_err}");
        assert!(stop_err.contains("stop stdout"), "{stop_err}");
        assert!(stop_err.contains("stop failed"), "{stop_err}");

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn service_specs_share_combined_kafka_process() {
        let specs = Kafka.service_specs(ENV_ID, "4.3.1");
        let keys: Vec<&str> = specs.iter().map(|spec| spec.key).collect();
        assert_eq!(keys, vec!["broker", "controller"]);
        assert_eq!(specs[0].process_needles, specs[1].process_needles);
        assert!(specs.iter().all(|spec| spec
            .process_needles
            .iter()
            .any(|needle| needle == "kafka.Kafka")));
    }
}
