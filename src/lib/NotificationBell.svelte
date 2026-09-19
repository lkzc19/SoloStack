<script lang="ts">
  import { Bell } from "lucide-svelte";
  import { goto } from "$app/navigation";
  import { hasUnreadStore } from "$lib/notifications.svelte.ts";

  let hasUnread = $state(false);
  hasUnreadStore.subscribe((v) => (hasUnread = v));
</script>

<button
  class="gear-btn ghost notif-bell"
  onclick={() => goto("/notifications")}
  aria-label="通知"
>
  <span class="icon-wrap">
    <Bell size={16} />
    {#if hasUnread}
      <span class="dot"></span>
    {/if}
  </span>
</button>

<style>
  .notif-bell {
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .icon-wrap {
    position: relative;
    display: flex;
  }

  .dot {
    position: absolute;
    top: -3px;
    right: -3px;
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: #dc2626;
    pointer-events: none;
  }
</style>
