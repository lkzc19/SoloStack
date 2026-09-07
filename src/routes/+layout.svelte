<script lang="ts">
  import "../app.css";
  import { onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { boot, refreshComponents, handleInstallProgress } from "$lib/stores.svelte.ts";
  import type { InstallProgressPayload } from "$lib/types";

  let { children } = $props();

  onMount(() => {
    boot();
    const pollTimer = setInterval(refreshComponents, 10000);
    let unlisten: (() => void) | undefined;
    listen<InstallProgressPayload>("install-progress", (e) =>
      handleInstallProgress(e.payload)
    ).then((u) => (unlisten = u));
    return () => {
      clearInterval(pollTimer);
      unlisten?.();
    };
  });
</script>

<div class="app">
  {@render children()}
</div>
