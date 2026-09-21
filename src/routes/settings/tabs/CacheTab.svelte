<script lang="ts">
  // 设置 · 缓存：下载包列表、勾选删除、在访达中显示。
  import { invoke } from "@tauri-apps/api/core";
  import { openPath } from "@tauri-apps/plugin-opener";
  import { Folder, RefreshCw } from "lucide-svelte";
  import { onMount } from "svelte";
  import Button from "$lib/components/ui/button/button.svelte";
  import { fmtBytes, store } from "$lib/stores.svelte.ts";
  import { toastSuccess } from "$lib/notifications.svelte.ts";
  import type { DownloadPackageInfo } from "$lib/types";

  let packages = $state<DownloadPackageInfo[]>([]);
  let selected = $state<Set<string>>(new Set());
  let busy = $state(false);

  onMount(() => {
    void loadCache();
  });

  async function loadCache() {
    busy = true;
    try {
      packages = await invoke<DownloadPackageInfo[]>("list_download_packages");
    } catch (e) {
      store.errorMsg = String(e);
    } finally {
      busy = false;
    }
  }

  function toggleCache(name: string) {
    const next = new Set(selected);
    if (next.has(name)) next.delete(name);
    else next.add(name);
    selected = next;
  }

  async function deleteSelectedPackages() {
    const names = [...selected];
    if (names.length === 0 || busy) return;
    busy = true;
    try {
      await invoke("delete_download_packages", { names });
      selected = new Set();
      await loadCache();
      toastSuccess(`已删除 ${names.length} 个缓存包`);
    } catch (e) {
      store.errorMsg = String(e);
    } finally {
      busy = false;
    }
  }

  async function openCacheDir() {
    const root = store.rootDir;
    if (!root) return;
    try {
      await openPath(`${root}/cache/downloads`);
    } catch {
      store.errorMsg = "无法打开下载目录";
    }
  }
</script>

<section class="settings-section">
  <h2 class="settings-title">缓存</h2>
  <p class="settings-desc">管理下载的安装包缓存，可勾选删除以释放空间。</p>
  <div class="cache-toolbar">
    <Button
      variant="destructive"
      size="sm"
      onclick={deleteSelectedPackages}
      disabled={busy || selected.size === 0}
    >
      删除选中（{selected.size}）
    </Button>
    <div class="cache-toolbar-spacer"></div>
    <Button variant="outline" size="sm" onclick={openCacheDir}>
      <Folder size={15} />
      在访达中显示
    </Button>
    <Button variant="outline" size="sm" onclick={loadCache} disabled={busy}>
      <RefreshCw size={14} />
      {busy ? "读取中…" : "刷新"}
    </Button>
  </div>
  {#if packages.length === 0}
    <p class="hint">暂无已下载的包。</p>
  {:else}
    <div class="cache-card">
      {#each packages as pkg (pkg.name)}
        <label class="cache-row">
          <input
            type="checkbox"
            checked={selected.has(pkg.name)}
            onchange={() => toggleCache(pkg.name)}
          />
          <span class="cache-name mono">{pkg.name}</span>
          <span class="cache-size mono">{fmtBytes(pkg.size)}</span>
        </label>
      {/each}
    </div>
  {/if}
</section>
