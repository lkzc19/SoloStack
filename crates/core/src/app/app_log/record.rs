//! 日志的数据模型：级别、记录、关联上下文与查询 DTO。

use serde::{Deserialize, Serialize};

/// 日志级别常量（保留旧调用接口）。
pub const INFO: &str = "INFO";
pub const WARN: &str = "WARN";
pub const ERROR: &str = "ERROR";

/// 日志级别。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Error,
    Warn,
    #[default]
    Info,
    Debug,
    Trace,
}

impl LogLevel {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Error => "ERROR",
            Self::Warn => "WARN",
            Self::Info => "INFO",
            Self::Debug => "DEBUG",
            Self::Trace => "TRACE",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "error" => Some(Self::Error),
            "warn" | "warning" => Some(Self::Warn),
            "info" => Some(Self::Info),
            "debug" => Some(Self::Debug),
            "trace" => Some(Self::Trace),
            _ => None,
        }
    }
}

/// 一条结构化日志。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogRecord {
    pub timestamp: String,
    #[serde(
        rename = "trace_id",
        alias = "task_id",
        skip_serializing_if = "Option::is_none"
    )]
    pub trace_id: Option<String>,
    pub level: LogLevel,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub environment_id: Option<String>,
    pub message: String,
}

/// 当前日志关联上下文。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LogContext {
    pub environment_id: Option<String>,
    pub trace_id: Option<String>,
}
