use std::io::Write;
use std::path::PathBuf;

use chrono::{Local, NaiveDate};

use crate::app::paths;

/// 日志级别。
pub const INFO: &str = "INFO";
pub const WARN: &str = "WARN";
pub const ERROR: &str = "ERROR";

/// app 操作日志目录：`~/.solostack/app/log/`。
pub fn log_dir() -> Result<PathBuf, String> {
    paths::app_log_dir().map_err(|e| e.to_string())
}

/// 今天日期串 `YYYY-MM-DD`。
pub fn today() -> String {
    Local::now().format("%Y-%m-%d").to_string()
}

/// 指定日期的日志文件：`solostack.log.YYYY-MM-DD`。
pub fn log_file_for(date: &str) -> Result<PathBuf, String> {
    validate_date(date)?;
    Ok(log_dir()?.join(format!("solostack.log.{date}")))
}

/// 严格校验日志日期，只接受 `YYYY-MM-DD`。
pub fn validate_date(date: &str) -> Result<(), String> {
    let parsed = NaiveDate::parse_from_str(date, "%Y-%m-%d")
        .map_err(|_| format!("日志日期格式无效: {date}"))?;
    if parsed.format("%Y-%m-%d").to_string() != date {
        return Err(format!("日志日期格式无效: {date}"));
    }
    Ok(())
}

/// 当天日志文件（追加写入目标）。
pub fn log_file() -> Result<PathBuf, String> {
    log_file_for(&today())
}

/// 追加一条操作日志（自动补时间戳，按天滚动文件）。
pub fn append(level: &str, message: &str) -> Result<(), String> {
    let file = log_file()?;
    if let Some(parent) = file.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let line = format!(
        "[{}] {} {}\n",
        Local::now().format("%Y-%m-%d %H:%M:%S"),
        level,
        message
    );
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&file)
        .map_err(|e| format!("打开日志文件失败: {e}"))?;
    f.write_all(line.as_bytes())
        .map_err(|e| format!("写入日志失败: {e}"))
}

/// 列出日志文件（按日期倒序，新的在前）。
///
/// 兼容命名：`solostack.log.YYYY-MM-DD`（当前）与旧式 `solostack-YYYY-MM-DD.log`。
pub fn list_log_files() -> Result<Vec<PathBuf>, String> {
    let dir = log_dir()?;
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut files: Vec<(String, PathBuf)> = std::fs::read_dir(&dir)
        .map_err(|e| e.to_string())?
        .flatten()
        .filter(|e| e.path().is_file())
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            log_date_of(&name).map(|d| (d, e.path()))
        })
        .collect();
    files.sort_by(|a, b| b.0.cmp(&a.0)); // 日期倒序
    Ok(files.into_iter().map(|(_, p)| p).collect())
}

/// 列出可用日志日期（倒序，新的在前；同一天新旧命名去重）。
pub fn list_log_dates() -> Result<Vec<String>, String> {
    let mut dates: Vec<String> = list_log_files()?
        .into_iter()
        .filter_map(|p| p.file_name().and_then(|n| n.to_str()).and_then(log_date_of))
        .collect();
    dates.sort_by(|a, b| b.cmp(a));
    dates.dedup();
    Ok(dates)
}

/// 从日志文件名解析日期，识别 `solostack.log.YYYY-MM-DD` 与 `solostack-YYYY-MM-DD.log`。
fn log_date_of(name: &str) -> Option<String> {
    if let Some(rest) = name.strip_prefix("solostack.log.") {
        return Some(rest.to_string());
    }
    if let Some(rest) = name.strip_prefix("solostack-") {
        if let Some(date) = rest.strip_suffix(".log") {
            return Some(date.to_string());
        }
    }
    None
}

/// 读取指定日期的完整日志内容（不截断，供设置页查看当天全部日志）。
///
/// 优先读当前命名 `solostack.log.YYYY-MM-DD`；缺失时回退旧命名 `solostack-YYYY-MM-DD.log`。
pub fn read_logs_for(date: &str) -> Result<String, String> {
    let file = log_file_for(date)?;
    let path = if file.is_file() {
        file
    } else {
        let legacy = log_dir()?.join(format!("solostack-{date}.log"));
        if legacy.is_file() {
            legacy
        } else {
            file
        }
    };
    if !path.is_file() {
        return Err(format!("{date} 无日志"));
    }
    std::fs::read_to_string(&path).map_err(|e| format!("读取日志失败: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_date_parses_both_formats() {
        assert_eq!(
            log_date_of("solostack.log.2026-09-05").as_deref(),
            Some("2026-09-05")
        );
        assert_eq!(
            log_date_of("solostack-2026-09-04.log").as_deref(),
            Some("2026-09-04")
        );
        assert_eq!(log_date_of("other.log"), None);
    }

    #[test]
    fn log_file_rejects_non_date_and_traversal() {
        assert!(validate_date("2026-09-14").is_ok());
        assert!(validate_date("2026-9-14").is_err());
        assert!(validate_date("../../etc/passwd").is_err());
        assert!(log_file_for("../../etc/passwd").is_err());
    }

    #[test]
    fn append_and_read_for_date() {
        use crate::test_util::HOME_LOCK;
        let _guard = HOME_LOCK.lock().unwrap();
        let tmp = std::env::temp_dir().join("solostack-applog-test");
        std::env::set_var("HOME", &tmp);
        let _ = std::fs::remove_dir_all(&tmp);

        append(INFO, "测试日志").unwrap();
        let content = read_logs_for(&today()).unwrap();
        assert!(content.contains("测试日志"));

        let dates = list_log_dates().unwrap();
        assert_eq!(dates.len(), 1);
        assert_eq!(dates[0], today());

        let _ = std::fs::remove_dir_all(&tmp);
    }
}
