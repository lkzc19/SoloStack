<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { goto } from "$app/navigation";
  import { page } from "$app/state";
  import { onDestroy, onMount } from "svelte";
  import { Pause, Play } from "lucide-svelte";
  import Button from "$lib/components/ui/button/button.svelte";
  import DatePicker from "$lib/components/date-picker.svelte";
  import LiveLogView from "$lib/LiveLogView.svelte";
  import PageHeader from "$lib/PageHeader.svelte";
  import SearchToggle from "$lib/SearchToggle.svelte";
  import {
    loadLogTail,
    logStream,
    pauseLogStream,
    resumeLogStream,
    startLogStream,
    stopLogStream,
  } from "$lib/log-stream.svelte.ts";
  import { basename, store } from "$lib/stores.svelte.ts";
  import type { LogSourceRequest } from "$lib/types";

  const source = $derived(page.url.searchParams.get("source") ?? "app");
  const environmentId = $derived(page.url.searchParams.get("environmentId") ?? "");
  const component = $derived(page.url.searchParams.get("component") ?? "");
  const logPath = $derived(page.url.searchParams.get("path") ?? "");
  const componentMode = $derived(source === "component" && Boolean(component));

  /** 本地日期 YYYY-MM-DD（避免 toISOString 的 UTC 偏移）。 */
  function localDate(): string {
    const now = new Date();
    return new Date(now.getTime() - now.getTimezoneOffset() * 60_000)
      .toISOString()
      .slice(0, 10);
  }
  const today = localDate();

  // 默认今天 = 实时跟随；选历史日期 = 静态加载
  let selectedDate = $state(today);
  let dates = $state<string[]>([]);
  const oldestDate = $derived(dates.length ? dates[dates.length - 1] : undefined);
  const isLive = $derived(!componentMode && selectedDate === today);

  const title = $derived(
    componentMode
      ? `${store.components.find((item) => item.name === component)?.display_name ?? component} · ${
          logPath ? basename(logPath) : "日志"
        }`
      : "应用日志"
  );

  function currentSource(): LogSourceRequest {
    if (componentMode) {
      return {
        kind: "component",
        environment_id: environmentId,
        component,
        path: logPath,
      };
    }
    return isLive ? { kind: "app_log" } : { kind: "app_log", date: selectedDate };
  }

  async function openSource() {
    const request = currentSource();
    // 组件日志与 app 的"今天"走流式；历史日期静态加载
    if (componentMode || isLive) {
      await startLogStream(request);
    } else {
      await loadLogTail(request);
    }
  }

  onMount(async () => {
    if (!componentMode) {
      try {
        dates = await invoke<string[]>("list_app_log_dates");
      } catch {
        dates = [];
      }
    }
    if (componentMode && !logPath) {
      store.errorMsg = "缺少要查看的组件日志文件";
      return;
    }
    await openSource().catch((error) => {
      store.errorMsg = String(error);
    });
  });

  async function onDatePick(date: string) {
    selectedDate = date;
    await openSource().catch((error) => {
      store.errorMsg = String(error);
    });
  }

  function goBack() {
    if (componentMode) {
      goto(`/component/${component}/logs`);
    } else {
      goto("/");
    }
  }

  onDestroy(() => {
    void stopLogStream();
  });
</script>

<PageHeader {title} onBack={goBack}>
  {#snippet actions()}
    <SearchToggle bind:value={logStream.query} />
    {#if logStream.live}
      {#if logStream.state === "paused"}
        <Button
          variant="outline"
          size="sm"
          class="header-btn"
          onclick={resumeLogStream}
        >
          <Play size={14} />
          恢复
        </Button>
      {:else}
        <Button
          variant="outline"
          size="sm"
          class="header-btn"
          onclick={pauseLogStream}
          disabled={logStream.state !== "following"}
        >
          <Pause size={14} />
          暂停
        </Button>
      {/if}
    {/if}
    {#if !componentMode}
      <DatePicker
        value={selectedDate}
        min={oldestDate}
        max={today}
        class="header-btn"
        onPick={onDatePick}
      />
    {/if}
  {/snippet}
</PageHeader>

<main class="logs-page-shell">
  <div class="logs-page-body">
    <LiveLogView />
  </div>
</main>

<style>
  .logs-page-shell {
    flex: 1;
    min-height: 0;
    overflow: hidden;
    padding: 0;
  }

  .logs-page-body {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
  }
</style>
