//! 实时日志流命令与 Tauri 事件。

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::Duration;

use serde::Serialize;
use solostack_core::app::id;
use solostack_core::logs::{LogFollower, LogLine, MAX_TAIL_LINES};
use tauri::{AppHandle, Emitter};

use super::logs::LogSourceRequest;

const BATCH_EVENT: &str = "logs-stream://batch";
const STATUS_EVENT: &str = "logs-stream://status";
const POLL_INTERVAL: Duration = Duration::from_millis(300);

static STREAMS: LazyLock<Mutex<HashMap<String, StreamControl>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// 取流注册表；锁中毒时仍取回内部数据（与 core 的一致做法，不二次 panic）。
fn stream_registry() -> std::sync::MutexGuard<'static, HashMap<String, StreamControl>> {
    STREAMS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

struct StreamControl {
    stop: Arc<AtomicBool>,
    paused: Arc<AtomicBool>,
}

#[derive(Clone, Serialize)]
struct LogStreamBatch {
    stream_id: String,
    sequence: u64,
    records: Vec<LogLine>,
    dropped: usize,
}

#[derive(Serialize)]
pub struct StartLogStreamResponse {
    stream_id: String,
    records: Vec<LogLine>,
    offset: u64,
}

#[derive(Clone, Serialize)]
struct LogStreamStatus {
    stream_id: String,
    state: &'static str,
    offset: u64,
    dropped: usize,
    rotated: bool,
    error: Option<String>,
}

/// 开始跟随一个或多个日志文件。
#[tauri::command]
pub fn start_log_stream(
    app: AppHandle,
    sources: Vec<LogSourceRequest>,
    backfill_lines: Option<usize>,
) -> Result<StartLogStreamResponse, String> {
    if sources.is_empty() {
        return Err("日志来源不能为空".to_string());
    }
    let sources = sources
        .into_iter()
        .map(LogSourceRequest::resolve)
        .collect::<Result<Vec<_>, _>>()?;
    let mut followers = sources
        .into_iter()
        .map(LogFollower::new)
        .collect::<Vec<_>>();
    let backfill_lines = backfill_lines.unwrap_or(200).clamp(1, MAX_TAIL_LINES);
    let mut initial = Vec::new();
    for follower in &mut followers {
        initial.extend(follower.backfill(backfill_lines)?);
    }

    let stream_id = id::new_id();
    let stop = Arc::new(AtomicBool::new(false));
    let paused = Arc::new(AtomicBool::new(false));
    stream_registry().insert(
        stream_id.clone(),
        StreamControl {
            stop: Arc::clone(&stop),
            paused: Arc::clone(&paused),
        },
    );
    let offset = followers.iter().map(LogFollower::offset).sum();

    let worker_stream_id = stream_id.clone();
    std::thread::spawn(move || {
        let mut sequence = 0u64;
        let mut had_error = false;
        emit_status(
            &app,
            &worker_stream_id,
            "following",
            &followers,
            0,
            false,
            None,
        );

        while !stop.load(Ordering::SeqCst) {
            if paused.load(Ordering::SeqCst) {
                std::thread::sleep(POLL_INTERVAL);
                continue;
            }
            std::thread::sleep(POLL_INTERVAL);
            if stop.load(Ordering::SeqCst) {
                break;
            }

            let mut records = Vec::new();
            let mut rotated = false;
            let mut first_error = None;
            for follower in &mut followers {
                match follower.poll() {
                    Ok(result) => {
                        records.extend(result.lines);
                        rotated |= result.rotated;
                    }
                    Err(error) => {
                        first_error.get_or_insert(error);
                    }
                }
            }

            if !records.is_empty() {
                let _ = app.emit(
                    BATCH_EVENT,
                    LogStreamBatch {
                        stream_id: worker_stream_id.clone(),
                        sequence,
                        records,
                        dropped: 0,
                    },
                );
                sequence += 1;
            }
            if rotated || first_error.is_some() || had_error {
                let state = if first_error.is_some() {
                    "error"
                } else {
                    "following"
                };
                emit_status(
                    &app,
                    &worker_stream_id,
                    state,
                    &followers,
                    0,
                    rotated,
                    first_error,
                );
                had_error = state == "error";
            }
        }

        emit_status(
            &app,
            &worker_stream_id,
            "stopped",
            &followers,
            0,
            false,
            None,
        );
    });

    Ok(StartLogStreamResponse {
        stream_id,
        records: initial,
        offset,
    })
}

/// 停止日志流并释放 follower。
#[tauri::command]
pub fn stop_log_stream(stream_id: String) -> Result<(), String> {
    let control = stream_registry()
        .remove(&stream_id)
        .ok_or_else(|| "日志流不存在".to_string())?;
    control.stop.store(true, Ordering::SeqCst);
    Ok(())
}

/// 暂停向前端推送，文件 offset 保持不变。
#[tauri::command]
pub fn pause_log_stream(stream_id: String) -> Result<(), String> {
    let streams = stream_registry();
    let control = streams
        .get(&stream_id)
        .ok_or_else(|| "日志流不存在".to_string())?;
    control.paused.store(true, Ordering::SeqCst);
    Ok(())
}

/// 恢复日志流。
#[tauri::command]
pub fn resume_log_stream(stream_id: String) -> Result<(), String> {
    let streams = stream_registry();
    let control = streams
        .get(&stream_id)
        .ok_or_else(|| "日志流不存在".to_string())?;
    control.paused.store(false, Ordering::SeqCst);
    Ok(())
}

fn emit_status(
    app: &AppHandle,
    stream_id: &str,
    state: &'static str,
    followers: &[LogFollower],
    dropped: usize,
    rotated: bool,
    error: Option<String>,
) {
    let _ = app.emit(
        STATUS_EVENT,
        LogStreamStatus {
            stream_id: stream_id.to_string(),
            state,
            offset: followers.iter().map(LogFollower::offset).sum(),
            dropped,
            rotated,
            error,
        },
    );
}
