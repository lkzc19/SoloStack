//! 应用操作日志（生产者）：结构化记录、落盘、轮转清理与脱敏。
//!
//! 以 JSONL 落盘（每日一个文件）；**读取与查询在 `crate::logs`**（app 与组件共用），
//! 本模块只负责写入。
//! 对外 API 全部经本模块 re-export，调用方无需关心内部拆分。
//!
//! # 模块
//!
//! | 文件 | 职责 |
//! |---|---|
//! | `record.rs` | 数据模型：级别、记录、关联上下文 |
//! | `store.rs` | 写入、按日分片、轮转与清理、文件枚举 |
//! | `redact.rs` | 参数 / 环境变量 / 文本脱敏 |
//! | `operation.rs` | 操作作用域（trace_id 与环境的关联） |

mod operation;
mod record;
mod redact;
mod store;

pub use operation::{current_context, Operation};
pub use record::{LogContext, LogLevel, LogRecord, ERROR, INFO, WARN};
pub use redact::{redact_args, redact_envs};
pub use store::{
    append, debug, error, info, list_log_dates, list_log_files, log, log_dir, log_file,
    log_file_for, log_parts_for, log_script_output, log_with_context, prune_logs, today,
    validate_date, warn,
};

#[cfg(test)]
mod tests {
    use super::redact;
    use super::store;
    use super::*;
    use crate::app::settings;
    use crate::test_util::HOME_LOCK;
    use std::collections::HashSet;
    use std::path::PathBuf;

    fn test_home(name: &str) -> PathBuf {
        let tmp = std::env::temp_dir().join(format!("solostack-applog-{name}"));
        std::env::set_var("HOME", &tmp);
        let _ = std::fs::remove_dir_all(&tmp);
        tmp
    }

    #[test]
    fn log_date_parses_all_formats() {
        assert_eq!(
            store::parse_log_file_name("solostack.log.2026-09-05"),
            Some(("2026-09-05".to_string(), 0))
        );
        assert_eq!(
            store::parse_log_file_name("solostack.log.2026-09-05.2"),
            Some(("2026-09-05".to_string(), 2))
        );
        assert_eq!(
            store::parse_log_file_name("solostack-2026-09-04.log"),
            Some(("2026-09-04".to_string(), 0))
        );
        assert_eq!(store::parse_log_file_name("other.log"), None);
    }

    #[test]
    fn log_file_rejects_non_date_and_traversal() {
        assert!(validate_date("2026-09-14").is_ok());
        assert!(validate_date("2026-9-14").is_err());
        assert!(validate_date("../../etc/passwd").is_err());
        assert!(log_file_for("../../etc/passwd").is_err());
    }

