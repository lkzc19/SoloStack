//! 实时日志文件跟随。

use std::collections::VecDeque;
use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;

use serde::Serialize;

use crate::app::app_log::{self, LogLevel, LogRecord};

const MAX_PARTIAL_LINE_BYTES: usize = 1024 * 1024;

/// 实时日志来源。
#[derive(Debug, Clone)]
pub enum LogSourceSpec {
    /// SoloStack 应用日志，始终跟随当天最新文件。
    AppLog,
    /// 组件或脚本原始文本日志。
    File {
        path: PathBuf,
        environment_id: Option<String>,
    },
}

/// 推送给前端的单条日志。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StreamLine {
    pub timestamp: Option<String>,
    pub trace_id: Option<String>,
    pub level: LogLevel,
    pub environment_id: Option<String>,
    pub message: String,
}

/// 一轮读取结果。
#[derive(Debug, Clone, Default)]
pub struct PollResult {
    pub lines: Vec<StreamLine>,
    pub rotated: bool,
}

/// 一个文件来源的 offset 跟随器。
pub struct LogFollower {
    source: LogSourceSpec,
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
    pub fn new(source: LogSourceSpec) -> Self {
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
    pub fn backfill(&mut self, lines: usize) -> Result<Vec<StreamLine>, String> {
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

        let bytes =
            std::fs::read(&path).map_err(|e| format!("读取日志 {} 失败: {e}", path.display()))?;
        let complete_len = if bytes.last().is_some_and(|byte| *byte == b'\n') {
            bytes.len()
        } else {
            bytes
                .iter()
                .rposition(|byte| *byte == b'\n')
                .map(|position| position + 1)
                .unwrap_or(0)
        };
        if complete_len < bytes.len() {
            self.partial = bytes[complete_len..].to_vec();
        }
        let mut tail: VecDeque<&[u8]> = VecDeque::with_capacity(lines.max(1));
        for line in bytes[..complete_len].split(|byte| *byte == b'\n') {
            if line.is_empty() {
                continue;
            }
            if tail.len() == lines.max(1) {
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
            lines.push(raw_line(
                format!("{text}...[单行过长，已截断]"),
                &self.source,
            ));
        }

        Ok(PollResult { lines, rotated })
    }

    fn current_path(&self) -> Result<Option<PathBuf>, String> {
        match &self.source {
            LogSourceSpec::AppLog => Ok(app_log::list_log_files()?.into_iter().last()),
            LogSourceSpec::File { path, .. } => Ok(path.is_file().then(|| path.clone())),
        }
    }
}

fn parse_line(line: &str, source: &LogSourceSpec) -> Option<StreamLine> {
    if line.trim().is_empty() {
        return None;
    }
    if matches!(source, LogSourceSpec::AppLog) {
        if let Ok(record) = serde_json::from_str::<LogRecord>(line) {
            return Some(StreamLine {
                timestamp: Some(record.timestamp),
                trace_id: record.trace_id,
                level: record.level,
                environment_id: record.environment_id,
                message: record.message,
            });
        }
    }
    Some(raw_line(line.to_string(), source))
}

fn raw_line(message: String, source: &LogSourceSpec) -> StreamLine {
    let environment_id = match source {
        LogSourceSpec::AppLog => None,
        LogSourceSpec::File { environment_id, .. } => environment_id.clone(),
    };
    StreamLine {
        timestamp: None,
        trace_id: None,
        level: LogLevel::Info,
        environment_id,
        message,
    }
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

    fn setup(name: &str) -> PathBuf {
        let tmp = std::env::temp_dir().join(format!("solostack-log-stream-{name}"));
        std::env::set_var("HOME", &tmp);
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        tmp
    }

    #[test]
    fn file_follower_reads_only_new_bytes() {
        let _guard = HOME_LOCK.lock().unwrap();
        let tmp = setup("incremental");
        let path = tmp.join("component.log");
        std::fs::write(&path, "first\n").unwrap();
        let mut follower = LogFollower::new(LogSourceSpec::File {
            path: path.clone(),
            environment_id: Some("B2xQ7mNp".to_string()),
        });

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
        let path = tmp.join("component.log");
        std::fs::write(&path, "hel").unwrap();
        let mut follower = LogFollower::new(LogSourceSpec::File {
            path: path.clone(),
            environment_id: None,
        });
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
        let path = tmp.join("component.log");
        std::fs::write(&path, "old-1\nold-2\n").unwrap();
        let mut follower = LogFollower::new(LogSourceSpec::File {
            path: path.clone(),
            environment_id: None,
        });
        assert_eq!(follower.backfill(10).unwrap().len(), 2);

        std::fs::write(&path, "new\n").unwrap();
        let polled = follower.poll().unwrap();
        assert!(polled.rotated);
        assert_eq!(polled.lines.len(), 1);
        assert_eq!(polled.lines[0].message, "new");

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn app_log_follower_switches_to_new_daily_file() {
        let _guard = HOME_LOCK.lock().unwrap();
        let tmp = setup("app-rotation");
        let dir = crate::app::paths::app_log_dir().unwrap();
        std::fs::create_dir_all(&dir).unwrap();
        let first = dir.join("solostack.log.2026-09-17");
        std::fs::write(&first, "old\n").unwrap();
        let mut follower = LogFollower::new(LogSourceSpec::AppLog);
        assert_eq!(follower.backfill(10).unwrap().len(), 1);

        let second = dir.join("solostack.log.2026-09-18");
        std::fs::write(&second, "new\n").unwrap();
        let polled = follower.poll().unwrap();
        assert!(polled.rotated);
        assert_eq!(polled.lines.len(), 1);
        assert_eq!(polled.lines[0].message, "new");

        let _ = std::fs::remove_dir_all(&tmp);
    }
}
