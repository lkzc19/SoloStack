<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { openPath, openUrl } from "@tauri-apps/plugin-opener";
  import { relaunch } from "@tauri-apps/plugin-process";
  import { check, type Update } from "@tauri-apps/plugin-updater";
  import { Folder } from "lucide-svelte";
  import { onDestroy, onMount } from "svelte";
  import { Popover } from "bits-ui";
  import {
    Activity,
    AppWindow,
    ArrowRightLeft,
    Box,
    Check,
    ChevronDown,
    Download,
    Database,
    ExternalLink,
    HardDrive,
    Monitor,
    Moon,
    Pencil,
    RefreshCw,
    ScrollText,
    Sun,
    Trash2,
    X,
  } from "lucide-svelte";
  import Button from "$lib/components/ui/button/button.svelte";
  import PageHeader from "$lib/PageHeader.svelte";
  import Select from "$lib/components/ui/select/select.svelte";
  import Switch from "$lib/components/ui/switch/switch.svelte";
  import { componentAdapter } from "$lib/component-adapters/registry";
  import {
    store,
    deleteEnvironment,
    flashSuccess,
    fmtBytes,
    renameEnvironment,
    setThemeMode,
    switchEnvironment,
    themeState,
  } from "$lib/stores.svelte.ts";
  import type {
    AppDef,
    DownloadPackageInfo,
    EnvironmentOverview,
    EnvironmentUsage,
    SettingsInfo,
  } from "$lib/types";

  let settingsInfo = $state<SettingsInfo | null>(null);
  let settingsTab = $state<
    "general" | "cache" | "environments" | "advanced" | "about"
  >("general");
  let loggingLevel = $state("info");
  let logRetentionDays = $state(7);
  let logMaxTotalMb = $state(1024);
  let openAdvancedSection = $state<string | null>(null);
  let cachePackages = $state<DownloadPackageInfo[]>([]);
  let cacheSelected = $state<Set<string>>(new Set());
  let cacheBusy = $state(false);
  let apps = $state<AppDef[]>([]);
  let logViewer = $state("");
  let closeToTray = $state(true);
  let showLogsButton = $state(true);
  let openEditor = $state(false);
  let updateStatus = $state<
    "idle" | "checking" | "up-to-date" | "available" | "downloading" | "installing" | "error"
  >("idle");
  let updateVersion = $state("");
  let updateDate = $state("");
  let updateNotes = $state("");
  let updateDownloaded = $state(0);
  let updateTotal = $state(0);
  let pendingUpdate: Update | null = null;
  let environmentBusyId = $state("");
  let editingEnvironmentId = $state("");
  let editingEnvironmentName = $state("");
  let deleteEnvironmentTarget = $state<{ id: string; name: string } | null>(null);
  let environmentOverviews = $state<EnvironmentOverview[]>([]);
  let environmentOverviewBusy = $state(false);
  let openEnvironmentId = $state<string | null>(null);

  const currentEditor = $derived(apps.find((a) => a.bundle_id === logViewer) ?? null);
  const updateProgress = $derived(
    updateTotal > 0 ? Math.min(100, Math.round((updateDownloaded / updateTotal) * 100)) : 0
  );
  const updateProgressText = $derived(
    updateTotal > 0
      ? `${fmtBytes(updateDownloaded)} / ${fmtBytes(updateTotal)}`
      : fmtBytes(updateDownloaded)
  );
  const logLevelItems = [
    { value: "error", label: "ERROR" },
    { value: "warn", label: "WARN" },
    { value: "info", label: "INFO" },
    { value: "debug", label: "DEBUG" },
    { value: "trace", label: "TRACE" },
  ];
  const GITHUB_URL = "https://github.com/lkzc19/SoloStack";
  const CHANGELOG_URL = `${GITHUB_URL}/releases`;

  onMount(() => {
    loadSettings();
  });

  onDestroy(() => {
    void pendingUpdate?.close();
  });

  async function loadSettings() {
    try {
      settingsInfo = await invoke<SettingsInfo>("get_settings");
      logViewer = settingsInfo.log_viewer;
      closeToTray = settingsInfo.close_to_tray;
      showLogsButton = settingsInfo.show_logs_button;
      loggingLevel = settingsInfo.log_level;
      logRetentionDays = settingsInfo.log_retention_days;
      logMaxTotalMb = settingsInfo.log_max_total_mb;
      await loadApps();
    } catch (e) {
      store.errorMsg = String(e);
    }
  }

  async function loadApps() {
    try {
      apps = await invoke<AppDef[]>("list_apps");
    } catch (e) {
      store.errorMsg = String(e);
    }
  }

  async function changeLogViewer(app: string) {
    logViewer = app;
    try {
      await invoke("set_log_viewer", { app });
      flashSuccess("日志查看器已更新");
    } catch (e) {
      store.errorMsg = String(e);
    }
  }

  async function changeCloseToTray(value: boolean) {
    const previous = closeToTray;
    closeToTray = value;
    try {
      await invoke("set_close_to_tray", { closeToTray: value });
      flashSuccess("窗口行为已更新");
    } catch (e) {
      closeToTray = previous;
      store.errorMsg = String(e);
    }
  }

  async function changeShowLogsButton(value: boolean) {
    const previous = showLogsButton;
    showLogsButton = value;
    try {
      await invoke("set_show_logs_button", { showLogsButton: value });
      store.showLogsButton = value;
      flashSuccess("日志入口设置已更新");
    } catch (e) {
      showLogsButton = previous;
      store.errorMsg = String(e);
    }
  }

  async function saveEnvironmentName(id: string) {
    const name = editingEnvironmentName.trim();
    if (!name || environmentBusyId) return;
    environmentBusyId = id;
    store.errorMsg = "";
    try {
      await renameEnvironment(id, name);
      await loadEnvironmentOverviews();
      editingEnvironmentId = "";
      editingEnvironmentName = "";
      flashSuccess("环境名称已更新");
    } catch (error) {
      store.errorMsg = String(error);
    } finally {
      environmentBusyId = "";
    }
  }

  function requestRemoveEnvironment(id: string, name: string) {
    if (environmentBusyId) return;
    deleteEnvironmentTarget = { id, name };
  }

  async function confirmRemoveEnvironment() {
    const target = deleteEnvironmentTarget;
    if (!target || environmentBusyId) return;
    environmentBusyId = target.id;
    store.errorMsg = "";
    try {
      await deleteEnvironment(target.id);
      await loadEnvironmentOverviews();
      deleteEnvironmentTarget = null;
      flashSuccess("环境已删除");
    } catch (error) {
      store.errorMsg = String(error);
    } finally {
      environmentBusyId = "";
    }
  }

  async function changeEnvironment(id: string) {
    if (environmentBusyId) return;
    environmentBusyId = id;
    await switchEnvironment(id);
    await loadEnvironmentOverviews();
    environmentBusyId = "";
  }

  async function loadEnvironmentOverviews() {
    environmentOverviewBusy = true;
    store.errorMsg = "";
    try {
      environmentOverviews = await invoke<EnvironmentOverview[]>(
        "list_environment_overviews"
      );
    } catch (error) {
      store.errorMsg = String(error);
    } finally {
      environmentOverviewBusy = false;
    }
  }

  async function openEnvironmentDirectory(path: string) {
    try {
      await openPath(path);
    } catch (error) {
      store.errorMsg = `无法打开环境目录: ${error}`;
    }
  }

  function storageItems(usage: EnvironmentUsage) {
    const items = [
      { key: "components", label: "组件本体", bytes: usage.components_bytes },
      { key: "data", label: "数据", bytes: usage.data_bytes },
      { key: "logs", label: "日志", bytes: usage.log_bytes },
      {
        key: "other",
        label: "其他",
        bytes: usage.runtime_bytes + usage.other_bytes,
      },
    ];
    const total = Math.max(usage.total_bytes, 1);
    return items.map((item) => ({ ...item, percent: (item.bytes / total) * 100 }));
  }

  function toggleEnvironment(id: string) {
    openEnvironmentId = openEnvironmentId === id ? null : id;
  }

  function componentStatusText(status: string) {
    if (status === "running") return "运行中";
    if (status === "partial") return "部分运行";
    if (status === "stopped") return "已停止";
    return "状态异常";
  }

  function componentStatusClass(status: string) {
    if (status.startsWith("error")) return "error";
    return status;
  }

  function formatEnvironmentTime(value: string | null) {
    if (!value) return "尚未激活";
    const date = new Date(value);
    return Number.isNaN(date.getTime()) ? value : date.toLocaleString();
  }

  function switchSettingsTab(
    t: "general" | "cache" | "environments" | "advanced" | "about"
  ) {
    settingsTab = t;
    if (t === "cache") loadCache();
    if (t === "environments") {
      openEnvironmentId = null;
      void loadEnvironmentOverviews();
    }
  }

  async function loadCache() {
    cacheBusy = true;
    try {
      cachePackages = await invoke<DownloadPackageInfo[]>("list_download_packages");
    } catch (e) {
      store.errorMsg = String(e);
    } finally {
      cacheBusy = false;
    }
  }

  function toggleCache(name: string) {
    const next = new Set(cacheSelected);
    if (next.has(name)) next.delete(name);
    else next.add(name);
    cacheSelected = next;
  }

  async function deleteSelectedPackages() {
    const names = [...cacheSelected];
    if (names.length === 0 || cacheBusy) return;
    cacheBusy = true;
    try {
      await invoke("delete_download_packages", { names });
      cacheSelected = new Set();
      await loadCache();
      flashSuccess(`已删除 ${names.length} 个缓存包`);
    } catch (e) {
      store.errorMsg = String(e);
    } finally {
      cacheBusy = false;
    }
  }

  async function openCacheDir() {
    const root = settingsInfo?.data_root ?? store.rootDir;
    if (!root) return;
    try {
      await openPath(`${root}/cache/downloads`);
    } catch {
      store.errorMsg = "无法打开下载目录";
    }
  }

  async function openExternal(url: string) {
    try {
      await openUrl(url);
    } catch (e) {
      store.errorMsg = String(e);
    }
  }

  async function saveLoggingSettings() {
    try {
      await invoke("set_logging_settings", {
        logLevel: loggingLevel,
        logRetentionDays,
        logMaxTotalMb,
      });
      flashSuccess("日志设置已更新");
    } catch (e) {
      store.errorMsg = String(e);
    }
  }

  function toggleAdvancedSection(section: string) {
    openAdvancedSection = openAdvancedSection === section ? null : section;
  }

  async function checkForUpdate() {
    if (
      updateStatus === "checking" ||
      updateStatus === "downloading" ||
      updateStatus === "installing"
    ) {
      return;
    }

    if (pendingUpdate) {
      await pendingUpdate.close().catch(() => {});
      pendingUpdate = null;
    }

    updateStatus = "checking";
    updateVersion = "";
    updateDate = "";
    updateNotes = "";
    updateDownloaded = 0;
    updateTotal = 0;
    store.errorMsg = "";

    try {
      const update = await check({ timeout: 30_000 });
      if (!update) {
        updateStatus = "up-to-date";
        return;
      }

      pendingUpdate = update;
      updateVersion = update.version;
      updateDate = update.date ?? "";
      updateNotes = update.body ?? "";
      updateStatus = "available";
    } catch (e) {
      updateStatus = "error";
      store.errorMsg = `检查更新失败: ${e}`;
    }
  }

  function formatUpdateDate(value: string) {
    const date = new Date(value);
    return Number.isNaN(date.getTime()) ? value : date.toLocaleString();
  }

  async function installUpdate() {
    if (!pendingUpdate || updateStatus === "downloading" || updateStatus === "installing") {
      return;
    }

    updateStatus = "downloading";
    updateDownloaded = 0;
    updateTotal = 0;
    store.errorMsg = "";

    try {
      await pendingUpdate.downloadAndInstall((event) => {
        if (event.event === "Started") {
          updateTotal = event.data.contentLength ?? 0;
        } else if (event.event === "Progress") {
          updateDownloaded += event.data.chunkLength;
        } else if (event.event === "Finished") {
          updateStatus = "installing";
        }
      });
      updateStatus = "installing";
      await relaunch();
    } catch (e) {
      updateStatus = "error";
      store.errorMsg = `安装更新失败: ${e}`;
    }
  }
