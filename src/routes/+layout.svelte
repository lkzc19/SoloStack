<script lang="ts">
  import "../app.css";
  import { onMount } from "svelte";
  import { page } from "$app/state";
  import { listen } from "@tauri-apps/api/event";
  import { boot, refreshComponents, handleInstallProgress } from "$lib/stores.svelte.ts";
  import { initNotifications, cleanupNotifications } from "$lib/notifications.svelte.ts";
  import ToastContainer from "$lib/ToastContainer.svelte";
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

  /** 只有主页 / 组件页 / 安装页会展示组件运行状态，其余页面无需轮询。 */
  function needsComponentPolling(pathname: string): boolean {
    return (
      pathname === "/" ||
      pathname.startsWith("/component") ||
      pathname.startsWith("/install")
    );
  }

  onMount(() => {
    boot();
    initNotifications();

    const pollTimer = setInterval(() => {
      if (needsComponentPolling(page.url.pathname)) void refreshComponents();
    }, 10000);

    // 监听器注册是异步的；用 cancelled 标记避免「注册完成前就卸载」导致监听器泄漏
    let cancelled = false;
    const unlisteners: (() => void)[] = [];
    const track = (pending: Promise<() => void>) =>
      pending.then((unlisten) => {
        if (cancelled) unlisten();
        else unlisteners.push(unlisten);
      });
    track(
      listen<InstallProgressPayload>("install-progress", (e) =>
        handleInstallProgress(e.payload)
      )
    );
    track(
      listen<LogStreamBatch>("logs-stream://batch", (e) =>
        handleLogStreamBatch(e.payload)
      )
    );
    track(
      listen<LogStreamStatus>("logs-stream://status", (e) =>
        handleLogStreamStatus(e.payload)
      )
    );

    return () => {
      cancelled = true;
      clearInterval(pollTimer);
      cleanupNotifications();
      void stopLogStream();
      for (const unlisten of unlisteners) unlisten();
    };
  });
</script>

<div class="app">
  {@render children()}
  <ToastContainer />
</div>
