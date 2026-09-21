import { invoke } from "@tauri-apps/api/core";
import type {
  LogLine,
  LogSourceRequest,
  LogStreamBatch,
  LogStreamState,
  LogStreamStatus,
  StartLogStreamResponse,
} from "./types";

const MAX_ROWS = 5_000;
const TAIL_LINES = 2_000;

export const logStream = $state({
  streamId: "",
  state: "stopped" as LogStreamState,
  rows: [] as LogLine[],
  offset: 0,
  dropped: 0,
  error: "",
  /** true = 实时跟随；false = 静态加载（历史日期）。 */
  live: true,
  /** 就地搜索关键字（header 输入，LiveLogView 过滤用）。 */
  query: "",
});

/// 实时跟随一个来源（app 今天 / 组件文件）。
export async function startLogStream(source: LogSourceRequest, backfillLines = 300) {
  await stopLogStream();
  logStream.state = "starting";
  logStream.live = true;
  logStream.error = "";
  logStream.dropped = 0;
  try {
    const response = await invoke<StartLogStreamResponse>("start_log_stream", {
      sources: [source],
      backfillLines,
    });
    logStream.streamId = response.stream_id;
    logStream.rows = response.records.slice(-MAX_ROWS);
    logStream.offset = response.offset;
    logStream.state = "following";
  } catch (error) {
    logStream.state = "error";
    logStream.error = String(error);
    throw error;
  }
}

/// 静态加载某来源末尾若干行（历史日期，不跟随）。
export async function loadLogTail(source: LogSourceRequest, lines = TAIL_LINES) {
  await stopLogStream();
  logStream.state = "starting";
  logStream.live = false;
  logStream.error = "";
  logStream.dropped = 0;
  try {
    const rows = await invoke<LogLine[]>("read_log_tail", { source, lines });
    logStream.rows = rows.slice(-MAX_ROWS);
    logStream.state = "stopped";
  } catch (error) {
    logStream.state = "error";
    logStream.error = String(error);
    throw error;
  }
}

export async function stopLogStream() {
  if (!logStream.streamId) {
    resetStream();
    return;
  }
  const streamId = logStream.streamId;
  resetStream();
  try {
    await invoke("stop_log_stream", { streamId });
  } catch {
    /* 流已结束或后端已释放 */
  }
}

export async function pauseLogStream() {
  if (!logStream.streamId || logStream.state !== "following") return;
  await invoke("pause_log_stream", { streamId: logStream.streamId });
  logStream.state = "paused";
}

export async function resumeLogStream() {
  if (!logStream.streamId || logStream.state !== "paused") return;
  await invoke("resume_log_stream", { streamId: logStream.streamId });
  logStream.state = "following";
}

export function handleLogStreamBatch(batch: LogStreamBatch) {
  if (logStream.state === "starting" && !logStream.streamId) {
    logStream.streamId = batch.stream_id;
  } else if (batch.stream_id !== logStream.streamId) {
    return;
  }
  const rows = [...logStream.rows, ...batch.records];
  if (rows.length > MAX_ROWS) {
    logStream.dropped += rows.length - MAX_ROWS;
    logStream.rows = rows.slice(-MAX_ROWS);
  } else {
    logStream.rows = rows;
  }
}

export function handleLogStreamStatus(status: LogStreamStatus) {
  if (logStream.state === "starting" && !logStream.streamId) {
    logStream.streamId = status.stream_id;
  } else if (status.stream_id !== logStream.streamId) {
    return;
  }
  logStream.state = status.state;
  logStream.offset = status.offset;
  logStream.dropped = Math.max(logStream.dropped, status.dropped);
  logStream.error = status.error ?? "";
}

function resetStream() {
  logStream.streamId = "";
  logStream.state = "stopped";
  logStream.rows = [];
  logStream.offset = 0;
  logStream.dropped = 0;
  logStream.error = "";
  logStream.query = "";
}
