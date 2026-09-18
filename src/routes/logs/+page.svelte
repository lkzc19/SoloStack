<script lang="ts">
  import { goto } from "$app/navigation";
  import { page } from "$app/state";
  import { onDestroy, onMount } from "svelte";
  import LiveLogView from "$lib/LiveLogView.svelte";
  import PageHeader from "$lib/PageHeader.svelte";
  import {
    startAppLogStream,
    startFileLogStream,
    stopLogStream,
  } from "$lib/log-stream.svelte.ts";
  import { basename, store } from "$lib/stores.svelte.ts";

  const source = $derived(page.url.searchParams.get("source") ?? "app");
  const environmentId = $derived(page.url.searchParams.get("environmentId") ?? "");
  const component = $derived(page.url.searchParams.get("component") ?? "");
  const logPath = $derived(page.url.searchParams.get("path") ?? "");
  const componentMode = $derived(source === "component" && Boolean(component));
  const title = $derived(
    componentMode
      ? `${store.components.find((item) => item.name === component)?.display_name ?? component} · ${
          logPath ? basename(logPath) : "日志"
        }`
      : "应用日志"
  );

  onMount(async () => {
    if (!componentMode) {
      await startAppLogStream().catch((error) => {
        store.errorMsg = String(error);
      });
      return;
    }
    if (!logPath) {
      store.errorMsg = "缺少要查看的组件日志文件";
      return;
    }
    try {
      await startFileLogStream(logPath, environmentId);
    } catch (error) {
      store.errorMsg = String(error);
    }
  });

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

<PageHeader {title} onBack={goBack} />

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
