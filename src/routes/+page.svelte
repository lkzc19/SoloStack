<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { goto } from "$app/navigation";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import {
    AlertCircle,
    FileText,
    HardDrive,
    Play,
    Plus,
    Settings,
    SlidersHorizontal,
    Square,
  } from "lucide-svelte";
  import { componentAdapter } from "$lib/component-adapters/registry";
  import Button from "$lib/components/ui/button/button.svelte";
  import EnvironmentSwitcher from "$lib/EnvironmentSwitcher.svelte";
  import { store, startComponent, stopComponent } from "$lib/stores.svelte.ts";

  interface WebUiUrl {
    name: string;
    url: string;
  }

  let webUis = $state<Record<string, WebUiUrl[]>>({});

  // 组件列表就绪后逐个拉取各自的 WebUI 入口（已拉过的不重复）
  $effect(() => {
    for (const c of store.components) {
      const key = `${store.activeEnvironmentId}:${c.name}`;
      if (!(key in webUis)) {
        void loadWebUis(c.name);
      }
    }
  });

  async function loadWebUis(name: string) {
    const key = `${store.activeEnvironmentId}:${name}`;
    try {
      const urls = await invoke<WebUiUrl[]>("get_web_ui_urls", {
        environmentId: store.activeEnvironmentId,
        component: name,
      });
      webUis[key] = urls;
    } catch {
      /* 该组件无 WebUI 或尚未就绪时静默 */
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

  function openLogs() {
    goto("/logs");
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
  <div class="topbar-system-actions">
    <button class="gear-btn ghost" onclick={openSettings} aria-label="设置">
      <Settings size={16} />
    </button>
    {#if store.showLogsButton}
      <button class="gear-btn ghost" onclick={openLogs} aria-label="日志">
        <FileText size={16} />
      </button>
    {/if}
  </div>
  <div class="topbar-spacer"></div>
  {#if store.installing}
    <button class="add-btn progress" onclick={openInstallProgress} aria-label="查看安装进度">
      <span
        class="ring"
        style={`--ring-progress: ${store.installPct * 3.6}deg`}
      ></span>
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
  <EnvironmentSwitcher />
</header>

<main class="content">
  <div class="component-list">
  {#each store.components as component, i (component.name)}
      {@const webUiKey = `${store.activeEnvironmentId}:${component.name}`}
      {@const adapter = componentAdapter(component.name)}
      {@const starting = store.busy && store.busyAction === "start" && store.busyComponent === component.name}
      {@const stopping = store.busy && store.busyAction === "stop" && store.busyComponent === component.name}
      <div class="component-row" style={"--i:" + i}>
        <span class="component-logo {component.status}">
          {#if adapter}
            {@const Logo = adapter.logo}
            <Logo />
          {/if}
        </span>
        <div class="component-info">
          <div class="component-title-row">
            <span class="component-name">{component.display_name || component.name}</span>
            {#if webUis[webUiKey]?.length}
              <span class="webui-sep">|</span>
              {#each webUis[webUiKey] as wu (wu.name)}
                <button class="webui-badge {component.status}" onclick={() => openWebUi(wu.url)} title={`打开 ${wu.name} WebUI`}>
                  {wu.name}
                </button>
              {/each}
            {/if}
          </div>
          <span class="component-meta mono">
            v{component.version} · {starting ? "启动中" : stopping ? "停止中" : component.statusText}
          </span>
        </div>
        <div class="row-actions">
          {#if starting || stopping}
            <Button variant="secondary" size="sm" class="w-auto" disabled>
              <span class="log-spinner"></span>
              {starting ? "启动中…" : "停止中…"}
            </Button>
          {:else if component.status === "running" || component.status === "partial"}
            <Button
              variant="destructive"
              size="sm"
              class="w-auto"
              onclick={() => stopComponent(component.name)}
              disabled={store.busy || Boolean(store.switchingEnvironmentId)}
            >
              <Square size={14} />
              停止
            </Button>
          {:else}
            <Button
              size="sm"
              class="w-auto"
              onclick={() => startComponent(component.name)}
              disabled={store.busy || Boolean(store.switchingEnvironmentId)}
            >
              <Play size={14} />
              启动
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
