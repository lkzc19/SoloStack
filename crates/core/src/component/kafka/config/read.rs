//! Kafka 配置的精确读取、监听串解析与路径解析。

use std::path::PathBuf;

use super::super::NAME;
use super::{F_PROPS, K_LISTENERS, K_LOG_DIRS};
use crate::app::paths;
use crate::component;

/// 读一个配置项，未设置时用 `default`。
pub(super) fn read_prop(
    environment_id: &str,
    version: &str,
    key: &str,
    default: &str,
) -> String {
    component::config_io::open_config(environment_id, NAME, version, F_PROPS)
        .and_then(|f| f.get(key))
        .ok()
        .flatten()
        .unwrap_or_else(|| default.to_string())
}

/// 一次读入 server.properties 的全部键值（键 → 值）。
pub(super) fn read_all(
    environment_id: &str,
    version: &str,
) -> std::collections::HashMap<String, String> {
    component::config_io::open_config(environment_id, NAME, version, F_PROPS)
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
fn read_raw_opt(environment_id: &str, version: &str, key: &str) -> Option<String> {
    let f = component::config_io::open_config(environment_id, NAME, version, F_PROPS).ok()?;
    f.get_trimmed(key).ok()?
}

/// SoloStack 受管的 KRaft 日志目录（**写进配置的值**）。
///
/// 与 hadoop 同一条规则：数据目录键由 SoloStack 拥有，始终写入受管路径，
/// 绝不把官方模板的占位值（`/tmp/kraft-combined-logs`）回声回配置里 ——
/// 否则数据（以及格式化标记 `meta.properties`）会留在 /tmp，被系统清理后
/// 下次启动会重新格式化并丢掉 topic。
pub(super) fn managed_log_dir(
    environment_id: &str,
    version: &str,
) -> Result<PathBuf, String> {
    Ok(paths::var_data_instance_dir(environment_id, NAME, version)
        .map_err(|e| e.to_string())?
        .join("kafka"))
}

/// KRaft 日志目录**实际所在位置**：从配置精确读 `log.dirs`（未设置则受管路径）。
/// 供「是否已格式化」判断使用，必须与 Kafka 实际落盘位置一致。
pub fn configured_log_dir(
    environment_id: &str,
    version: &str,
) -> Result<PathBuf, String> {
    let fallback = managed_log_dir(environment_id, version)?;
    let Some(raw) = read_raw_opt(environment_id, version, K_LOG_DIRS) else {
        return Ok(fallback);
    };
    let base =
        paths::instance_dir(environment_id, NAME, version).unwrap_or_else(|_| fallback.clone());
    Ok(crate::config::resolve_path(&raw, &base).unwrap_or(fallback))
}

/// broker 端口：从 `listeners` 里精确读。
pub fn read_broker(environment_id: &str, version: &str) -> Option<u16> {
    let listeners = read_prop(environment_id, version, K_LISTENERS, "");
    let port = broker_port(&listeners)?;
    port.parse().ok()
}

/// 取 PLAINTEXT 那条监听项的端口数字串。
///
/// host 可为空（`PLAINTEXT://:9092`，官方 `listeners` 的写法）也可非空
/// （`PLAINTEXT://localhost:9092`，官方 `advertised.listeners` 的写法），
/// 故一律按**最后一个冒号**定位端口，不能写死 `://:`。
pub(super) fn broker_port(listeners: &str) -> Option<&str> {
    let text = listeners
        .split(',')
        .find(|l| l.trim_start().starts_with("PLAINTEXT"))?;
    let colon = text.rfind(':')?;
    let digits = &text[colon + 1..];
    let len = digits.chars().take_while(|c| c.is_ascii_digit()).count();
    (len > 0).then(|| &digits[..len])
}

/// 替换监听项里 PLAINTEXT 的端口（保留 host 部分与其余监听器定义）。
pub(super) fn replace_broker_port(listeners: &str, port: u16) -> String {
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
