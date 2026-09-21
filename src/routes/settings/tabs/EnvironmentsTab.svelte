<script lang="ts">
  // 设置 · 环境：各环境的组件清单与磁盘占用、切换 / 重命名 / 删除。
  import { invoke } from "@tauri-apps/api/core";
  import { openPath } from "@tauri-apps/plugin-opener";
  import {
    ArrowRightLeft,
    Box,
    Check,
    ChevronDown,
    Folder,
    Pencil,
    RefreshCw,
    Trash2,
    X,
  } from "lucide-svelte";
  import { onMount } from "svelte";
  import Button from "$lib/components/ui/button/button.svelte";
  import { componentAdapter } from "$lib/component-adapters/registry";
  import {
    store,
    deleteEnvironment,
    fmtBytes,
    renameEnvironment,
    statusTextFor,
    switchEnvironment,
  } from "$lib/stores.svelte.ts";
  import { toastSuccess } from "$lib/notifications.svelte.ts";
  import type { EnvironmentOverview, EnvironmentUsage } from "$lib/types";

  let overviews = $state<EnvironmentOverview[]>([]);
  let overviewBusy = $state(false);
  let busyId = $state("");
  let editingId = $state("");
  let editingName = $state("");
  let deleteTarget = $state<{ id: string; name: string } | null>(null);
  let openId = $state<string | null>(null);

  onMount(() => {
    void loadEnvironmentOverviews();
  });

  async function loadEnvironmentOverviews() {
    overviewBusy = true;
    store.errorMsg = "";
    try {
      overviews = await invoke<EnvironmentOverview[]>("list_environment_overviews");
    } catch (error) {
      store.errorMsg = String(error);
    } finally {
      overviewBusy = false;
    }
  }

  async function saveEnvironmentName(id: string) {
    const name = editingName.trim();
    if (!name || busyId) return;
    busyId = id;
    store.errorMsg = "";
    try {
      await renameEnvironment(id, name);
      await loadEnvironmentOverviews();
      editingId = "";
      editingName = "";
      toastSuccess("环境名称已更新");
    } catch (error) {
      store.errorMsg = String(error);
    } finally {
      busyId = "";
    }
  }

  function requestRemoveEnvironment(id: string, name: string) {
    if (busyId) return;
    deleteTarget = { id, name };
  }

  async function confirmRemoveEnvironment() {
    const target = deleteTarget;
    if (!target || busyId) return;
    busyId = target.id;
    store.errorMsg = "";
    try {
      await deleteEnvironment(target.id);
      await loadEnvironmentOverviews();
      deleteTarget = null;
      toastSuccess("环境已删除");
    } catch (error) {
      store.errorMsg = String(error);
    } finally {
      busyId = "";
    }
  }

  async function changeEnvironment(id: string) {
    if (busyId) return;
    busyId = id;
    await switchEnvironment(id);
    await loadEnvironmentOverviews();
    busyId = "";
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
    openId = openId === id ? null : id;
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
</script>

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
      disabled={overviewBusy}
    >
      <RefreshCw size={14} class={overviewBusy ? "spin" : ""} />
      {overviewBusy ? "统计中…" : "刷新"}
    </Button>
  </div>

  {#if overviewBusy && overviews.length === 0}
    <p class="hint">正在统计环境占用…</p>
  {:else if overviews.length === 0}
    <p class="hint">暂无环境。</p>
  {:else}
    <div class="environment-ledger">
      {#each overviews as environment (environment.id)}
        <article class="environment-entry" class:open={openId === environment.id}>
          <div class="environment-entry-header">
            {#if editingId === environment.id}
              <div class="environment-entry-toggle-copy environment-name-editor">
                <input
                  class="input environment-name-input"
                  bind:value={editingName}
                  maxlength="40"
                  disabled={Boolean(busyId)}
                  onkeydown={(event) => {
                    if (event.key === "Enter") {
                      void saveEnvironmentName(environment.id);
                    } else if (event.key === "Escape") {
                      editingId = "";
                    }
                  }}
                />
                <button
                  class="environment-icon-btn"
                  type="button"
                  title="保存名称"
                  onclick={() => saveEnvironmentName(environment.id)}
                  disabled={Boolean(busyId)}
                >
                  <Check size={14} />
                </button>
                <button
                  class="environment-icon-btn"
                  type="button"
                  title="取消"
                  onclick={() => (editingId = "")}
                  disabled={Boolean(busyId)}
                >
                  <X size={14} />
                </button>
              </div>
            {:else}
              <button
                class="environment-entry-toggle"
                type="button"
                aria-label={openId === environment.id ? "收起环境" : "展开环境"}
                aria-expanded={openId === environment.id}
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
                  disabled={Boolean(busyId) || store.installing || store.busy}
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
                  editingId = environment.id;
                  editingName = environment.name;
                }}
                disabled={Boolean(busyId)}
              >
                <Pencil size={14} />
              </button>
              <button
                class="environment-icon-btn destructive"
                type="button"
                title={environment.active ? "不能删除当前活动环境" : "删除环境"}
                onclick={() => requestRemoveEnvironment(environment.id, environment.name)}
                disabled={environment.active || Boolean(busyId)}
              >
                <Trash2 size={14} />
              </button>
            </div>
            <span class="environment-entry-expand" aria-hidden="true">
              <ChevronDown size={16} class="environment-entry-chevron" />
            </span>
          </div>

          {#if openId === environment.id}
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
                        {#if item.status}
                          <span
                            class="environment-component-status {componentStatusClass(
                              item.status
                            )}"
                          >
                            <span class="environment-status-dot"></span>
                            {statusTextFor(item.status)}
                          </span>
                        {/if}
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

{#if deleteTarget}
  <div
    class="overlay"
    role="presentation"
    onclick={(event) =>
      event.target === event.currentTarget && !busyId && (deleteTarget = null)}
  >
    <div class="modal" role="dialog" aria-modal="true" tabindex="-1">
      <h3 class="modal-title danger">删除环境</h3>
      <p class="modal-warn">
        确定删除环境“{deleteTarget.name}”吗？环境中的组件、配置、日志和数据都会被删除，此操作不可恢复。
      </p>
      {#if store.errorMsg}
        <p class="msg error">{store.errorMsg}</p>
      {/if}
      <div class="modal-actions">
        <Button
          variant="ghost"
          size="sm"
          onclick={() => (deleteTarget = null)}
          disabled={Boolean(busyId)}
        >
          取消
        </Button>
        <Button
          variant="destructive"
          size="sm"
          onclick={confirmRemoveEnvironment}
          disabled={Boolean(busyId)}
        >
          {busyId ? "删除中…" : "确认删除"}
        </Button>
      </div>
    </div>
  </div>
{/if}
