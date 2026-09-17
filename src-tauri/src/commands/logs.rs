//! 组件日志：列出日志文件、按路径读取尾部。

use solostack_core::app::app_log::{LogPage, LogQuery};
use solostack_core::lifecycle::logs;

use super::resolve;

/// 列出组件日志文件（完整路径，按修改时间倒序）。
#[tauri::command]
pub fn list_component_logs(component: String) -> Result<Vec<String>, String> {
    let i = resolve(&component)?;
    logs::list_log_files(&i.name, &i.version)
        .map(|v| v.into_iter().map(|p| p.display().to_string()).collect())
}

/// 读取日志文件末尾若干行。仅允许该组件名下的日志路径。
#[tauri::command]
pub fn read_component_log_tail(
    component: String,
    path: String,
    lines: usize,
) -> Result<String, String> {
    let i = resolve(&component)?;
    let log_path = std::path::PathBuf::from(&path);
    let log_path = logs::validated_log_file(&i.name, &i.version, &log_path)?;
    logs::tail(&log_path, lines)
}

/// 查询结构化的 app 操作日志。
#[tauri::command]
pub fn query_app_logs(query: LogQuery) -> Result<LogPage, String> {
    solostack_core::app::app_log::query_logs(query)
}
