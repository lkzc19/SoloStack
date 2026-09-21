<script lang="ts">
  // 设置 · 通用：外观主题、窗口行为、界面入口显隐、首选编辑器。
  import { invoke } from "@tauri-apps/api/core";
  import { Check, ChevronDown, AppWindow, Bell, Monitor, Moon, ScrollText, Sun } from "lucide-svelte";
  import { onMount } from "svelte";
  import { Popover } from "bits-ui";
  import Button from "$lib/components/ui/button/button.svelte";
  import Switch from "$lib/components/ui/switch/switch.svelte";
  import { store, setThemeMode, themeState } from "$lib/stores.svelte.ts";
  import { toastSuccess } from "$lib/notifications.svelte.ts";
  import type { AppDef, SettingsInfo } from "$lib/types";

  let apps = $state<AppDef[]>([]);
  let logViewer = $state("");
  let closeToTray = $state(true);
  let showLogsButton = $state(true);
  let showNotificationsButton = $state(true);
  let openEditor = $state(false);

  const currentEditor = $derived(apps.find((a) => a.bundle_id === logViewer) ?? null);

  onMount(() => {
    void load();
  });

  async function load() {
    try {
      const settings = await invoke<SettingsInfo>("get_settings");
      logViewer = settings.log_viewer;
      closeToTray = settings.close_to_tray;
      showLogsButton = settings.show_logs_button;
      showNotificationsButton = settings.show_notifications_button;
      apps = await invoke<AppDef[]>("list_apps");
    } catch (e) {
      store.errorMsg = String(e);
    }
  }

  async function changeLogViewer(app: string) {
    logViewer = app;
    try {
      await invoke("set_log_viewer", { app });
      toastSuccess("日志查看器已更新");
    } catch (e) {
      store.errorMsg = String(e);
    }
  }

  async function changeCloseToTray(value: boolean) {
    const previous = closeToTray;
    closeToTray = value;
    try {
      await invoke("set_close_to_tray", { closeToTray: value });
      toastSuccess("窗口行为已更新");
    } catch (e) {
      closeToTray = previous;
      store.errorMsg = String(e);
    }
  }

  async function changeShowNotificationsButton(value: boolean) {
    const previous = showNotificationsButton;
    showNotificationsButton = value;
    try {
      await invoke("set_show_notifications_button", { showNotificationsButton: value });
      store.showNotificationsButton = value;
      toastSuccess("通知入口设置已更新");
    } catch (e) {
      showNotificationsButton = previous;
      store.errorMsg = String(e);
    }
  }

  async function changeShowLogsButton(value: boolean) {
    const previous = showLogsButton;
    showLogsButton = value;
    try {
      await invoke("set_show_logs_button", { showLogsButton: value });
      store.showLogsButton = value;
      toastSuccess("日志入口设置已更新");
    } catch (e) {
      showLogsButton = previous;
      store.errorMsg = String(e);
    }
  }
</script>

<section class="settings-section">
  <h2 class="settings-title">外观主题</h2>
  <p class="settings-desc">选择应用的外观主题，立即生效。</p>
  <div class="theme-tabs">
    <button
      class="settings-tab"
      class:active={themeState.mode === "system"}
      onclick={() => setThemeMode("system")}
    >
      <Monitor size={14} />
      跟随系统
    </button>
    <button
      class="settings-tab"
      class:active={themeState.mode === "light"}
      onclick={() => setThemeMode("light")}
    >
      <Sun size={14} />
      浅色
    </button>
    <button
      class="settings-tab"
      class:active={themeState.mode === "dark"}
      onclick={() => setThemeMode("dark")}
    >
      <Moon size={14} />
      深色
    </button>
  </div>
  <p class="editor-fallback">
    {#if themeState.mode === "system"}
      跟随 macOS 外观，切换系统外观时自动生效。
    {:else if themeState.mode === "dark"}
      已使用深色主题。
    {:else}
      已使用浅色主题。
    {/if}
  </p>
</section>

<section class="settings-section">
  <h2 class="settings-title">窗口行为</h2>
  <hr class="settings-divider" />
  <div class="settings-toggle-row">
    <div class="settings-toggle-content">
      <span class="settings-toggle-icon">
        <AppWindow size={17} aria-hidden="true" />
      </span>
      <div class="settings-toggle-copy">
        <div class="settings-toggle-label">关闭时最小化到托盘</div>
        <p class="settings-toggle-desc">
          勾选后点击关闭按钮会隐藏到系统托盘，取消则直接退出应用。
        </p>
      </div>
    </div>
    <Switch
      checked={closeToTray}
      onCheckedChange={changeCloseToTray}
      label="关闭时最小化到托盘"
    />
  </div>
</section>

<section class="settings-section">
  <h2 class="settings-title">界面</h2>
  <hr class="settings-divider" />
  <div class="settings-toggle-row">
    <div class="settings-toggle-content">
      <span class="settings-toggle-icon">
        <ScrollText size={17} aria-hidden="true" />
      </span>
      <div class="settings-toggle-copy">
        <div class="settings-toggle-label">显示日志入口</div>
        <p class="settings-toggle-desc">
          在主页顶部设置按钮右侧显示实时日志按钮。
        </p>
      </div>
    </div>
    <Switch
      checked={showLogsButton}
      onCheckedChange={changeShowLogsButton}
      label="显示日志入口"
    />
  </div>
  <div class="settings-toggle-row">
    <div class="settings-toggle-content">
      <span class="settings-toggle-icon">
        <Bell size={17} aria-hidden="true" />
      </span>
      <div class="settings-toggle-copy">
        <div class="settings-toggle-label">显示通知入口</div>
        <p class="settings-toggle-desc">
          在主页顶部设置按钮右侧显示通知按钮。
        </p>
      </div>
    </div>
    <Switch
      checked={showNotificationsButton}
      onCheckedChange={changeShowNotificationsButton}
      label="显示通知入口"
    />
  </div>
</section>

<section class="settings-section">
  <h2 class="settings-title">首选编辑器</h2>
  <p class="settings-desc">选择点击查看文件时使用的编辑器应用。</p>
  <Popover.Root bind:open={openEditor}>
    <Popover.Trigger>
      <Button variant="outline" size="sm" type="button" class="editor-trigger">
        {#if currentEditor}
          <img class="editor-icon" src={`/icons/${currentEditor.icon}.png`} alt="" />
          <span class="editor-label">{currentEditor.name}</span>
        {:else}
          <span class="editor-label">选择应用</span>
        {/if}
        <ChevronDown size={14} class="editor-chev" />
      </Button>
    </Popover.Trigger>
    <Popover.Content side="bottom" align="end" sideOffset={6} class="toolbar-menu editor-menu">
      {#each apps as app (app.bundle_id)}
        <button
          class="toolbar-menu-item"
          class:active={app.bundle_id === logViewer}
          type="button"
          onclick={() => { changeLogViewer(app.bundle_id); openEditor = false; }}
        >
          <img class="editor-icon" src={`/icons/${app.icon}.png`} alt="" />
          <span class="editor-label">{app.name}</span>
          {#if app.bundle_id === logViewer}
            <Check size={13} />
          {/if}
        </button>
      {/each}
      {#if apps.length === 0}
        <div class="toolbar-menu-item">暂无可用应用</div>
      {/if}
    </Popover.Content>
  </Popover.Root>
  <p class="editor-fallback">如果选择的编辑器不可用，将自动使用系统默认编辑器。</p>
</section>
