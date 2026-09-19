<script lang="ts">
  import { Trash2, X, Info, TriangleAlert, CircleX } from "lucide-svelte";
  import PageHeader from "$lib/PageHeader.svelte";
  import Button from "$lib/components/ui/button/button.svelte";
  import {
    notifications,
    markSeen,
    clearAll,
    deleteNotification,
  } from "$lib/notifications.svelte.ts";
  import type { NotificationLevel } from "$lib/types";

  import { onMount } from "svelte";
  onMount(() => { markSeen(); });
  const levelIcon: Record<NotificationLevel, typeof Info> = {
    info: Info,
    warn: TriangleAlert,
    error: CircleX,
  };

  function relativeTime(iso: string): string {
    const diff = Date.now() - new Date(iso).getTime();
    if (diff < 60_000) return "刚刚";
    if (diff < 3_600_000) return `${Math.floor(diff / 60_000)} 分钟前`;
    if (diff < 86_400_000) return `${Math.floor(diff / 3_600_000)} 小时前`;
    return `${Math.floor(diff / 86_400_000)} 天前`;
  }
</script>

<PageHeader title="通知">
  {#snippet actions()}
    <Button variant="outline" size="sm" onclick={clearAll} disabled={notifications.items.length === 0}>
      <Trash2 size={14} />
      清空
    </Button>
  {/snippet}
</PageHeader>

<main class="notif-page">
  {#if notifications.items.length === 0}
    <div class="notif-empty">
      <p class="muted">暂无通知</p>
    </div>
  {:else}
    <div class="notif-list">
      {#each notifications.items as n (n.id)}
        <div class="notif-item">
          <span class="notif-icon level-{n.level}">
            <svelte:component this={levelIcon[n.level]} size={16} />
          </span>
          <div class="notif-body">
            <div class="notif-title-row">
              <span class="notif-title">{n.title}</span>
              <span class="notif-sep">|</span>
              <span class="notif-time">{relativeTime(n.created_at)}</span>
            </div>
            {#if n.message}
              <span class="notif-message">{n.message}</span>
            {/if}
          </div>
          <button
            class="notif-delete"
            onclick={(e) => { e.stopPropagation(); deleteNotification(n.id); }}
            title="删除"
          >
            <X size={14} />
          </button>
        </div>
      {/each}
    </div>
  {/if}
</main>

<style>
  .notif-page {
    flex: 1;
    min-height: 0;
    overflow: hidden;
    display: flex;
    flex-direction: column;
    padding: 0 2rem 2rem;
  }

  .notif-empty {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .notif-list {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 0.8rem;
  }

  .notif-item {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 1rem 1.25rem;
    background: var(--panel);
    border: 1px solid var(--line);
    border-radius: var(--radius);
    transition: border-color 0.2s ease-in-out;
  }
  .notif-item:hover { border-color: var(--primary); }

  .notif-icon {
    flex-shrink: 0;
    width: 34px;
    height: 34px;
    border: 1px solid var(--line);
    border-radius: 8px;
    background: transparent;
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .notif-icon.level-info { color: var(--blue); }
  .notif-icon.level-warn { color: var(--accent); }
  .notif-icon.level-error { color: var(--red); }

  .notif-body {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .notif-title-row {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .notif-title {
    font-size: 13px;
    font-weight: 600;
    color: var(--text-hi);
    line-height: 1.4;
  }

  .notif-sep {
    color: var(--line);
    user-select: none;
  }

  .notif-time {
    font-size: 11px;
    color: var(--muted);
    flex-shrink: 0;
  }

  .notif-message {
    font-size: 12px;
    color: var(--muted);
    line-height: 1.4;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }

  .notif-delete {
    flex-shrink: 0;
    background: none;
    border: none;
    padding: 4px;
    cursor: pointer;
    color: var(--muted);
    opacity: 0.5;
    transition: opacity 0.15s, color 0.15s;
  }
  .notif-delete:hover { opacity: 1; color: var(--red); }
</style>
