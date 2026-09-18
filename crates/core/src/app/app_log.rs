use std::cell::RefCell;
use std::collections::{HashSet, VecDeque};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Instant;

use chrono::{Days, Local, NaiveDate};
use serde::{Deserialize, Serialize};

use crate::app::{id, paths, settings};

/// 日志级别常量（保留旧调用接口）。
pub const INFO: &str = "INFO";
pub const WARN: &str = "WARN";
pub const ERROR: &str = "ERROR";

const MAX_ROTATED_FILES: u32 = 5;
const DEFAULT_QUERY_LIMIT: usize = 1_000;
const MAX_QUERY_LIMIT: usize = 5_000;
const PRUNE_INTERVAL: u64 = 256;

static WRITE_COUNTER: AtomicU64 = AtomicU64::new(1);
static WRITE_LOCK: Mutex<()> = Mutex::new(());

thread_local! {
    static CURRENT_CONTEXT: RefCell<Vec<LogContext>> = const { RefCell::new(Vec::new()) };
}

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

/// GUI 查询参数。
#[derive(Debug, Clone, Default, Deserialize)]
pub struct LogQuery {
    pub date: Option<String>,
    pub level: Option<LogLevel>,
    pub environment_id: Option<String>,
    pub trace_id: Option<String>,
    pub search: Option<String>,
    pub limit: Option<usize>,
}

/// GUI 查询结果。
#[derive(Debug, Clone, Serialize)]
pub struct LogPage {
    pub date: String,
    pub records: Vec<LogRecord>,
    pub total: usize,
    pub truncated: bool,
    pub environments: Vec<String>,
}

/// 一次操作的日志作用域。
pub struct Operation {
    context: LogContext,
    started: Instant,
    finished: bool,
}

impl Operation {
    pub fn begin(environment_id: &str, begin_message: &str) -> Self {
        let trace_id = id::new_id();
        let context = LogContext {
            environment_id: Some(environment_id.to_string()),
            trace_id: Some(trace_id),
        };
        CURRENT_CONTEXT.with(|stack| stack.borrow_mut().push(context.clone()));
        let _ = log_with_context(LogLevel::Info, begin_message, Some(&context));
        Self {
            context,
            started: Instant::now(),
            finished: false,
        }
    }

    pub fn context(&self) -> &LogContext {
        &self.context
    }

    pub fn finish<T>(mut self, result: &Result<T, String>) {
        let duration_ms = self.started.elapsed().as_millis();
        match result {
            Ok(_) => {
                let _ = log_with_context(
                    LogLevel::Info,
                    &format!("操作完成（耗时 {duration_ms} ms）"),
                    Some(&self.context),
                );
            }
            Err(error) => {
                let level = if error.contains("取消") {
                    LogLevel::Warn
                } else {
                    LogLevel::Error
                };
                let _ = log_with_context(
                    level,
                    &format!("{error}（耗时 {duration_ms} ms）"),
                    Some(&self.context),
                );
            }
        }
        self.finished = true;
    }

    /// 记录一次结构化取消，避免同时写取消事件和 operation.failed。
    pub fn finish_cancelled(mut self, message: &str) {
        let duration_ms = self.started.elapsed().as_millis();
        let _ = log_with_context(
            LogLevel::Warn,
            &format!("{message}（耗时 {duration_ms} ms）"),
            Some(&self.context),
        );
        self.finished = true;
    }
}

impl Drop for Operation {
    fn drop(&mut self) {
        if !self.finished {
            let _ = log_with_context(LogLevel::Warn, "操作未正常结束", Some(&self.context));
        }
        CURRENT_CONTEXT.with(|stack| {
            stack.borrow_mut().pop();
        });
    }
}

pub fn current_context() -> Option<LogContext> {
    CURRENT_CONTEXT.with(|stack| stack.borrow().last().cloned())
}

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

/// 追加一条兼容格式日志。新日志仍写入完整结构化字段。
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

