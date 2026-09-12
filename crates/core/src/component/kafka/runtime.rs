//! Kafka 的运行期：首启 KRaft 格式化、启停序列。
//! Kafka 无 WebUI（用 `Runtime` 的默认空实现）。

use super::config::F_PROPS;
use super::{Kafka, NAME};
use crate::component::exec;
use crate::component::{self, Runtime};

impl Runtime for Kafka {
    fn start(&self, version: &str) -> Result<(), String> {
        let _ = crate::app::app_log::append(
            crate::app::app_log::INFO,
            &format!("启动组件 {NAME} v{version}"),
        );
        format_storage_if_needed(version)?;
        let conf = component::config_path(NAME, version, F_PROPS)?;
        exec::run_script(
            &Kafka,
            version,
            "bin/kafka-server-start.sh",
            &["-daemon", conf.to_str().unwrap_or_default()],
            &[],
        )?;
        Ok(())
    }

    fn stop(&self, version: &str) -> Result<(), String> {
        let _ = crate::app::app_log::append(
            crate::app::app_log::INFO,
            &format!("停止组件 {NAME} v{version}"),
        );
        exec::run_script(&Kafka, version, "bin/kafka-server-stop.sh", &[], &[])?;
        Ok(())
    }
}

/// 首次启动前格式化 KRaft 存储目录（幂等，与 hadoop 的 `namenode -format` 同一套路）。
///
/// - **幂等信号用官方产物**：格式化完成后官方会在 `log.dirs` 写下 `meta.properties`，
///   它存在即已初始化 —— 不需要我们自己造标记文件（hadoop 同样改用官方产物判断）。
/// - **必须带 `--standalone`**：组合模式（`process.roles=broker,controller`）单节点若
///   没配 `controller.quorum.voters`，StorageTool 会因「未指定初始 quorum」直接报错，
///   必须由 `--standalone` 显式声明自举（见 Kafka `StorageTool.scala`）。
fn format_storage_if_needed(version: &str) -> Result<(), String> {
    // 判断用的是「配置里 log.dirs 实际指向的位置」，而非受管默认路径：
    // 两者不一致时若按默认路径判断，会每次启动都尝试格式化同一目录
    let log_dir = super::config::configured_log_dir(version)?;
    if log_dir.join("meta.properties").exists() {
        return Ok(());
    }

    let conf = component::config_path(NAME, version, F_PROPS)?;
    println!("首次使用，格式化 Kafka KRaft 存储...");
    let _ = crate::app::app_log::append(
        crate::app::app_log::INFO,
        &format!("首次启动 {NAME} v{version}，格式化 KRaft 存储目录"),
    );

    let cluster_id = exec::run_to_string(
        &Kafka,
        version,
        "bin/kafka-storage.sh",
        &["random-uuid"],
        &[],
    )?;
    let cluster_id = cluster_id.trim();
    if cluster_id.is_empty() {
        return Err(
            "获取 Kafka cluster id 失败（kafka-storage.sh random-uuid 无输出）".to_string(),
        );
    }

    exec::run_to_string(
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
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_is_skipped_when_meta_properties_exists() {
        use crate::test_util::HOME_LOCK;
        let _guard = HOME_LOCK.lock().unwrap();
        let tmp = std::env::temp_dir().join("solostack-kafka-format-skip");
        std::env::set_var("HOME", &tmp);
        let _ = std::fs::remove_dir_all(&tmp);

        // 官方产物 meta.properties 存在 → 视为已格式化，不执行任何脚本
        // （此环境下 kafka 二进制并不存在，能返回 Ok 就证明它真的没跑脚本）
        let log_dir = super::super::config::managed_log_dir("4.3.1").unwrap();
        std::fs::create_dir_all(&log_dir).unwrap();
        std::fs::write(log_dir.join("meta.properties"), "version=1\n").unwrap();
        assert!(format_storage_if_needed("4.3.1").is_ok());

        let _ = std::fs::remove_dir_all(&tmp);
    }
}
