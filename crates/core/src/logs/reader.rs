//! 日志读取与实时跟随：Flink 式"看尾部 + 跟新增"。
//!
//! app 与组件共用同一份读取：按来源分片列表读行、按来源种类解析、同一套轮转/
//! 半行缓冲逻辑。批量读取（`tail`）与实时跟随（`LogFollower`）也共用解析。

use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use super::model::LogLine;
use super::source::LogSource;
use crate::app::app_log::LogRecord;

const MAX_PARTIAL_LINE_BYTES: usize = 1024 * 1024;
/// tail / backfill 的行数上限（IPC 传入值不可信，core 兜底）。
pub const MAX_TAIL_LINES: usize = 5_000;
/// 反向读取的块大小。
const BACKFILL_CHUNK_BYTES: usize = 64 * 1024;
/// 反向扫描的字节上限：避免"极少换行的巨型文件"把整文件读进内存。
const MAX_BACKFILL_BYTES: usize = 16 * 1024 * 1024;

/// 一轮跟随读取的结果。
#[derive(Debug, Clone, Default)]
pub struct PollResult {
    pub lines: Vec<LogLine>,
    pub rotated: bool,
}

/// 读取来源末尾 `lines` 行（跨分片，读序从旧到新）。
pub fn tail(source: &LogSource, lines: usize) -> Result<Vec<LogLine>, String> {
    let keep = lines.clamp(1, MAX_TAIL_LINES);
    let mut tail: VecDeque<LogLine> = VecDeque::with_capacity(keep);
    for path in source.parts()? {
        let file = std::fs::File::open(&path)
            .map_err(|e| format!("读取日志 {} 失败: {e}", path.display()))?;
        for line in BufReader::new(file).lines() {
            let line = line.map_err(|e| format!("读取日志 {} 失败: {e}", path.display()))?;
            let Some(parsed) = parse_line(&line, source) else {
                continue;
            };
            if tail.len() == keep {
                tail.pop_front();
            }
            tail.push_back(parsed);
        }
    }
    Ok(tail.into_iter().collect())
}

/// 一个来源的 offset 跟随器（含轮转检测与半行缓冲）。
pub struct LogFollower {
    source: LogSource,
    path: Option<PathBuf>,
    offset: u64,
    identity: Option<FileIdentity>,
    partial: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FileIdentity {
    device: u64,
    inode: u64,
}

impl LogFollower {
    pub fn new(source: LogSource) -> Self {
        Self {
            source,
            path: None,
            offset: 0,
            identity: None,
            partial: Vec::new(),
        }
    }

    pub fn offset(&self) -> u64 {
        self.offset
    }

    /// 从一个安全位置回填最近 N 行，并把 offset 推进到文件末尾。
    ///
    /// 只从文件尾部反向读取若干块（不整文件读入内存），凑够行数即停。
    pub fn backfill(&mut self, lines: usize) -> Result<Vec<LogLine>, String> {
        let Some(path) = self.current_path()? else {
            return Ok(Vec::new());
        };
        self.path = Some(path.clone());
        self.partial.clear();
        let file = std::fs::File::open(&path)
            .map_err(|e| format!("读取日志 {} 失败: {e}", path.display()))?;
        self.identity = file_identity(&file);
        let len = file
            .metadata()
            .map_err(|e| format!("读取日志元数据 {} 失败: {e}", path.display()))?
            .len();
        self.offset = len;
        drop(file);

        let keep = lines.clamp(1, MAX_TAIL_LINES);
        let (complete, partial) = read_tail_bytes(&path, keep)?;
        self.partial = partial;
        let mut tail: VecDeque<&[u8]> = VecDeque::with_capacity(keep);
        for line in complete.split(|byte| *byte == b'\n') {
            if line.is_empty() {
                continue;
            }
            if tail.len() == keep {
                tail.pop_front();
            }
            tail.push_back(line);
        }
        Ok(tail
            .into_iter()
            .filter_map(|line| {
                let line = String::from_utf8_lossy(line);
                parse_line(&line, &self.source)
            })
            .collect())
    }