fn rotated_path(file: &Path, part: u32) -> PathBuf {
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

/// 查询结构化日志。
pub fn query_logs(query: LogQuery) -> Result<LogPage, String> {
    let date = query.date.unwrap_or_else(today);
    validate_date(&date)?;
    let needle = query
        .search
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_lowercase);
    let limit = query
        .limit
        .unwrap_or(DEFAULT_QUERY_LIMIT)
        .clamp(1, MAX_QUERY_LIMIT);
    let mut environment_set = HashSet::new();
    let mut matched = VecDeque::with_capacity(limit);
    let mut total = 0;

    visit_records_for(&date, |record| {
        if let Some(environment_id) = &record.environment_id {
            environment_set.insert(environment_id.clone());
        }
        if query.level.is_some_and(|level| record.level != level)
            || query
                .environment_id
                .as_deref()
                .is_some_and(|value| record.environment_id.as_deref() != Some(value))
            || query
                .trace_id
                .as_deref()
                .is_some_and(|value| record.trace_id.as_deref() != Some(value))
        {
            return;
        }
        if let Some(needle) = &needle {
            let haystack = format!(
                "{} {}",
                record.message,
                record.trace_id.as_deref().unwrap_or("")
            )
            .to_lowercase();
            if !haystack.contains(needle) {
                return;
            }
        }
        total += 1;
        if matched.len() == limit {
            matched.pop_front();
        }
        matched.push_back(record);
    })?;

    let mut environments: Vec<String> = environment_set.into_iter().collect();
    environments.sort();
    Ok(LogPage {
        date,
        records: matched.into_iter().collect(),
        total,
        truncated: total > limit,
        environments,
    })
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

/// 读取指定日期的日志文本（JSONL 转为可读文本，兼容旧格式）。
pub fn read_logs_for(date: &str) -> Result<String, String> {
    let records = read_records_for(date)?;
    if records.is_empty() {
        return Err(format!("{date} 无日志"));
    }
    Ok(records
        .iter()
        .map(format_record)
        .collect::<Vec<_>>()
        .join("\n"))
}

fn read_records_for(date: &str) -> Result<Vec<LogRecord>, String> {
    let mut records = Vec::new();
    visit_records_for(date, |record| records.push(record))?;
    records.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
    Ok(records)
}

fn visit_records_for(date: &str, mut visit: impl FnMut(LogRecord)) -> Result<(), String> {
    validate_date(date)?;
    let parts = log_parts_for(date)?;
    for path in parts {
        let file = std::fs::File::open(&path)
            .map_err(|e| format!("读取日志 {} 失败: {e}", path.display()))?;
        for line in BufReader::new(file).lines() {
            let line = line.map_err(|e| format!("读取日志 {} 失败: {e}", path.display()))?;
            if line.trim().is_empty() {
                continue;
            }
            if let Some(record) = parse_record(&line, date) {
                visit(record);
            }
        }
    }
    Ok(())
}

fn log_parts_for(date: &str) -> Result<Vec<PathBuf>, String> {
    let base = log_file_for(date)?;
    let mut parts = Vec::new();
    let legacy = log_dir()?.join(format!("solostack-{date}.log"));
    if legacy.is_file() {
        parts.push(legacy);
    }
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

fn parse_record(line: &str, date: &str) -> Option<LogRecord> {
    if line.starts_with('{') {
        return serde_json::from_str(line).ok();
    }
    let (time, rest) = line.strip_prefix('[')?.split_once("] ")?;
    let (level, message) = rest.split_once(' ')?;
    Some(LogRecord {
        timestamp: format!("{date}T{time}"),
        trace_id: None,
        level: LogLevel::parse(level)?,
        environment_id: None,
        message: message.to_string(),
    })
}

fn format_record(record: &LogRecord) -> String {
    format!(
        "[{}] [{}] {:<5} [{}] {}",
        record.timestamp,
        record.trace_id.as_deref().unwrap_or("-"),
        record.level.as_str(),
        record.environment_id.as_deref().unwrap_or("-"),
        record.message
    )
}

/// 从文件名解析日期和分片号。
fn parse_log_file_name(name: &str) -> Option<(String, u32)> {
    if let Some(rest) = name.strip_prefix("solostack.log.") {
        let date = rest.get(..10)?;
        NaiveDate::parse_from_str(date, "%Y-%m-%d").ok()?;
        let part = match rest.get(10..) {
            None | Some("") => 0,
            Some(value) => value.strip_prefix('.')?.parse().ok()?,
        };
        return Some((date.to_string(), part));
    }
    if let Some(rest) = name.strip_prefix("solostack-") {
        let date = rest.strip_suffix(".log")?;
        NaiveDate::parse_from_str(date, "%Y-%m-%d").ok()?;
        return Some((date.to_string(), 0));
    }
    None
}

/// 参数脱敏：密码、Token、Secret、Key、Cookie、Authorization。
pub fn redact_args(args: &[&str]) -> Vec<String> {
    let mut redact_next = false;
    args.iter()
        .map(|arg| {
            if redact_next {
                redact_next = false;
                return "<redacted>".to_string();
            }
            if let Some((key, _)) = arg.split_once('=') {
                if sensitive_key(key.trim_start_matches('-')) {
                    return format!("{key}=<redacted>");
                }
            }
            let trimmed = arg.trim_start_matches('-');
            if sensitive_key(trimmed) {
                redact_next = true;
                return (*arg).to_string();
            }
            redact_text(arg)
        })
        .collect()
}

/// 环境变量脱敏，仅返回安全或已脱敏的键值。
pub fn redact_envs(envs: &[(&str, &str)]) -> Vec<(String, String)> {
    envs.iter()
        .map(|(key, value)| {
            (
                (*key).to_string(),
                if sensitive_key(key) {
                    "<redacted>".to_string()
                } else {
                    redact_text(value)
                },
            )
        })
        .collect()
}

fn sensitive_key(key: &str) -> bool {
    let key = key.to_ascii_lowercase();
    [
        "password",
        "passwd",
        "token",
        "secret",
        "cookie",
        "authorization",
    ]
    .iter()
    .any(|marker| key.contains(marker))
        || key == "key"
        || key.ends_with("_key")
        || key.ends_with("-key")
}

fn redact_text(value: &str) -> String {
    let mut out = value.to_string();
    for marker in [
        "password=",
        "passwd=",
        "token=",
        "secret=",
        "cookie=",
        "authorization=",
        "api_key=",
        "apikey=",
    ] {
        let mut search_from = 0;
        while let Some(relative) = out[search_from..].to_ascii_lowercase().find(marker) {
            let start = search_from + relative + marker.len();
            let end = out[start..]
                .find(|c: char| c.is_whitespace() || c == '&' || c == ';')
                .map(|offset| start + offset)
                .unwrap_or(out.len());
            out.replace_range(start..end, "<redacted>");
            search_from = start + "<redacted>".len();
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::HOME_LOCK;

    fn test_home(name: &str) -> PathBuf {
        let tmp = std::env::temp_dir().join(format!("solostack-applog-{name}"));
        std::env::set_var("HOME", &tmp);
        let _ = std::fs::remove_dir_all(&tmp);
        tmp
    }

    #[test]
    fn log_date_parses_all_formats() {
        assert_eq!(
            parse_log_file_name("solostack.log.2026-09-05"),
            Some(("2026-09-05".to_string(), 0))
        );
        assert_eq!(
            parse_log_file_name("solostack.log.2026-09-05.2"),
            Some(("2026-09-05".to_string(), 2))
        );
        assert_eq!(
            parse_log_file_name("solostack-2026-09-04.log"),
            Some(("2026-09-04".to_string(), 0))
        );
        assert_eq!(parse_log_file_name("other.log"), None);
    }

    #[test]
    fn log_file_rejects_non_date_and_traversal() {
        assert!(validate_date("2026-09-14").is_ok());
        assert!(validate_date("2026-9-14").is_err());
        assert!(validate_date("../../etc/passwd").is_err());
        assert!(log_file_for("../../etc/passwd").is_err());
    }

    #[test]
    fn append_and_query_structured_record() {
        let _guard = HOME_LOCK.lock().unwrap();
        let tmp = test_home("structured");

        append(INFO, "测试日志").unwrap();
        let page = query_logs(LogQuery {
            date: Some(today()),
            search: Some("测试".to_string()),
            ..LogQuery::default()
        })
        .unwrap();
        assert_eq!(page.records.len(), 1);
        assert_eq!(page.records[0].message, "测试日志");

        let dates = list_log_dates().unwrap();
        assert_eq!(dates, vec![today()]);

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn query_reads_legacy_log_name() {
        let _guard = HOME_LOCK.lock().unwrap();
        let tmp = test_home("legacy");
        let dir = log_dir().unwrap();
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join(format!("solostack-{}.log", today())),
            "[12:00:00] INFO 旧格式日志\n",
        )
        .unwrap();

        let page = query_logs(LogQuery {
            date: Some(today()),
            ..LogQuery::default()
        })
        .unwrap();
        assert_eq!(page.records.len(), 1);
        assert_eq!(page.records[0].message, "旧格式日志");

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

        let page = query_logs(LogQuery {
            date: Some(today()),
            ..LogQuery::default()
        })
        .unwrap();
        assert!(page.records.iter().all(|record| record.trace_id.is_some()));
        assert!(page
            .records
            .iter()
            .all(|record| { record.environment_id.as_deref() == Some(environment_id) }));
        assert_eq!(page.environments, vec![environment_id.to_string()]);
        assert!(page
            .records
            .iter()
            .any(|record| record.message == "broker started"));
        assert!(page
            .records
            .iter()
            .any(|record| record.message.contains("操作完成")));

        let filtered = query_logs(LogQuery {
            date: Some(today()),
            environment_id: Some(environment_id.to_string()),
            ..LogQuery::default()
        })
        .unwrap();
        assert_eq!(filtered.total, 3);
        let empty = query_logs(LogQuery {
            date: Some(today()),
            environment_id: Some("00000000-0000-4000-8000-000000000002".to_string()),
            ..LogQuery::default()
        })
        .unwrap();
        assert_eq!(empty.total, 0);

        let by_trace = query_logs(LogQuery {
            date: Some(today()),
            trace_id: Some(trace_id),
            ..LogQuery::default()
        })
        .unwrap();
        assert_eq!(by_trace.total, 3);

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

        let page = query_logs(LogQuery {
            date: Some(today()),
            ..LogQuery::default()
        })
        .unwrap();
        assert_eq!(page.records.len(), 2);
        assert_eq!(page.records[0].message, "开始安装 kafka v4.3.1");
        assert!(page.records[1].message.contains("安装已取消"));

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
        assert_eq!(redact_text("token=abc&x=1"), "token=<redacted>&x=1");
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
        assert_eq!(redact_text(&value), value);
    }

    #[test]
    fn level_filter_and_query_limit_are_enforced() {
        let _guard = HOME_LOCK.lock().unwrap();
        let tmp = test_home("level-filter");
        let mut settings = settings::Settings::load().unwrap();
        settings.log.level = "info".to_string();
        settings.save().unwrap();

        append("DEBUG", "hidden").unwrap();
        append("INFO", "visible-1").unwrap();
        append("INFO", "visible-2").unwrap();
        append("ERROR", "visible-2").unwrap();

        let page = query_logs(LogQuery {
            date: Some(today()),
            level: Some(LogLevel::Info),
            limit: Some(1),
            ..LogQuery::default()
        })
        .unwrap();
        assert_eq!(page.records.len(), 1);
        assert_eq!(page.records[0].message, "visible-2");
        assert!(page.truncated);

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn legacy_rotated_parts_are_pruned() {
        let _guard = HOME_LOCK.lock().unwrap();
        let tmp = test_home("legacy-rotation");
        let file = log_file().unwrap();
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(rotated_path(&file, 1), "legacy rotated").unwrap();
        assert!(rotated_path(&file, 1).is_file());

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
