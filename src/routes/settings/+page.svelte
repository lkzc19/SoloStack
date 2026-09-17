<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { openPath } from "@tauri-apps/plugin-opener";
  import { relaunch } from "@tauri-apps/plugin-process";
  import { check, type Update } from "@tauri-apps/plugin-updater";
  import { Folder } from "lucide-svelte";
  import { onDestroy, onMount, untrack } from "svelte";
  import { Popover } from "bits-ui";
  import {
    AppWindow,
    Check,
    ChevronDown,
    Download,
    ExternalLink,
    Globe,
    Monitor,
    Moon,
    RefreshCw,
    Sun,
  } from "lucide-svelte";
  import Button from "$lib/components/ui/button/button.svelte";
  import DatePicker from "$lib/components/date-picker.svelte";
  import PageHeader from "$lib/PageHeader.svelte";
  import Select from "$lib/components/ui/select/select.svelte";
  import Switch from "$lib/components/ui/switch/switch.svelte";
  import { store, flashSuccess, fmtBytes, themeState, setThemeMode } from "$lib/stores.svelte.ts";
  import type { AppDef, DownloadPackageInfo, SettingsInfo } from "$lib/types";

  let settingsInfo = $state<SettingsInfo | null>(null);
  let settingsTab = $state<"general" | "logs" | "cache" | "advanced" | "about">("general");
  let appLogs = $state("");
  let appLogsBusy = $state(false);
  let logDates = $state<string[]>([]);
  let selectedLogDate = $state("");
  let appLogRefresh = $state(0);
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
  let openEditor = $state(false);
  let openRefresh = $state(false);
  let updateStatus = $state<
    "idle" | "checking" | "up-to-date" | "available" | "downloading" | "installing" | "error"
  >("idle");
  let updateVersion = $state("");
  let updateDate = $state("");
  let updateNotes = $state("");
  let updateDownloaded = $state(0);
  let updateTotal = $state(0);
  let pendingUpdate: Update | null = null;

  const currentEditor = $derived(apps.find((a) => a.bundle_id === logViewer) ?? null);
  const updateProgress = $derived(
    updateTotal > 0 ? Math.min(100, Math.round((updateDownloaded / updateTotal) * 100)) : 0
  );
  const updateProgressText = $derived(
    updateTotal > 0
      ? `${fmtBytes(updateDownloaded)} / ${fmtBytes(updateTotal)}`
      : fmtBytes(updateDownloaded)
  );
  const refreshItems = $derived([
    { value: "0", label: "不刷新" },
    { value: "5", label: "5 秒" },
    { value: "10", label: "10 秒" },
    { value: "30", label: "30 秒" },
    { value: "60", label: "60 秒" },
  ]);
  const refreshLabel = $derived(
    refreshItems.find((i) => Number(i.value) === appLogRefresh)?.label ?? "不刷新"
  );
  const logLevelItems = [
    { value: "error", label: "ERROR" },
    { value: "warn", label: "WARN" },
    { value: "info", label: "INFO" },
    { value: "debug", label: "DEBUG" },
    { value: "trace", label: "TRACE" },
  ];


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

  function switchSettingsTab(t: "general" | "logs" | "cache" | "advanced" | "about") {
    settingsTab = t;
    if (t === "cache") loadCache();
  }

  async function loadLogsView() {
    await loadLogDates();
    await loadAppLogs();
  }

  async function loadLogDates() {
    try {
      logDates = await invoke<string[]>("list_log_dates");
      // 默认选最新的（倒序第一个，即今天或最近一天）
      if (!logDates.includes(selectedLogDate)) {
        selectedLogDate = logDates[0] ?? "";
      }
    } catch {
      logDates = [];
    }
  }

  async function loadAppLogs() {
    appLogsBusy = true;
    try {
      appLogs = await invoke<string>("get_app_logs", {
        date: selectedLogDate || null,
      });
      autoScrollLogs();
    } catch (e) {
      appLogs = String(e);
    } finally {
      appLogsBusy = false;
    }
  }

  // 日志自动刷新：按选择的频率（0 = 不刷新）
  $effect(() => {
    if (settingsTab !== "logs") return;
    if (appLogRefresh <= 0) return;
    const timer = setInterval(loadAppLogs, appLogRefresh * 1000);
    return () => clearInterval(timer);
  });

  $effect(() => {
    if (settingsTab === "logs") {
      untrack(() => {
        void loadLogsView();
      });
    }
  });

  let logBox = $state<HTMLPreElement | undefined>(undefined);
  function autoScrollLogs() {
    // 等 DOM 更新后滚到底部
    setTimeout(() => {
      if (logBox) logBox.scrollTop = logBox.scrollHeight;
    }, 0);
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
      await openPath(`${root}/var/downloads`);
    } catch {
      store.errorMsg = "无法打开下载目录";
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
        class:active={settingsTab === "logs"}
        onclick={() => switchSettingsTab("logs")}
      >
        日志
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
    {:else if settingsTab === "logs"}
      <section class="settings-section">
        <h2 class="settings-title">日志</h2>
        <p class="settings-desc">查看 SoloStack 运行日志，选择日期查看某天记录，可设置自动刷新。</p>
        <div class="log-viewer-toolbar">
          <DatePicker
            value={selectedLogDate}
            onPick={(d) => {
              selectedLogDate = d;
              loadAppLogs();
            }}
          />
          <div class="cache-toolbar-spacer"></div>
          <Popover.Root bind:open={openRefresh}>
            <Popover.Trigger>
              <Button variant="outline" size="sm" type="button">
                <RefreshCw size={14} />
                <span>{refreshLabel}</span>
                <ChevronDown size={14} class="refresh-chev" />
              </Button>
            </Popover.Trigger>
            <Popover.Content side="bottom" align="end" sideOffset={6} class="toolbar-menu">
              {#each refreshItems as it (it.value)}
                <button
                  class="toolbar-menu-item"
                  class:active={appLogRefresh === Number(it.value)}
                  type="button"
                  onclick={() => { appLogRefresh = Number(it.value); openRefresh = false; }}
                >
                  <span>{it.label}</span>
                  {#if appLogRefresh === Number(it.value)}
                    <Check size={13} />
                  {/if}
                </button>
              {/each}
            </Popover.Content>
          </Popover.Root>
        </div>
        <pre class="log-box mono" bind:this={logBox}>{appLogs || (appLogsBusy ? "正在读取日志…" : "暂无日志")}</pre>
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
            <Button variant="outline" size="sm"><Globe size={14} />官方网站</Button>
            <Button variant="outline" size="sm">
              <svg class="about-icon" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
                <path
                  d="M8 0C3.58 0 0 3.58 0 8c0 3.54 2.29 6.53 5.47 7.59.4.07.55-.17.55-.38 0-.19-.01-.82-.01-1.49-2.01.37-2.53-.49-2.69-.94-.09-.23-.48-.94-.82-1.13-.28-.15-.68-.52-.01-.53.63-.01 1.08.58 1.23.82.72 1.21 1.87.87 2.33.66.07-.52.28-.87.51-1.07-1.78-.2-3.64-.89-3.64-3.95 0-.87.31-1.59.82-2.15-.08-.2-.36-1.02.08-2.12 0 0 .67-.21 2.2.82.64-.18 1.32-.27 2-.27.68 0 1.36.09 2 .27 1.53-1.04 2.2-.82 2.2-.82.44 1.1.16 1.92.08 2.12.51.56.82 1.27.82 2.15 0 3.07-1.87 3.75-3.65 3.95.29.25.54.73.54 1.48 0 1.07-.01 1.93-.01 2.2 0 .21.15.46.55.38A8.01 8.01 0 0 0 16 8c0-4.42-3.58-8-8-8z"
                />
              </svg>
              GitHub
            </Button>
            <Button variant="outline" size="sm"><ExternalLink size={14} />更新日志</Button>
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
