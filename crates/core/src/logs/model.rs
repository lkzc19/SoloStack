//! 日志读取的统一行模型。

use crate::app::app_log::{LogLevel, LogRecord};

/// 供前端渲染的一行日志。
///
/// app 日志解析 JSONL 后填充全部字段；组件日志是原始文本，只有 `message`，
/// `raw = true`。前端据此用同一套渲染逻辑：raw 行直接换行显示 message，
/// 结构化行再分列显示时间/级别/trace/环境。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct LogLine {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub level: Option<LogLevel>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trace_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub environment_id: Option<String>,
    pub message: String,
    /// true = 原始文本行（组件日志），前端只按 message 渲染。
    pub raw: bool,
}

impl LogLine {
    /// app 日志：结构化记录 → 行。
    pub fn from_record(record: LogRecord) -> Self {
        Self {
            timestamp: Some(record.timestamp),
            level: Some(record.level),
            trace_id: record.trace_id,
            environment_id: record.environment_id,
            message: record.message,
            raw: false,
        }
    }

    /// 组件日志：原始文本行 → 行。
    pub fn raw(message: impl Into<String>, environment_id: Option<String>) -> Self {
        Self {
            timestamp: None,
            level: None,
            trace_id: None,
            environment_id,
            message: message.into(),
            raw: true,
        }
    }
}
