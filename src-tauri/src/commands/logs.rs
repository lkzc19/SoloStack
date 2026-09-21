//! 日志查看：来源解析、列出可查看文件、读取尾部。
//!
//! app 与组件共用同一套读取（core 的 `logs`）；本层只做参数解析与错误映射。

use std::path::PathBuf;

use serde::Deserialize;
use solostack_core::logs::{self, LogLine, LogSource};

use super::resolve;

/// 前端提交的日志来源。
#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LogSourceRequest {
    /// app 操作日志；`date` 省略表示实时（今天，跨天自动切换）。
    AppLog { date: Option<String> },
    /// 组件单个日志文件。
    Component {
        environment_id: String,
        component: String,
        path: String,
    },
}

impl LogSourceRequest {
    /// 解析为 core 的 `LogSource`（组件来源会校验实例已安装）。
    pub(crate) fn resolve(self) -> Result<LogSource, String> {
        match self {
            LogSourceRequest::AppLog { date } => Ok(LogSource::App { date }),
            LogSourceRequest::Component {
                environment_id,
                component,
                path,
            } => {
                let i = resolve(&environment_id, &component)?;
                Ok(LogSource::Component {
                    environment_id: i.environment_id,
                    component: i.name,
                    version: i.version,
                    file: PathBuf::from(path),
                })
            }
        }
    }
}

/// 列出组件可查看的日志文件（完整路径，按修改时间倒序）。
#[tauri::command]
pub fn list_component_logs(
    environment_id: String,
    component: String,
) -> Result<Vec<String>, String> {
    let i = resolve(&environment_id, &component)?;
    logs::list_component_files(&i.environment_id, &i.name, &i.version)
        .map(|v| v.into_iter().map(|p| p.display().to_string()).collect())
}

/// 列出 app 日志可用日期（倒序，新的在前）。
#[tauri::command]
pub fn list_app_log_dates() -> Result<Vec<String>, String> {
    solostack_core::app::app_log::list_log_dates()
}

/// 读取某来源末尾若干行（Flink 式 tail）。
#[tauri::command]
pub fn read_log_tail(source: LogSourceRequest, lines: usize) -> Result<Vec<LogLine>, String> {
    logs::tail(&source.resolve()?, lines)
}