    /// 只读取自上次 offset 之后新增的内容。
    pub fn poll(&mut self) -> Result<PollResult, String> {
        let Some(path) = self.current_path()? else {
            return Ok(PollResult::default());
        };
        let mut rotated = false;
        if self.path.as_deref() != Some(path.as_path()) {
            rotated = self.path.is_some();
            self.path = Some(path.clone());
            self.offset = 0;
            self.identity = None;
            self.partial.clear();
        }

        let mut file = match std::fs::File::open(&path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(PollResult::default());
            }
            Err(error) => return Err(format!("读取日志 {} 失败: {error}", path.display())),
        };
        let metadata = file
            .metadata()
            .map_err(|e| format!("读取日志元数据 {} 失败: {e}", path.display()))?;
        let identity = file_identity(&file);
        if self
            .identity
            .zip(identity)
            .is_some_and(|(current, next)| current != next)
        {
            rotated = true;
            self.offset = 0;
            self.partial.clear();
        }
        self.identity = identity;
        if metadata.len() < self.offset {
            rotated = true;
            self.offset = 0;
            self.partial.clear();
        }
        if metadata.len() == self.offset {
            return Ok(PollResult {
                lines: Vec::new(),
                rotated,
            });
        }

        file.seek(SeekFrom::Start(self.offset))
            .map_err(|e| format!("定位日志 {} 失败: {e}", path.display()))?;
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)
            .map_err(|e| format!("读取日志新增内容 {} 失败: {e}", path.display()))?;
        self.offset = self.offset.saturating_add(bytes.len() as u64);

        let mut lines = Vec::new();
        self.partial.extend_from_slice(&bytes);
        while let Some(position) = self.partial.iter().position(|byte| *byte == b'\n') {
            let line: Vec<u8> = self.partial.drain(..=position).collect();
            let text = String::from_utf8_lossy(&line);
            if let Some(parsed) = parse_line(text.trim_end_matches(['\r', '\n']), &self.source) {
                lines.push(parsed);
            }
        }
        if self.partial.len() > MAX_PARTIAL_LINE_BYTES {
            let text = String::from_utf8_lossy(&self.partial).into_owned();
            self.partial.clear();
            lines.push(LogLine::raw(
                format!("{text}...[单行过长，已截断]"),
                self.source.environment_id().map(str::to_string),
            ));
        }

        Ok(PollResult { lines, rotated })
    }

    fn current_path(&self) -> Result<Option<PathBuf>, String> {
        self.source.current_file()
    }
}

/// 一行文本 → `LogLine`。
///
/// 结构化来源（app）按 JSONL 解析，解析失败则按原始行；组件来源直接按原始行。
fn parse_line(line: &str, source: &LogSource) -> Option<LogLine> {
    if line.trim().is_empty() {
        return None;
    }
    if source.structured() {
        if let Ok(record) = serde_json::from_str::<LogRecord>(line) {
            return Some(LogLine::from_record(record));
        }
    }
    Some(LogLine::raw(
        line.to_string(),
        source.environment_id().map(str::to_string),
    ))
}

/// 从文件尾部反向读取：返回「完整区域」与「结尾未换行的半行」。
///
/// 每凑够一块就检查是否已有 `max_lines` 条非空行，够就停；总量再受
/// `MAX_BACKFILL_BYTES` 约束。两者保证读一个大日志时内存有界。
fn read_tail_bytes(path: &Path, max_lines: usize) -> Result<(Vec<u8>, Vec<u8>), String> {
    let mut file =
        std::fs::File::open(path).map_err(|e| format!("读取日志 {} 失败: {e}", path.display()))?;
    let len = file
        .metadata()
        .map_err(|e| format!("读取日志元数据 {} 失败: {e}", path.display()))?
        .len();

    let mut buf: Vec<u8> = Vec::new();
    let mut pos = len;
    while pos > 0 {
        let start = pos.saturating_sub(BACKFILL_CHUNK_BYTES as u64);
        let mut chunk = vec![0u8; (pos - start) as usize];
        file.seek(SeekFrom::Start(start))
            .map_err(|e| format!("定位日志 {} 失败: {e}", path.display()))?;
        file.read_exact(&mut chunk)
            .map_err(|e| format!("读取日志 {} 失败: {e}", path.display()))?;
        chunk.extend_from_slice(&buf);
        buf = chunk;
        pos = start;
        if complete_line_count(&buf) >= max_lines || buf.len() >= MAX_BACKFILL_BYTES {
            break;
        }
    }

    let complete = complete_len(&buf);
    let partial = buf[complete..].to_vec();
    buf.truncate(complete);
    Ok((buf, partial))
}

