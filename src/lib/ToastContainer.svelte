<script lang="ts">
  import { X, Info, TriangleAlert, CircleX } from "lucide-svelte";
  import { toasts, dismissToast, toastConfig } from "$lib/notifications.svelte.ts";
  import type { NotificationLevel } from "$lib/types";

  const levelIcon: Record<NotificationLevel, typeof Info> = {
    info: Info,
    warn: TriangleAlert,
    error: CircleX,
  };
</script>

<div class="toast-container position-{toastConfig.position}">
  {#each toasts as toast (toast.id)}
    <div class="toast toast-{toast.level}">
      <span class="toast-icon toast-icon-{toast.level}"><svelte:component this={levelIcon[toast.level]} size={20} /></span>
      <div class="toast-body">
        <div class="toast-title">{toast.title}</div>
        {#if toast.message}
          <div class="toast-msg">{toast.message}</div>
        {/if}
      </div>
      <button class="toast-close" onclick={() => dismissToast(toast.id)}>
        <X size={14} />
      </button>
    </div>
  {/each}
</div>

<style>
  .toast-container {
    position: fixed;
    z-index: 9999;
    display: flex;
    flex-direction: column;
    gap: 8px;
    pointer-events: none;
    max-width: 360px;
  }

  .toast-container.position-top-right {
    top: 12px;
    right: 12px;
  }
  .toast-container.position-bottom-left {
    bottom: 12px;
    left: 12px;
  }
  .toast-container.position-bottom-right {
    bottom: 12px;
    right: 12px;
  }

  .toast {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    padding: 10px 12px;
    border-radius: 8px;
    background: var(--panel);
    border: 1px solid var(--line);
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.08);
    pointer-events: auto;
    animation: toast-in 0.2s ease-out;
  }

  @keyframes toast-in {
    from { opacity: 0; transform: translateY(-8px); }
    to   { opacity: 1; transform: translateY(0); }
  }

  .toast-icon {
    flex-shrink: 0;
    display: flex;
    align-items: flex-start;
    padding-top: 1px;
  }

  .toast-icon-info { color: #2563eb; }
  .toast-icon-warn { color: #d97706; }
  .toast-icon-error { color: #dc2626; }

  .toast-body {
    flex: 1;
    min-width: 0;
  }

  .toast-title {
    font-size: 13px;
    font-weight: 600;
    color: var(--text);
    line-height: 1.3;
  }

  .toast-msg {
    font-size: 12px;
    color: var(--muted);
    margin-top: 2px;
    line-height: 1.35;
    display: -webkit-box;
    -webkit-line-clamp: 3;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }

  .toast-close {
    flex-shrink: 0;
    background: none;
    border: none;
    padding: 2px;
    cursor: pointer;
    color: var(--muted);
    opacity: 0.6;
    transition: opacity 0.15s;
  }
  .toast-close:hover { opacity: 1; }
</style>
