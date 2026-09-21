//! Hadoop 配置的精确读取与路径解析。

use std::path::PathBuf;

use super::super::NAME;
use super::{F_HDFS, F_MAPRED, K_HISTORY_WEB, K_NN_NAME_DIR};
use crate::app::paths;
use crate::component;

/// 精确读一个配置项的原始值（空值视为未设置）。
pub(super) fn read_raw(
    environment_id: &str,
    version: &str,
    file: &str,
    key: &str,
) -> Option<String> {
    let f = component::config_io::open_config(environment_id, NAME, version, file).ok()?;
    f.get_trimmed(key).ok()?
}

/// 一次读入某配置文件的全部键值（键 → 值）。
pub(super) fn read_all(
    environment_id: &str,
    version: &str,
    file: &str,
) -> std::collections::HashMap<String, String> {
    component::config_io::open_config(environment_id, NAME, version, file)
        .and_then(|f| f.read_entries())
        .map(|entries| {
            entries
                .into_iter()
                .map(|e| (e.key, e.value.trim().to_string()))
                .collect()
        })
        .unwrap_or_default()
}

/// 从 `host:port` 形式的配置项精确读出端口。
pub(super) fn read_port(
    environment_id: &str,
    version: &str,
    file: &str,
    key: &str,
) -> Option<u16> {
    parse_host_port(&read_raw(environment_id, version, file, key)?)
}

/// 解析 `localhost:9870` / `0.0.0.0:9864` 这类「主机:端口」里的端口。
pub(super) fn parse_host_port(s: &str) -> Option<u16> {
    s.rsplit(':').next()?.trim().parse::<u16>().ok()
}

/// SoloStack 受管的 NameNode 元数据目录（**写进配置的值**）。
///
/// 数据目录键由 SoloStack 拥有：它们决定组件数据落在哪，而卸载（`keep_data`）、
/// `is_within_root` 等机制都建立在 `var/data/<组件>/<版本>/` 之下。因此我们
/// 始终写入受管路径，而不是把官方模板里的占位值或历史值原样回声回去。
pub(super) fn managed_namenode_dir(
    environment_id: &str,
    version: &str,
) -> Result<PathBuf, String> {
    Ok(paths::var_data_instance_dir(environment_id, NAME, version)
        .map_err(|e| e.to_string())?
        .join("name"))
}

/// SoloStack 受管的 DataNode 数据目录（**写进配置的值**）。
pub(super) fn managed_datanode_dir(
    environment_id: &str,
    version: &str,
) -> Result<PathBuf, String> {
    Ok(paths::var_data_instance_dir(environment_id, NAME, version)
        .map_err(|e| e.to_string())?
        .join("data"))
}

/// NameNode 元数据目录**实际所在位置**：从配置精确读 `dfs.namenode.name.dir`。
///
/// 用于「是否已格式化」的判断，必须与 Hadoop 实际使用的位置一致：
/// 若按受管默认路径判断，手改过该键（或用 `file://` / 相对路径写法）时会错位，
/// 于是每次启动都跑一次 `namenode -format -force`（`-force` 会重格式化、抹掉
/// 命名空间数据）。相对路径按实例目录解析，与组件启动时的工作目录一致。
pub fn namenode_dir(environment_id: &str, version: &str) -> Result<PathBuf, String> {
    Ok(configured_dir(
        environment_id,
        version,
        K_NN_NAME_DIR,
        managed_namenode_dir(environment_id, version)?,
    ))
}

/// 读路径型配置项并解析为绝对路径；未设置时用 `fallback`（受管默认路径）。
fn configured_dir(
    environment_id: &str,
    version: &str,
    key: &str,
    fallback: PathBuf,
) -> PathBuf {
    let Some(raw) = read_raw(environment_id, version, F_HDFS, key) else {
        return fallback;
    };
    let base =
        paths::instance_dir(environment_id, NAME, version).unwrap_or_else(|_| fallback.clone());
    crate::config::resolve_path(&raw, &base).unwrap_or(fallback)
}

/// 历史服务器是否开启：以 `mapred-site.xml` 里有没有该键为准（精确读）。
pub fn history_enabled(environment_id: &str, version: &str) -> bool {
    read_port(environment_id, version, F_MAPRED, K_HISTORY_WEB).is_some()
}
