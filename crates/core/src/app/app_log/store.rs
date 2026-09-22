//! 日志存储：写入、按日分片、轮转与清理、文件枚举。

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use chrono::{Days, Local, NaiveDate};

use super::record::{LogContext, LogLevel, LogRecord};
use super::{current_context, redact::redact_text};
use crate::app::{paths, settings};

const MAX_ROTATED_FILES: u32 = 5;
const PRUNE_INTERVAL: u64 = 256;

static WRITE_COUNTER: AtomicU64 = AtomicU64::new(1);
static WRITE_LOCK: Mutex<()> = Mutex::new(());

/// app 操作日志目录：`~/.solostack/app/log/`。
pub fn log_dir() -> Result<PathBuf, String> {
    paths::app_log_dir().map_err(|e| e.to_string())
}

/// 今天日期串 `YYYY-MM-DD`。
pub fn today() -> String {
    Local::now().format("%Y-%m-%d").to_string()
}

/// 指定日期的当前日志文件：`solostack.log.YYYY-MM-DD`。
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

/// 当天当前日志文件（追加写入目标）。
pub fn log_file() -> Result<PathBuf, String> {
    log_file_for(&today())
}

/// 追加一条结构化日志。
pub fn append(level: &str, message: &str) -> Result<(), String> {
    let level = LogLevel::parse(level).unwrap_or(LogLevel::Info);
    log_with_context(level, message, current_context().as_ref())
}

pub fn debug(message: &str) -> Result<(), String> {
    log(LogLevel::Debug, message)
}

pub fn info(message: &str) -> Result<(), String> {
    log(LogLevel::Info, message)
}

pub fn warn(message: &str) -> Result<(), String> {
    log(LogLevel::Warn, message)
}

pub fn error(message: &str) -> Result<(), String> {
    log(LogLevel::Error, message)
}

/// 使用当前任务上下文写日志。
pub fn log(level: LogLevel, message: &str) -> Result<(), String> {
    log_with_context(level, message, current_context().as_ref())
}

/// 使用显式上下文写日志。
pub fn log_with_context(
    level: LogLevel,
    message: &str,
    context: Option<&LogContext>,
) -> Result<(), String> {
    if !enabled(level) {
        return Ok(());
    }
    let context = context.cloned().unwrap_or_default();
    let record = LogRecord {
        timestamp: Local::now().format("%Y-%m-%dT%H:%M:%S%.3f%:z").to_string(),
        trace_id: context.trace_id,
        level,
        environment_id: context.environment_id,
        message: redact_text(message),
    };
    write_record(&record)
}

/// 写脚本 stdout/stderr；由进程读取线程调用，因此显式传入上下文。
pub fn log_script_output(context: Option<&LogContext>, stdout: bool, text: &str) {
    if text.is_empty() {
        return;
    }
    let level = if stdout {
        LogLevel::Info
    } else {
        LogLevel::Warn
    };
    let _ = log_with_context(level, text, context);
}

fn enabled(level: LogLevel) -> bool {
    let configured = settings::Settings::load()
        .ok()
        .and_then(|settings| LogLevel::parse(&settings.log.level))
        .unwrap_or(LogLevel::Info);
    level <= configured
}

fn write_record(record: &LogRecord) -> Result<(), String> {
    let _guard = WRITE_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let file = log_file()?;
    if let Some(parent) = file.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }

    let line = serde_json::to_string(record).map_err(|e| format!("序列化日志失败: {e}"))?;
    let result = (|| -> Result<(), String> {
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&file)
            .map_err(|e| format!("打开日志文件失败: {e}"))?;
        f.write_all(line.as_bytes())
            .and_then(|_| f.write_all(b"\n"))
            .map_err(|e| format!("写入日志失败: {e}"))
    })();

    if let Err(error) = &result {
        eprintln!("日志写入失败: {error}");
    }
    if WRITE_COUNTER
        .fetch_add(1, Ordering::Relaxed)
        .is_multiple_of(PRUNE_INTERVAL)
    {
        let _ = prune_logs();
    }
    result
}

