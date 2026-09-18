import { invoke } from "@tauri-apps/api/core";
import type {
  LogStreamBatch,
  LogStreamState,
  LogStreamStatus,
  StartLogStreamResponse,
  StreamLogLine,
} from "./types";

const MAX_ROWS = 5_000;

export const logStream = $state({
  mode: "app" as "app" | "component",
  streamId: "",
  state: "stopped" as LogStreamState,
  rows: [] as StreamLogLine[],
  offset: 0,
  dropped: 0,
  error: "",
});

export async function startAppLogStream(backfillLines = 300) {
  await startLogStream("app", [{ kind: "app_log" }], backfillLines);
}

export async function startFileLogStream(
  path: string,
  environmentId?: string,
  backfillLines = 300
) {
  await startLogStream(
    "component",
    [{ kind: "file", path, environment_id: environmentId }],
    backfillLines
  );
}

async function startLogStream(
  mode: "app" | "component",
  sources: unknown[],
  backfillLines: number
) {
  await stopLogStream();
  logStream.mode = mode;
  logStream.state = "starting";
  logStream.rows = [];
  logStream.error = "";
  logStream.dropped = 0;
  try {
    const response = await invoke<StartLogStreamResponse>("start_log_stream", {
      sources,
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

export function clearLogStreamRows() {
  logStream.rows = [];
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
}
