<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { goto } from "$app/navigation";
  import { page } from "$app/state";
  import { openPath } from "@tauri-apps/plugin-opener";
  import { ExternalLink, Folder, Play } from "lucide-svelte";
  import Button from "$lib/components/ui/button/button.svelte";
  import PageHeader from "$lib/PageHeader.svelte";
  import { store, basename, getSelected } from "$lib/stores.svelte.ts";
  import type { ComponentDirs, LogEntry } from "$lib/types";

  const name = $derived(page.params.name ?? "");
  const selected = $derived(getSelected());

  let logFiles = $state<LogEntry[]>([]);
  let logDir = $state("");
  let searchQuery = $state("");
  let loadedKey = $state("");

  const filteredLogs = $derived(
    logFiles.filter((f) => f.name.toLowerCase().includes(searchQuery.trim().toLowerCase()))
  );

  $effect(() => {
    store.selectedName = name;
    if (!selected || !store.activeEnvironmentId) return;
    const key = `${store.activeEnvironmentId}:${name}@${selected.version}`;
    if (loadedKey === key) return;
    loadedKey = key;
    void loadLogs(name);
    void loadLogDir(name);
  });

  async function loadLogs(component: string) {
    try {
      const paths = await invoke<string[]>("list_component_logs", {
        environmentId: store.activeEnvironmentId,
        component,
      });
      logFiles = paths.map((p) => ({ path: p, name: p.split("/").pop() ?? p }));
    } catch {
      logFiles = [];
    }
  }

  async function loadLogDir(component: string) {
    try {
      const dirs = await invoke<ComponentDirs>("get_component_dirs", {
        environmentId: store.activeEnvironmentId,
        component,
      });
      logDir = dirs.log;
    } catch {
      logDir = "";
    }
  }

  async function openInFinder() {
    if (!logDir) return;
    try {
      await openPath(logDir);
    } catch {
      store.errorMsg = `无法在访达中显示: ${logDir}`;
    }
  }

  async function openLogFile(path: string) {
    try {
      await invoke("open_log_file", {
        environmentId: store.activeEnvironmentId,
        component: name,
        path,
      });
    } catch (e) {
      store.errorMsg = String(e);
    }
  }

  async function openLiveLog(path: string) {
    const params = new URLSearchParams({
      source: "component",
      environmentId: store.activeEnvironmentId,
      component: name,
      path,
    });
    await goto(`/logs?${params.toString()}`);
  }
</script>

<PageHeader title={`${selected?.display_name || store.selectedName} · 日志`} />

<div class="logs-view">
  <div class="logs-toolbar">
    <input
      class="input mono search-input"
      type="text"
      placeholder="搜索日志文件名"
      bind:value={searchQuery}
      spellcheck="false"
    />
    <div class="logs-toolbar-spacer"></div>
    <span class="logs-count mono">{filteredLogs.length} 个日志文件</span>
    <Button variant="outline" size="sm" onclick={openInFinder} disabled={!logDir}>
      <Folder size={15} />
      在访达中显示
    </Button>
  </div>

  <div class="logs-scroll">
    {#if filteredLogs.length === 0}
      <div class="logs-empty">
        <p class="hint">
          {logFiles.length === 0
            ? "暂无日志文件。启动组件后日志会出现在 var/log/ 下。"
            : "没有匹配的日志文件。"}
        </p>
      </div>
    {:else}
      <div class="logs-card">
        {#each filteredLogs as f (f.path)}
          <div class="log-file-item">
            <span class="log-file-name mono">{f.name}</span>
            <button
              class="log-open-btn"
              onclick={() => openLiveLog(f.path)}
              title="查看日志"
            >
              <Play size={14} />
            </button>
            <button class="log-open-btn" onclick={() => openLogFile(f.path)} title="打开">
              <ExternalLink size={14} />
            </button>
          </div>
        {/each}
      </div>
    {/if}

    {#if store.errorMsg}
      <p class="msg error">{store.errorMsg}</p>
    {/if}
  </div>
</div>