/// 轮转分片路径：`<当前日志>.<part>`（仅测试与 `log_parts_for` 使用）。
pub(super) fn rotated_path(file: &Path, part: u32) -> PathBuf {
    let name = file
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("solostack.log");
    file.with_file_name(format!("{name}.{part}"))
}

/// 清理超过保留期或空间上限的日志。
pub fn prune_logs() -> Result<usize, String> {
    let settings = settings::Settings::load().unwrap_or_default();
    let retention = settings.log.retention_days.max(1) as u64;
    let cutoff = Local::now()
        .date_naive()
        .checked_sub_days(Days::new(retention.saturating_sub(1)))
        .unwrap_or_else(|| Local::now().date_naive());
    let max_bytes = settings.log.max_total_mb.max(1) * 1024 * 1024;

    let mut files: Vec<(String, u32, PathBuf, u64)> = Vec::new();
    let dir = log_dir()?;
    if !dir.is_dir() {
        return Ok(0);
    }
    for entry in std::fs::read_dir(&dir)
        .map_err(|e| e.to_string())?
        .flatten()
    {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        let Some((date, part)) = parse_log_file_name(name) else {
            continue;
        };
        let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
        files.push((date, part, path, size));
    }

    let mut removed = 0;
    for (date, _, path, _) in &files {
        let Ok(parsed) = NaiveDate::parse_from_str(date, "%Y-%m-%d") else {
            continue;
        };
        if parsed < cutoff && std::fs::remove_file(path).is_ok() {
            removed += 1;
        }
    }

    files.retain(|(_, _, path, _)| path.exists());
    files.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
    let mut total: u64 = files.iter().map(|(_, _, _, size)| size).sum();
    for (_, _, path, size) in files {
        if total <= max_bytes {
            break;
        }
        if std::fs::remove_file(path).is_ok() {
            total = total.saturating_sub(size);
            removed += 1;
        }
    }
    Ok(removed)
}

/// 列出日志文件（日期、分片升序）。
pub fn list_log_files() -> Result<Vec<PathBuf>, String> {
    let dir = log_dir()?;
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut files: Vec<(String, u32, PathBuf)> = std::fs::read_dir(&dir)
        .map_err(|e| e.to_string())?
        .flatten()
        .filter(|entry| entry.path().is_file())
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().to_string();
            parse_log_file_name(&name).map(|(date, part)| (date, part, entry.path()))
        })
        .collect();
    files.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
    Ok(files.into_iter().map(|(_, _, path)| path).collect())
}

/// 列出可用日志日期（倒序，新的在前）。
pub fn list_log_dates() -> Result<Vec<String>, String> {
    let mut dates: Vec<String> = list_log_files()?
        .into_iter()
        .filter_map(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .and_then(parse_log_file_name)
                .map(|(date, _)| date)
        })
        .collect();
    dates.sort_by(|a, b| b.cmp(a));
    dates.dedup();
    Ok(dates)
}

/// 指定日期的全部日志分片（轮转分片、当前文件），读序从旧到新。
pub fn log_parts_for(date: &str) -> Result<Vec<PathBuf>, String> {
    let base = log_file_for(date)?;
    let mut parts = Vec::new();
    for part in (1..=MAX_ROTATED_FILES).rev() {
        let path = rotated_path(&base, part);
        if path.is_file() {
            parts.push(path);
        }
    }
    if base.is_file() {
        parts.push(base);
    }
    Ok(parts)
}

/// 从文件名解析日期和分片号：`solostack.log.<date>[.<part>]`。
pub(super) fn parse_log_file_name(name: &str) -> Option<(String, u32)> {
    if let Some(rest) = name.strip_prefix("solostack.log.") {
        let date = rest.get(..10)?;
        NaiveDate::parse_from_str(date, "%Y-%m-%d").ok()?;
        let part = match rest.get(10..) {
            None | Some("") => 0,
            Some(value) => value.strip_prefix('.')?.parse().ok()?,
        };
        return Some((date.to_string(), part));
    }
    None
}
