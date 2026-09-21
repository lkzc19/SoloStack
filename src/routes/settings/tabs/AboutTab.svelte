<script lang="ts">
  // 设置 · 关于：版本信息、GitHub / 更新日志链接、检查更新与安装。
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { relaunch } from "@tauri-apps/plugin-process";
  import { check, type Update } from "@tauri-apps/plugin-updater";
  import { Check, Download, ExternalLink, RefreshCw } from "lucide-svelte";
  import { onDestroy } from "svelte";
  import Button from "$lib/components/ui/button/button.svelte";
  import { fmtBytes, store } from "$lib/stores.svelte.ts";

  let updateStatus = $state<
    "idle" | "checking" | "up-to-date" | "available" | "downloading" | "installing" | "error"
  >("idle");
  let updateVersion = $state("");
  let updateDate = $state("");
  let updateNotes = $state("");
  let updateDownloaded = $state(0);
  let updateTotal = $state(0);
  let pendingUpdate: Update | null = null;

  const updateProgress = $derived(
    updateTotal > 0 ? Math.min(100, Math.round((updateDownloaded / updateTotal) * 100)) : 0
  );
  const updateProgressText = $derived(
    updateTotal > 0
      ? `${fmtBytes(updateDownloaded)} / ${fmtBytes(updateTotal)}`
      : fmtBytes(updateDownloaded)
  );
  const GITHUB_URL = "https://github.com/lkzc19/SoloStack";
  const CHANGELOG_URL = `${GITHUB_URL}/releases`;

  onDestroy(() => {
    void pendingUpdate?.close();
  });

  async function openExternal(url: string) {
    try {
      await openUrl(url);
    } catch (e) {
      store.errorMsg = String(e);
    }
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