    #[test]
    fn append_writes_structured_record_and_dates() {
        let _guard = HOME_LOCK.lock().unwrap();
        let tmp = test_home("structured");

        append(INFO, "测试日志").unwrap();
        let lines = crate::logs::tail(
            &crate::logs::LogSource::App {
                date: Some(today()),
            },
            10,
        )
        .unwrap();
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].message, "测试日志");
        assert!(!lines[0].raw);

        let dates = list_log_dates().unwrap();
        assert_eq!(dates, vec![today()]);

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn reads_legacy_log_format() {
        let _guard = HOME_LOCK.lock().unwrap();
        let tmp = test_home("legacy");
        let dir = log_dir().unwrap();
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join(format!("solostack-{}.log", today())),
            "[12:00:00] INFO 旧格式日志\n",
        )
        .unwrap();

        let lines = crate::logs::tail(
            &crate::logs::LogSource::App {
                date: Some(today()),
            },
            10,
        )
        .unwrap();
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].message, "旧格式日志");
        assert!(!lines[0].raw, "旧文本格式仍应按结构化解析");
        assert_eq!(lines[0].level, Some(LogLevel::Info));

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn operation_context_is_attached_to_script_logs() {
        let _guard = HOME_LOCK.lock().unwrap();
        let tmp = test_home("operation");
        let mut settings = settings::Settings::load().unwrap();
        settings.log.level = "debug".to_string();
        settings.save().unwrap();
        let environment_id = "00000000-0000-4000-8000-000000000001";
        let operation = Operation::begin(environment_id, "启动组件 kafka v4.3.1");
        let trace_id = operation.context().trace_id.clone().unwrap();
        log_script_output(current_context().as_ref(), true, "broker started");
        operation.finish(&Ok::<(), String>(()));

        let lines = crate::logs::tail(
            &crate::logs::LogSource::App {
                date: Some(today()),
            },
            100,
        )
        .unwrap();
        // 启动日志、脚本输出、操作完成三条都挂在同一 trace / 环境上
        assert!(lines.iter().all(|line| line.trace_id.is_some()));
        assert!(lines
            .iter()
            .all(|line| line.environment_id.as_deref() == Some(environment_id)));
        assert!(lines.iter().all(|line| line.trace_id.as_deref() == Some(&trace_id)));
        assert!(lines.iter().any(|line| line.message == "broker started"));
        assert!(lines
            .iter()
            .any(|line| line.message.contains("操作完成")));

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn legacy_task_id_is_read_as_trace_id() {
        let record: LogRecord = serde_json::from_str(
            r#"{
                "timestamp":"2026-09-14T15:19:01.000+08:00",
                "level":"info",
                "source":"app",
                "event":"operation.begin",
                "task_id":"start-42-1",
                "message":"legacy"
            }"#,
        )
        .unwrap();
        assert_eq!(record.trace_id.as_deref(), Some("start-42-1"));
    }

    #[test]
    fn record_serializes_only_core_fields() {
        let record = LogRecord {
            timestamp: "2026-09-18T14:30:01.123+08:00".to_string(),
            trace_id: Some("V1StGXR8".to_string()),
            level: LogLevel::Info,
            environment_id: Some("B2xQ7mNp".to_string()),
            message: "启动组件 Kafka 4.3.1".to_string(),
        };
        let value = serde_json::to_value(record).unwrap();
        let object = value.as_object().unwrap();
        let keys: HashSet<&str> = object.keys().map(String::as_str).collect();
        assert_eq!(
            keys,
            HashSet::from([
                "timestamp",
                "trace_id",
                "level",
                "environment_id",
                "message"
            ])
        );
    }

    #[test]
    fn trace_id_is_nanoid_without_business_context() {
        let _guard = HOME_LOCK.lock().unwrap();
        let tmp = test_home("trace-id");
        let operation = Operation::begin(
            "00000000-0000-4000-8000-000000000001",
            "开始安装 kafka v4.3.1",
        );
        let trace_id = operation.context().trace_id.as_deref().unwrap();
        assert_eq!(trace_id.len(), 8);
        assert!(trace_id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-'));
        assert!(!trace_id.contains("install"));
        assert!(!trace_id.contains("kafka"));
        operation.finish(&Ok::<(), String>(()));

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn cancelled_operation_writes_one_structured_event() {
        let _guard = HOME_LOCK.lock().unwrap();
        let tmp = test_home("cancelled");
        let operation = Operation::begin(
            "00000000-0000-4000-8000-000000000001",
            "开始安装 kafka v4.3.1",
        );
        operation.finish_cancelled("安装已取消：来源 user，阶段 download");

        let lines = crate::logs::tail(
            &crate::logs::LogSource::App {
                date: Some(today()),
            },
            10,
        )
        .unwrap();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].message, "开始安装 kafka v4.3.1");
        assert!(lines[1].message.contains("安装已取消"));

        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// 日志级别只由显式调用决定，不解析错误文案：
    /// 走 `finish` 的失败一律 ERROR，取消必须显式走 `finish_cancelled`（WARN）。
    #[test]
    fn operation_level_depends_on_explicit_finish_not_message_text() {
        let _guard = HOME_LOCK.lock().unwrap();
        let tmp = test_home("operation-level");
        let env = "00000000-0000-4000-8000-000000000001";

        // 即使文案含「取消」，走 finish 也是 ERROR（不再靠字符串判断）
        Operation::begin(env, "开始 A").finish(&Err::<(), String>("安装已取消".to_string()));
        // 显式取消路径记 WARN
        Operation::begin(env, "开始 B").finish_cancelled("安装已取消");

        let lines = crate::logs::tail(
            &crate::logs::LogSource::App {
                date: Some(today()),
            },
            10,
        )
        .unwrap();
        assert_eq!(lines[1].level, Some(LogLevel::Error));
        assert_eq!(lines[3].level, Some(LogLevel::Warn));

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn sensitive_values_are_redacted() {
        let args = vec!["--password", "secret-value", "--token=abc", "normal"];
        let redacted = redact_args(&args);
        assert_eq!(
            redacted,
            vec![
                "--password".to_string(),
                "<redacted>".to_string(),
                "--token=<redacted>".to_string(),
                "normal".to_string()
            ]
        );
        assert_eq!(redact::redact_text("token=abc&x=1"), "token=<redacted>&x=1");
        assert_eq!(
            redact_envs(&[("API_TOKEN", "abc"), ("JAVA_HOME", "/jdk")]),
            vec![
                ("API_TOKEN".to_string(), "<redacted>".to_string()),
                ("JAVA_HOME".to_string(), "/jdk".to_string())
            ]
        );
    }

    #[test]
    fn redaction_does_not_truncate_long_text() {
        let value = "x".repeat(5_000);
        assert_eq!(redact::redact_text(&value), value);
    }

    #[test]
    fn level_filter_applies_on_write() {
        let _guard = HOME_LOCK.lock().unwrap();
        let tmp = test_home("level-filter");
        let mut settings = settings::Settings::load().unwrap();
        settings.log.level = "info".to_string();
        settings.save().unwrap();

        append("DEBUG", "hidden").unwrap();
        append("INFO", "visible").unwrap();

        let lines = crate::logs::tail(
            &crate::logs::LogSource::App {
                date: Some(today()),
            },
            10,
        )
        .unwrap();
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].message, "visible");

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn legacy_rotated_parts_are_pruned() {
        let _guard = HOME_LOCK.lock().unwrap();
        let tmp = test_home("legacy-rotation");
        let file = log_file().unwrap();
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(store::rotated_path(&file, 1), "legacy rotated").unwrap();
        assert!(store::rotated_path(&file, 1).is_file());

        let old = log_dir().unwrap().join("solostack.log.2000-01-01");
        std::fs::write(&old, "old").unwrap();
        let mut settings = settings::Settings::load().unwrap();
        settings.log.retention_days = 1;
        settings.save().unwrap();
        assert!(prune_logs().unwrap() >= 1);
        assert!(!old.exists());

        let _ = std::fs::remove_dir_all(&tmp);
    }
}
