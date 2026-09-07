<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { goto } from "$app/navigation";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { onMount } from "svelte";
  import {
    AlertCircle,
    Database,
    FileText,
    HardDrive,
    Play,
    Plus,
    Settings,
    SlidersHorizontal,
    Square,
  } from "lucide-svelte";
  import ComponentLogo from "$lib/ComponentLogo.svelte";
  import Button from "$lib/components/ui/button/button.svelte";
  import { store, startComponent, stopComponent } from "$lib/stores.svelte.ts";

  interface WebUiUrl {
    name: string;
    url: string;
  }

  let webUis = $state<Record<string, WebUiUrl[]>>({});

  onMount(() => {
    loadWebUis();
  });

  async function loadWebUis() {
    try {
      const urls = await invoke<WebUiUrl[]>("get_web_ui_urls", { component: "hadoop" });
      webUis = { hadoop: urls };
    } catch {
      /* 未安装 hadoop 时静默 */
    }
  }

  async function openWebUi(url: string) {
    try {
      await openUrl(url);
    } catch (e) {
      store.errorMsg = String(e);
    }
  }

  function openSettings() {
    goto("/settings");
  }

  function openInstall() {
    goto("/install");
  }

  function openInstallProgress() {
    goto("/install/progress");
  }

  function openComponentConfig(name: string) {
    goto(`/component/${name}/config`);
  }

  function openComponentLogs(name: string) {
    goto(`/component/${name}/logs`);
  }
</script>

<header class="topbar" data-tauri-drag-region>
  <div class="brand">
    <span class="brand-name">SoloStack</span>
  </div>
  <button class="gear-btn ghost" onclick={openSettings} aria-label="设置">
    <Settings size={16} />
  </button>
  <div class="topbar-spacer"></div>
  {#if store.installing}
    <button class="add-btn progress" onclick={openInstallProgress} aria-label="查看安装进度">
      <svg class="ring" viewBox="0 0 36 36">
        <circle class="ring-bg" cx="18" cy="18" r="15"></circle>
        <circle
          class="ring-fg"
          cx="18"
          cy="18"
          r="15"
          style={`stroke-dashoffset: ${94.25 * (1 - store.installPct / 100)}`}
        ></circle>
      </svg>
    </button>
  {:else if store.installFailed}
    <button class="add-btn failed" onclick={openInstallProgress} aria-label="查看安装失败">
      <AlertCircle size={18} />
    </button>
  {:else}
    <button class="add-btn" onclick={openInstall} aria-label="添加组件">
      <Plus size={18} />
    </button>
  {/if}
</header>

<main class="content">
  <div class="component-list">
    {#each store.components as component, i (component.name)}
      <div class="component-row" style={"--i:" + i}>
        <span class="component-logo {component.status}">
          {#if component.name === "hadoop" || component.name === "kafka"}
            <ComponentLogo name={component.name} />
          {:else}
            <Database size={20} />
          {/if}
        </span>
        <div class="component-info">
          <div class="component-title-row">
            <span class="component-name">{component.display_name || component.name}</span>
            {#if component.name === "hadoop" && webUis.hadoop?.length}
              <span class="webui-sep">|</span>
              {#each webUis.hadoop as wu (wu.name)}
                <button class="webui-badge {component.status}" onclick={() => openWebUi(wu.url)} title={`打开 ${wu.name} WebUI`}>
                  {wu.name}
                </button>
              {/each}
            {/if}
          </div>
          <span class="component-meta mono">v{component.version} · {component.statusText}</span>
        </div>
        <div class="row-actions">
          {#if component.status === "running" || component.status === "partial"}
            {@const stopping = store.busy && store.busyAction === "stop" && store.busyComponent === component.name}
            <Button
              variant="destructive"
              size="sm"
              class="w-auto"
              onclick={() => stopComponent(component.name)}
              disabled={store.busy}
            >
              {#if stopping}
                <span class="log-spinner"></span>
              {:else}
                <Square size={14} />
              {/if}
              {stopping ? "停止中…" : "停止"}
            </Button>
          {:else}
            {@const starting = store.busy && store.busyAction === "start" && store.busyComponent === component.name}
            <Button
              size="sm"
              class="w-auto"
              onclick={() => startComponent(component.name)}
              disabled={store.busy}
            >
              {#if starting}
                <span class="log-spinner"></span>
              {:else}
                <Play size={14} />
              {/if}
              {starting ? "启动中…" : "启动"}
            </Button>
          {/if}
          <button class="icon-btn" onclick={() => openComponentConfig(component.name)} title="配置">
            <SlidersHorizontal size={15} />
          </button>
          <button class="icon-btn" onclick={() => openComponentLogs(component.name)} title="日志">
            <FileText size={15} />
          </button>
        </div>
      </div>
    {/each}
    {#if store.components.length === 0}
      <div class="empty">
        <span class="empty-glyph"><HardDrive size={30} /></span>
        <p class="big">暂无组件</p>
        <p>点击右上角 + 安装组件</p>
      </div>
    {/if}
  </div>
</main>