/// 完整区域长度：以换行结尾取全长，否则取最后一个换行之后（结尾半行另行处理）。
fn complete_len(buf: &[u8]) -> usize {
    if buf.last().is_some_and(|byte| *byte == b'\n') {
        buf.len()
    } else {
        buf.iter()
            .rposition(|byte| *byte == b'\n')
            .map(|position| position + 1)
            .unwrap_or(0)
    }
}

/// 完整区域里的非空行数（用于判断能否停止反向读取）。
fn complete_line_count(buf: &[u8]) -> usize {
    buf[..complete_len(buf)]
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
        .count()
}

#[cfg(unix)]
fn file_identity(file: &std::fs::File) -> Option<FileIdentity> {
    use std::os::unix::fs::MetadataExt;
    file.metadata().ok().map(|metadata| FileIdentity {
        device: metadata.dev(),
        inode: metadata.ino(),
    })
}

#[cfg(not(unix))]
fn file_identity(_file: &std::fs::File) -> Option<FileIdentity> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::HOME_LOCK;
    use std::io::Write;

    const ENV_ID: &str = "Env00001";

    fn setup(name: &str) -> PathBuf {
        let tmp = std::env::temp_dir().join(format!("solostack-logs-reader-{name}"));
        std::env::set_var("HOME", &tmp);
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        tmp
    }

    fn component_source(file: PathBuf) -> LogSource {
        LogSource::Component {
            environment_id: ENV_ID.to_string(),
            component: "hadoop".to_string(),
            version: "3.5.0".to_string(),
            file,
        }
    }

    #[test]
    fn tail_reads_last_lines_as_raw_for_component() {
        let _guard = HOME_LOCK.lock().unwrap();
        let tmp = setup("tail-component");
        let path = crate::app::paths::var_log_instance_dir(ENV_ID, "hadoop", "3.5.0")
            .unwrap()
            .join("hadoop.log");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "1\n2\n3\n4\n5\n").unwrap();

        let lines = tail(&component_source(path), 2).unwrap();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].message, "4");
        assert_eq!(lines[1].message, "5");
        assert!(lines[0].raw);
        assert_eq!(lines[0].environment_id.as_deref(), Some(ENV_ID));

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn tail_parses_structured_app_lines() {
        let _guard = HOME_LOCK.lock().unwrap();
        let tmp = setup("tail-app");
        let dir = crate::app::paths::app_log_dir().unwrap();
        std::fs::create_dir_all(&dir).unwrap();
        let date = crate::app::app_log::today();
        std::fs::write(
            dir.join(format!("solostack.log.{date}")),
            "{\"timestamp\":\"2026-09-20T10:00:00.000+08:00\",\"level\":\"info\",\"message\":\"hello\"}\nraw line\n",
        )
        .unwrap();

        let lines = tail(
            &LogSource::App {
                date: Some(date.clone()),
            },
            10,
        )
        .unwrap();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].message, "hello");
        assert!(!lines[0].raw);
        assert_eq!(lines[0].level, Some(crate::app::app_log::LogLevel::Info));
        assert!(lines[1].raw);
        assert_eq!(lines[1].message, "raw line");

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn tail_clamps_absurd_line_limit() {
        // IPC 传入超大行数不应触发巨量分配，而是收敛到上限
        let _guard = HOME_LOCK.lock().unwrap();
        let tmp = setup("tail-clamp");
        let path = crate::app::paths::var_log_instance_dir(ENV_ID, "hadoop", "3.5.0")
            .unwrap()
            .join("hadoop.log");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "1\n2\n3\n").unwrap();

        let lines = tail(&component_source(path), usize::MAX).unwrap();
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[2].message, "3");

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn backfill_returns_only_the_last_lines_of_a_large_file() {
        let _guard = HOME_LOCK.lock().unwrap();
        let tmp = setup("backfill-window");
        let path = crate::app::paths::var_log_instance_dir(ENV_ID, "hadoop", "3.5.0")
            .unwrap()
            .join("hadoop.log");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut content = String::new();
        for i in 0..5_000 {
            content.push_str(&format!("line-{i}\n"));
        }
        std::fs::write(&path, content).unwrap();

        let mut follower = LogFollower::new(component_source(path.clone()));
        let back = follower.backfill(10).unwrap();
        assert_eq!(back.len(), 10);
        assert_eq!(back[0].message, "line-4990");
        assert_eq!(back[9].message, "line-4999");

        // offset 仍推进到文件末尾：新增内容能被 poll 读到
        std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap()
            .write_all(b"tail\n")
            .unwrap();
        let polled = follower.poll().unwrap();
        assert_eq!(polled.lines.len(), 1);
        assert_eq!(polled.lines[0].message, "tail");

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn file_follower_reads_only_new_bytes() {
        let _guard = HOME_LOCK.lock().unwrap();
        let tmp = setup("incremental");
        let path = crate::app::paths::var_log_instance_dir(ENV_ID, "hadoop", "3.5.0")
            .unwrap()
            .join("component.log");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "first\n").unwrap();
        let mut follower = LogFollower::new(component_source(path.clone()));

        let backfill = follower.backfill(10).unwrap();
        assert_eq!(backfill.len(), 1);
        assert_eq!(backfill[0].message, "first");
        assert!(follower.poll().unwrap().lines.is_empty());

        std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap()
            .write_all(b"second\n")
            .unwrap();
        let polled = follower.poll().unwrap();
        assert_eq!(polled.lines.len(), 1);
        assert_eq!(polled.lines[0].message, "second");

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn file_follower_keeps_partial_line_until_complete() {
        let _guard = HOME_LOCK.lock().unwrap();
        let tmp = setup("partial");
        let path = crate::app::paths::var_log_instance_dir(ENV_ID, "hadoop", "3.5.0")
            .unwrap()
            .join("component.log");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "hel").unwrap();
        let mut follower = LogFollower::new(component_source(path.clone()));
        assert!(follower.backfill(10).unwrap().is_empty());

        std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap()
            .write_all(b"lo\n")
            .unwrap();
        let polled = follower.poll().unwrap();
        assert_eq!(polled.lines.len(), 1);
        assert_eq!(polled.lines[0].message, "hello");

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn file_follower_restarts_after_truncation() {
        let _guard = HOME_LOCK.lock().unwrap();
        let tmp = setup("truncate");
        let path = crate::app::paths::var_log_instance_dir(ENV_ID, "hadoop", "3.5.0")
            .unwrap()
            .join("component.log");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "old-1\nold-2\n").unwrap();
        let mut follower = LogFollower::new(component_source(path.clone()));
        assert_eq!(follower.backfill(10).unwrap().len(), 2);

        std::fs::write(&path, "new\n").unwrap();
        let polled = follower.poll().unwrap();
        assert!(polled.rotated);
        assert_eq!(polled.lines.len(), 1);
        assert_eq!(polled.lines[0].message, "new");

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn app_follower_uses_daily_base_file_not_rotated_parts() {
        let _guard = HOME_LOCK.lock().unwrap();
        let tmp = setup("app-current");
        let dir = crate::app::paths::app_log_dir().unwrap();
        std::fs::create_dir_all(&dir).unwrap();
        let today = crate::app::app_log::today();
        // 轮转分片 `.1` 不应被当成当前文件（当前文件是分片号 0 的 base）
        std::fs::write(dir.join(format!("solostack.log.{today}.1")), "rotated\n").unwrap();
        let base = dir.join(format!("solostack.log.{today}"));
        std::fs::write(&base, "current\n").unwrap();

        let mut follower = LogFollower::new(LogSource::App { date: None });
        let backfill = follower.backfill(10).unwrap();
        assert_eq!(backfill.len(), 1);
        assert_eq!(backfill[0].message, "current");

        // 追加后仍跟随当天 base 文件（跨天切换靠每次取 `log_file()` 重新求今天）
        std::fs::OpenOptions::new()
            .append(true)
            .open(&base)
            .unwrap()
            .write_all(b"next\n")
            .unwrap();
        let polled = follower.poll().unwrap();
        assert_eq!(polled.lines.len(), 1);
        assert_eq!(polled.lines[0].message, "next");

        let _ = std::fs::remove_dir_all(&tmp);
    }
}
