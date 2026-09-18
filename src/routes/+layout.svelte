<script lang="ts">
  import "../app.css";
  import { onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { boot, refreshComponents, handleInstallProgress } from "$lib/stores.svelte.ts";
  import {
    handleLogStreamBatch,
    handleLogStreamStatus,
    stopLogStream,
  } from "$lib/log-stream.svelte.ts";
  import type {
    InstallProgressPayload,
    LogStreamBatch,
    LogStreamStatus,
  } from "$lib/types";

  let { children } = $props();

  onMount(() => {
    boot();
    const pollTimer = setInterval(refreshComponents, 10000);
    let unlistenInstall: (() => void) | undefined;
    let unlistenBatch: (() => void) | undefined;
    let unlistenStatus: (() => void) | undefined;
    listen<InstallProgressPayload>("install-progress", (e) =>
      handleInstallProgress(e.payload)
    ).then((u) => (unlistenInstall = u));
    listen<LogStreamBatch>("logs-stream://batch", (e) =>
      handleLogStreamBatch(e.payload)
    ).then((u) => (unlistenBatch = u));
    listen<LogStreamStatus>("logs-stream://status", (e) =>
      handleLogStreamStatus(e.payload)
    ).then((u) => (unlistenStatus = u));
    return () => {
      clearInterval(pollTimer);
      void stopLogStream();
      unlistenInstall?.();
      unlistenBatch?.();
      unlistenStatus?.();
    };
  });
</script>

<div class="app">
  {@render children()}
</div>