</script>

<PageHeader title="设置" />

<div class="settings-view">
  <div class="settings-sticky">
    <div class="settings-tabs">
      <button
        class="settings-tab"
        type="button"
        class:active={settingsTab === "general"}
        onclick={() => switchSettingsTab("general")}
      >
        通用
      </button>
      <button
        class="settings-tab"
        type="button"
        class:active={settingsTab === "environments"}
        onclick={() => switchSettingsTab("environments")}
      >
        环境
      </button>
      <button
        class="settings-tab"
        type="button"
        class:active={settingsTab === "cache"}
        onclick={() => switchSettingsTab("cache")}
      >
        缓存
      </button>
      <button
        class="settings-tab"
        type="button"
        class:active={settingsTab === "advanced"}
        onclick={() => switchSettingsTab("advanced")}
      >
        高级
      </button>
      <button
        class="settings-tab"
        type="button"
        class:active={settingsTab === "about"}
        onclick={() => switchSettingsTab("about")}
      >
        关于
      </button>
    </div>
  </div>

  <div class="page-body">
    {#if settingsTab === "general"}
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
    {:else if settingsTab === "cache"}
      <section class="settings-section">
        <h2 class="settings-title">缓存</h2>
        <p class="settings-desc">管理下载的安装包缓存，可勾选删除以释放空间。</p>
        <div class="cache-toolbar">
          <Button
            variant="destructive"
            size="sm"
            onclick={deleteSelectedPackages}
            disabled={cacheBusy || cacheSelected.size === 0}
          >
            删除选中（{cacheSelected.size}）
          </Button>
          <div class="cache-toolbar-spacer"></div>
          <Button variant="outline" size="sm" onclick={openCacheDir}>
            <Folder size={15} />
            在访达中显示
          </Button>
          <Button variant="outline" size="sm" onclick={loadCache} disabled={cacheBusy}>
            <RefreshCw size={14} />
            {cacheBusy ? "读取中…" : "刷新"}
          </Button>
        </div>
        {#if cachePackages.length === 0}
          <p class="hint">暂无已下载的包。</p>
        {:else}
          <div class="cache-card">
            {#each cachePackages as pkg (pkg.name)}
              <label class="cache-row">
                <input
                  type="checkbox"
                  checked={cacheSelected.has(pkg.name)}
                  onchange={() => toggleCache(pkg.name)}
                />
                <span class="cache-name mono">{pkg.name}</span>
                <span class="cache-size mono">{fmtBytes(pkg.size)}</span>
              </label>
            {/each}
          </div>
        {/if}
      </section>
    {:else if settingsTab === "environments"}
      <section class="settings-section">
        <div class="environment-page-head">
          <div>
            <h2 class="settings-title">环境</h2>
            <p class="settings-desc">
              每个环境独立保存组件、数据、日志和运行记录。共享安装包缓存不计入环境占用。
            </p>
          </div>
          <Button
            variant="outline"
            size="sm"
            onclick={loadEnvironmentOverviews}
            disabled={environmentOverviewBusy}
          >
            <RefreshCw size={14} class={environmentOverviewBusy ? "spin" : ""} />
            {environmentOverviewBusy ? "统计中…" : "刷新"}
          </Button>
        </div>

        {#if environmentOverviewBusy && environmentOverviews.length === 0}
          <p class="hint">正在统计环境占用…</p>
        {:else if environmentOverviews.length === 0}
          <p class="hint">暂无环境。</p>
        {:else}
          <div class="environment-ledger">
            {#each environmentOverviews as environment (environment.id)}
              <article
                class="environment-entry"
                class:open={openEnvironmentId === environment.id}
              >
                <div class="environment-entry-header">
                  {#if editingEnvironmentId === environment.id}
                    <div class="environment-entry-toggle-copy environment-name-editor">
                      <input
                        class="input environment-name-input"
                        bind:value={editingEnvironmentName}
                        maxlength="40"
                        disabled={Boolean(environmentBusyId)}
                        onkeydown={(event) => {
                          if (event.key === "Enter") {
                            void saveEnvironmentName(environment.id);
                          } else if (event.key === "Escape") {
                            editingEnvironmentId = "";
                          }
                        }}
                      />
                      <button
                        class="environment-icon-btn"
                        type="button"
                        title="保存名称"
                        onclick={() => saveEnvironmentName(environment.id)}
                        disabled={Boolean(environmentBusyId)}
                      >
                        <Check size={14} />
                      </button>
                      <button
                        class="environment-icon-btn"
                        type="button"
                        title="取消"
                        onclick={() => (editingEnvironmentId = "")}
                        disabled={Boolean(environmentBusyId)}
                      >
                        <X size={14} />
                      </button>
                    </div>
                  {:else}
                    <button
                      class="environment-entry-toggle"
                      type="button"
                      aria-label={openEnvironmentId === environment.id ? "收起环境" : "展开环境"}
                      aria-expanded={openEnvironmentId === environment.id}
                      onclick={() => toggleEnvironment(environment.id)}
                    ></button>
                    <div class="environment-entry-toggle-copy">
                      <span class="environment-name-row">
                        <span class="environment-name">{environment.name}</span>
                        {#if environment.active}
                          <span class="environment-active-badge">当前</span>
                        {/if}
                      </span>
                      <span class="environment-summary">
                        {environment.components.length} 个组件 · 总占用 {fmtBytes(
                          environment.usage.total_bytes
                        )}
                      </span>
                    </div>
                  {/if}

                  <div class="environment-actions">
                    {#if !environment.active}
                      <button
                        class="environment-icon-btn"
                        type="button"
                        title="切换到该环境"
                        onclick={() => changeEnvironment(environment.id)}
                        disabled={Boolean(environmentBusyId) || store.installing || store.busy}
                      >
                        <ArrowRightLeft size={14} />
                      </button>
                    {/if}
                    <button
                      class="environment-icon-btn"
                      type="button"
                      title="在访达中显示"
                      onclick={() => openEnvironmentDirectory(environment.path)}
                    >
                      <Folder size={14} />
                    </button>
                    <button
                      class="environment-icon-btn"
                      type="button"
                      title="重命名"
                      onclick={() => {
                        editingEnvironmentId = environment.id;
                        editingEnvironmentName = environment.name;
                      }}
                      disabled={Boolean(environmentBusyId)}
                    >
                      <Pencil size={14} />
                    </button>
                    <button
                      class="environment-icon-btn destructive"
                      type="button"
                      title={environment.active ? "不能删除当前活动环境" : "删除环境"}
                      onclick={() => requestRemoveEnvironment(environment.id, environment.name)}
                      disabled={environment.active || Boolean(environmentBusyId)}
                    >
                      <Trash2 size={14} />
                    </button>
                  </div>
                  <span class="environment-entry-expand" aria-hidden="true">
                    <ChevronDown size={16} class="environment-entry-chevron" />
                  </span>
                </div>

                {#if openEnvironmentId === environment.id}
                  <div class="environment-entry-body">
                    <section class="environment-inventory">
                      <div class="environment-column-head">
                        <span>组件清单</span>
                        <span class="environment-count-badge">
                          {environment.components.length}
                        </span>
                      </div>
                      {#if environment.components.length === 0}
                        <div class="environment-empty">尚未安装组件</div>
                      {:else}
                        <div class="environment-component-list">
                          {#each environment.components as item (item.component)}
                            {@const adapter = componentAdapter(item.component)}
                            <div
                              class="environment-component-row"
                              title={`安装于 ${formatEnvironmentTime(item.installed_at)}`}
                            >
                              <span class="environment-component-logo">
                                {#if adapter}
                                  {@const Logo = adapter.logo}
                                  <Logo />
                                {:else}
                                  <Box size={18} />
                                {/if}
                              </span>
                              <div class="environment-component-copy">
                                <span class="environment-component-name">
                                  {item.display_name || item.component}
                                </span>
                                <span class="environment-component-version mono">
                                  v{item.version}
                                </span>
                              </div>
                              <span
                                class="environment-component-status {componentStatusClass(
                                  item.status
                                )}"
                              >
                                <span class="environment-status-dot"></span>
                                {componentStatusText(item.status)}
                              </span>
                            </div>
                          {/each}
                        </div>
                      {/if}
                    </section>

                    <section class="environment-storage">
                      <div class="environment-column-head">
                        <span>存储明细</span>
                        <strong>{fmtBytes(environment.usage.total_bytes)}</strong>
                      </div>
                      <div class="environment-storage-grid">
                        {#each storageItems(environment.usage) as item (item.key)}
                          <div class="environment-storage-item">
                            <span>{item.label}</span>
                            <strong class="mono">{fmtBytes(item.bytes)}</strong>
                            <span class="mono">{item.percent.toFixed(0)}%</span>
                          </div>
                        {/each}
                      </div>
                    </section>
                  </div>
                {/if}
              </article>
            {/each}
          </div>
        {/if}
      </section>
    {:else if settingsTab === "advanced"}
      <section class="settings-section">
        <div class="settings-accordion-list">
          <div
            class="settings-accordion"
            class:open={openAdvancedSection === "logging"}
          >
            <button
              class="settings-accordion-header"
              type="button"
              aria-expanded={openAdvancedSection === "logging"}
              onclick={() => toggleAdvancedSection("logging")}
            >
              <span class="settings-accordion-copy">
                <span class="settings-field-title">应用诊断日志</span>
                <span class="settings-field-desc">设置日志记录级别、保留期限和容量限制。</span>
              </span>
              <ChevronDown size={16} class="settings-accordion-chevron" />
            </button>
            {#if openAdvancedSection === "logging"}
              <div class="settings-accordion-content">
                <div class="settings-accordion-field-row">
                  <div class="settings-field-copy">
                    <div class="settings-accordion-label">日志级别</div>
                    <p class="settings-field-desc">设置输出的最低日志级别</p>
                  </div>
                  <Select
                    value={loggingLevel}
                    items={logLevelItems}
                    class="settings-log-level-select"
                    size="sm"
                    onSelect={(value) => {
                      loggingLevel = value;
                      saveLoggingSettings();
                    }}
                  />
                </div>
                <div class="log-level-help">
                  <div class="log-level-help-title">日志级别说明：</div>
                  <div class="log-level-help-list">
                    <div class="log-level-help-item">
                      <code class="log-level-error">ERROR</code>
                      <span>仅记录操作失败、脚本失败等严重错误</span>
                    </div>
                    <div class="log-level-help-item">
                      <code class="log-level-warn">WARN</code>
                      <span>记录错误、脚本 stderr、取消操作和可恢复异常</span>
                    </div>
                    <div class="log-level-help-item">
                      <code class="log-level-info">INFO</code>
                      <span>记录安装、启停、卸载等一般操作信息（发布版默认）</span>
                    </div>
                    <div class="log-level-help-item">
                      <code class="log-level-debug">DEBUG</code>
                      <span>记录脚本命令、stdout 和执行细节（开发版默认）</span>
                    </div>
                    <div class="log-level-help-item">
                      <code class="log-level-trace">TRACE</code>
                      <span>预留的最详细级别，目前没有业务代码主动写入</span>
                    </div>
                  </div>
                </div>
                <div class="settings-accordion-inline-fields">
                  <label class="settings-inline-field">
                    <span class="settings-inline-label">保留天数</span>
                    <input
                      class="settings-inline-input"
                      type="number"
                      min="1"
                      max="365"
                      bind:value={logRetentionDays}
                      onchange={saveLoggingSettings}
                    />
                  </label>
                  <label class="settings-inline-field">
                    <span class="settings-inline-label">总容量上限</span>
                    <span class="settings-inline-number">
                      <input
                        type="number"
                        min="10"
                        max="10240"
                        bind:value={logMaxTotalMb}
                        onchange={saveLoggingSettings}
                      />
                      <span>MB</span>
                    </span>
                  </label>
                </div>
              </div>
            {/if}
          </div>
        </div>
      </section>
    {:else}
      <section class="about-section">
        <h2 class="settings-title">关于</h2>
        <p class="settings-desc">查看版本信息与更新状态。</p>
        <div class="about-card">
          <div class="about-info">
            <div class="about-brand">
              <img class="about-logo" src="/icons/solo-logo.png" alt="SoloStack" />
              <div class="about-name">SoloStack</div>
            </div>
            <span class="version-badge mono">
              {store.appVersion ? `版本 v${store.appVersion}` : "版本未知"}
            </span>
          </div>
          <div class="about-actions">
            <Button
              variant="outline"
              size="sm"
              onclick={() => openExternal(GITHUB_URL)}
            >
              <svg class="about-icon" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
                <path
                  d="M8 0C3.58 0 0 3.58 0 8c0 3.54 2.29 6.53 5.47 7.59.4.07.55-.17.55-.38 0-.19-.01-.82-.01-1.49-2.01.37-2.53-.49-2.69-.94-.09-.23-.48-.94-.82-1.13-.28-.15-.68-.52-.01-.53.63-.01 1.08.58 1.23.82.72 1.21 1.87.87 2.33.66.07-.52.28-.87.51-1.07-1.78-.2-3.64-.89-3.64-3.95 0-.87.31-1.59.82-2.15-.08-.2-.36-1.02.08-2.12 0 0 .67-.21 2.2.82.64-.18 1.32-.27 2-.27.68 0 1.36.09 2 .27 1.53-1.04 2.2-.82 2.2-.82.44 1.1.16 1.92.08 2.12.51.56.82 1.27.82 2.15 0 3.07-1.87 3.75-3.65 3.95.29.25.54.73.54 1.48 0 1.07-.01 1.93-.01 2.2 0 .21.15.46.55.38A8.01 8.01 0 0 0 16 8c0-4.42-3.58-8-8-8z"
                />
              </svg>
              GitHub
            </Button>
            <Button
              variant="outline"
              size="sm"
              onclick={() => openExternal(CHANGELOG_URL)}
            >
              <ExternalLink size={14} />更新日志
            </Button>
            <Button
              variant="outline"
              size="sm"
              onclick={checkForUpdate}
              disabled={updateStatus === "checking" ||
                updateStatus === "downloading" ||
                updateStatus === "installing"}
            >
              <RefreshCw size={14} class={updateStatus === "checking" ? "spin" : ""} />
              {updateStatus === "checking" ? "检查中…" : "检查更新"}
            </Button>
          </div>
        </div>
        {#if updateStatus !== "idle"}
          <div class="update-panel">
            {#if updateStatus === "checking"}
              <div class="update-heading">
                <RefreshCw size={15} class="spin" />
                <span>正在检查更新…</span>
              </div>
            {:else if updateStatus === "up-to-date"}
              <div class="update-heading">
                <Check size={15} />
                <span>已是最新版本</span>
              </div>
            {:else if updateStatus === "available"}
              <div class="update-heading">
                <Download size={15} />
                <span>发现新版本 v{updateVersion}</span>
              </div>
              {#if updateDate}
                <p class="update-meta">发布于 {formatUpdateDate(updateDate)}</p>
              {/if}
              {#if updateNotes}
                <p class="update-notes">{updateNotes}</p>
              {/if}
              <div class="update-actions">
                <Button size="sm" onclick={installUpdate}>
                  <Download size={14} />
                  下载并安装
                </Button>
              </div>
            {:else if updateStatus === "downloading"}
              <div class="update-heading">
                <Download size={15} />
                <span>正在下载 v{updateVersion}…</span>
              </div>
              <div class="update-progress-track">
                <div class="update-progress-bar" style={`width: ${updateProgress}%`}></div>
              </div>
              <p class="update-meta">{updateProgressText}</p>
            {:else if updateStatus === "installing"}
              <div class="update-heading">
                <RefreshCw size={15} class="spin" />
                <span>正在安装更新，应用即将重启…</span>
              </div>
            {:else}
              <div class="update-heading">更新失败</div>
              {#if updateVersion}
                <div class="update-actions">
                  <Button size="sm" variant="outline" onclick={installUpdate}>
                    <RefreshCw size={14} />
                    重试更新
                  </Button>
                </div>
              {/if}
            {/if}
          </div>
        {/if}
      </section>
    {/if}
    {#if store.errorMsg}
      <p class="msg error">{store.errorMsg}</p>
    {/if}
  </div>
</div>

{#if deleteEnvironmentTarget}
  <div
    class="overlay"
    role="presentation"
    onclick={(event) =>
      event.target === event.currentTarget &&
      !environmentBusyId &&
      (deleteEnvironmentTarget = null)}
  >
    <div class="modal" role="dialog" aria-modal="true" tabindex="-1">
      <h3 class="modal-title danger">删除环境</h3>
      <p class="modal-warn">
        确定删除环境“{deleteEnvironmentTarget.name}”吗？环境中的组件、配置、日志和数据都会被删除，此操作不可恢复。
      </p>
      {#if store.errorMsg}
        <p class="msg error">{store.errorMsg}</p>
      {/if}
      <div class="modal-actions">
        <Button
          variant="ghost"
          size="sm"
          onclick={() => (deleteEnvironmentTarget = null)}
          disabled={Boolean(environmentBusyId)}
        >
          取消
        </Button>
        <Button
          variant="destructive"
          size="sm"
          onclick={confirmRemoveEnvironment}
          disabled={Boolean(environmentBusyId)}
        >
          {environmentBusyId ? "删除中…" : "确认删除"}
        </Button>
      </div>
    </div>
  </div>
{/if}
