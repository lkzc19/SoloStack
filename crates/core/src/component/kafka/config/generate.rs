//! Kafka 配置生成：把生效配置合并写进官方 server.properties。

use super::effective::{ensure_broker_port_ok, mb_to_bytes, Effective};
use super::read::managed_log_dir;
use super::super::{CONTROLLER_PORT, NAME, NODE_ID};
use super::{
    F_PROPS, K_ADVERTISED, K_AUTO_CREATE, K_BOOTSTRAP, K_CONTROLLER_NAMES, K_INTER_BROKER,
    K_LISTENERS, K_LOG_DIRS, K_MESSAGE_MAX_BYTES, K_NODE_ID, K_NUM_PARTITIONS, K_OFFSETS_RF,
    K_RETENTION_HOURS, K_ROLES, K_TXN_MIN_ISR, K_TXN_RF,
};
use crate::app::paths;
use crate::component;
use crate::config::ConfigPlan;

/// 把生效配置合并写进官方 server.properties（含 KRaft 必需项）。
pub(super) fn write_config(
    environment_id: &str,
    version: &str,
    eff: &Effective,
) -> Result<(), String> {
    ensure_broker_port_ok(eff.broker)?;
    let data_root =
        paths::var_data_instance_dir(environment_id, NAME, version).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&data_root).map_err(|e| e.to_string())?;
    let path = component::config_io::config_path(environment_id, NAME, version, F_PROPS)?;
    let mut plan = ConfigPlan::new();

    let entries = [
        (K_ROLES, "broker,controller".to_string()),
        (K_NODE_ID, NODE_ID.to_string()),
        (K_BOOTSTRAP, format!("localhost:{CONTROLLER_PORT}")),
        (K_LISTENERS, listeners(eff.broker)),
        (K_ADVERTISED, advertised_listeners(eff.broker)),
        (K_INTER_BROKER, "PLAINTEXT".to_string()),
        (K_CONTROLLER_NAMES, "CONTROLLER".to_string()),
        (
            K_LOG_DIRS,
            managed_log_dir(environment_id, version)?
                .display()
                .to_string(),
        ),
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

/// 监听地址（`host` 留空 = 监听所有网卡）。
pub(super) fn listeners(broker: u16) -> String {
    format!("PLAINTEXT://:{broker},CONTROLLER://:{CONTROLLER_PORT}")
}

/// 对外公布的地址：必须与 `listeners` 的端口保持一致，否则改端口后客户端会被
/// 告知连旧端口。本地单机固定用 localhost。
pub(super) fn advertised_listeners(broker: u16) -> String {
    format!("PLAINTEXT://localhost:{broker},CONTROLLER://localhost:{CONTROLLER_PORT}")
}
