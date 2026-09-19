<script lang="ts">
  import { Pause, Play, Trash2 } from "lucide-svelte";
  import Button from "$lib/components/ui/button/button.svelte";
  import {
    logStream,
    pauseLogStream,
    resumeLogStream,
  } from "$lib/log-stream.svelte.ts";

  let viewport = $state<HTMLDivElement | null>(null);
  let autoFollow = $state(true);
  let traceQuery = $state("");

  const filteredRows = $derived(
    traceQuery.trim()
      ? logStream.rows.filter((row) => {
          const query = traceQuery.trim().toLowerCase();
          return [
            row.timestamp ?? "",
            row.trace_id ?? "",
            row.level,
            row.environment_id ?? "",
            row.message,
          ].some((value) => value.toLowerCase().includes(query));
        })
      : logStream.rows
  );
  $effect(() => {
    filteredRows.length;
    if (!autoFollow || !viewport) return;
    queueMicrotask(() => {
      if (viewport && autoFollow) viewport.scrollTop = viewport.scrollHeight;
    });
  });

  function onScroll() {
    if (!viewport) return;
    const distance = viewport.scrollHeight - viewport.scrollTop - viewport.clientHeight;
    autoFollow = distance < 36;
  }
</script>

<div class="live-log">
  <div class="logs-toolbar">
    <input
      class="input mono search-input"
      bind:value={traceQuery}
      placeholder="搜索关键词"
      spellcheck="false"
    />
    <div class="logs-toolbar-spacer"></div>
    {#if logStream.state === "paused"}
      <Button variant="outline" size="sm" onclick={resumeLogStream}>
        <Play size={14} />
        恢复
      </Button>
    {:else}
      <Button
        variant="outline"
        size="sm"
        onclick={pauseLogStream}
        disabled={logStream.state !== "following"}
      >
        <Pause size={14} />
        暂停
      </Button>
    {/if}
    <Button
      variant="outline"
      size="sm"
      onclick={() => (logStream.rows = [])}
    >
      <Trash2 size={14} />
      清空
    </Button>
  </div>

  {#if logStream.error}
    <p class="msg error live-log-error">{logStream.error}</p>
  {/if}

  <div class="live-log-content">
    <div class="live-log-viewport mono" bind:this={viewport} onscroll={onScroll}>
      {#if filteredRows.length === 0}
        <div class="live-log-empty">暂无日志</div>
      {:else if logStream.mode === "component"}
        {#each filteredRows as row}
          <div class="live-log-raw-line">{row.message}</div>
        {/each}
      {:else}
        {#each filteredRows as row}
          <div class="live-log-row">
            <span class="live-log-time">{row.timestamp ?? "-"}</span>
            <span class="live-log-trace">{row.trace_id ?? "-"}</span>
            <span class="live-log-level {row.level}">
              {row.level.toUpperCase()}
            </span>
            <span class="live-log-env">{row.environment_id ?? "-"}</span>
            <span class="live-log-message">{row.message}</span>
          </div>
        {/each}
      {/if}
    </div>
  </div>
</div>

<style>
  .live-log {
    display: flex;
    height: 100%;
    min-height: 0;
    flex-direction: column;
  }

  .live-log-error {
    margin: 0 2rem 10px;
  }

  .live-log-content {
    display: flex;
    flex: 1;
    min-height: 0;
    padding: 0 2rem 2rem;
    box-sizing: border-box;
  }

  .live-log-viewport {
    flex: 1;
    min-height: 0;
    overflow: auto;
    border: 1px solid var(--line);
    border-radius: 7px;
    background: var(--panel);
    font-size: 12px;
  }

  .live-log-empty {
    padding: 16px;
    color: var(--muted);
  }

  .live-log-row {
    display: block;
    padding: 5px 10px;
    line-height: 1.55;
    border-bottom: 1px solid color-mix(in srgb, var(--line) 55%, transparent);
  }

  .live-log-time,
  .live-log-trace,
  .live-log-env {
    margin-right: 8px;
    color: var(--muted);
    white-space: nowrap;
  }

  .live-log-level {
    margin-right: 8px;
    font-weight: 700;
  }

  .live-log-level.error {
    color: #dc2626;
  }

  .live-log-level.warn {
    color: #d97706;
  }

  .live-log-level.info {
    color: #2563eb;
  }

  .live-log-level.debug,
  .live-log-level.trace {
    color: var(--muted);
  }

  .live-log-message {
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }

  .live-log-raw-line {
    display: block;
    margin: 0;
    padding: 5px 10px;
    color: var(--text);
    font: inherit;
    line-height: 1.55;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    border-bottom: 1px solid color-mix(in srgb, var(--line) 55%, transparent);
  }
</style>
