<script lang="ts">
  // 日志视口：只负责渲染与自动跟随；筛选/暂停等控件在页头（见 routes/logs）。
  import { logStream } from "$lib/log-stream.svelte.ts";

  let viewport = $state<HTMLDivElement | null>(null);
  let autoFollow = $state(true);

  const filteredRows = $derived(
    logStream.query.trim()
      ? logStream.rows.filter((row) => {
          const query = logStream.query.trim().toLowerCase();
          return [
            row.timestamp ?? "",
            row.trace_id ?? "",
            row.level ?? "",
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
  {#if logStream.error}
    <p class="msg error live-log-error">{logStream.error}</p>
  {/if}

  <div class="live-log-content">
    <div class="live-log-viewport mono" bind:this={viewport} onscroll={onScroll}>
      {#if filteredRows.length === 0}
        <div class="live-log-empty">暂无日志</div>
      {:else}
        {#each filteredRows as row}
          {#if row.raw}
            <div class="live-log-raw-line">{row.message}</div>
          {:else}
            <div class="live-log-row">
              <span class="live-log-time">{row.timestamp ?? "-"}</span>
              <span class="live-log-trace">{row.trace_id ?? "-"}</span>
              <span class="live-log-level {row.level ?? ""}">
                {(row.level ?? "").toUpperCase()}
              </span>
              <span class="live-log-env">{row.environment_id ?? "-"}</span>
              <span class="live-log-message">{row.message}</span>
            </div>
          {/if}
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
