<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";
  import { page } from "$app/state";
  import { Save, Trash2 } from "lucide-svelte";
  import PageHeader from "$lib/PageHeader.svelte";
  import Button from "$lib/components/ui/button/button.svelte";
  import Select from "$lib/components/ui/select/select.svelte";
  import { componentAdapter, validateConfigFieldIds } from "$lib/component-adapters/registry";
  import {
    store,
    getSelected,
    doUninstall,
    startComponent,
    stopComponent,
  } from "$lib/stores.svelte.ts";
  import { toastSuccess } from "$lib/notifications.svelte.ts";
  import type { ConfigFieldUpdate, JdkInfo } from "$lib/types";

  interface FieldValue {
    id: string;
    value: string;
  }

  const name = $derived(page.params.name ?? "");
  const selected = $derived(getSelected());
  const adapter = $derived(componentAdapter(name));

  // 后端只返回「字段当前值」；表单结构/文案/布局完全由前端定义
  let fields = $state<FieldValue[]>([]);
  let initialFields = $state<Record<string, string>>({});
  let loadedKey = $state("");
  let jdks = $state<JdkInfo[]>([]);
  let saving = $state(false);
  let showSaveConfirm = $state(false);
  let showUninstall = $state(false);
  let keepData = $state(true);

  const isRunning = $derived(
    selected !== null && (selected.status === "running" || selected.status === "partial")
  );

  // 本机所有 JDK（value = 目录名）
  const jdkItems = $derived(
    jdks.map((j) => ({ value: j.name, label: `${j.vendor} JDK ${j.version}` }))
  );
  const fieldMap = $derived(
    Object.fromEntries(fields.map((field) => [field.id, field.value]))
  );

  onMount(() => {
    loadJdks();
  });

  $effect(() => {
    store.selectedName = name;
    if (!selected) return;
    const key = `${store.activeEnvironmentId}:${name}@${selected.version}`;
    if (loadedKey === key) return;
    loadedKey = key;
    void loadFields(name, selected.version);
  });

  async function loadFields(component: string, version: string) {
    try {
      const loaded = await invoke<FieldValue[]>("list_config_fields", {
        environmentId: store.activeEnvironmentId,
        component,
      });
      validateConfigFieldIds(component, version, loaded.map((field) => field.id));
      fields = loaded;
      initialFields = Object.fromEntries(loaded.map((field) => [field.id, field.value]));
    } catch (e) {
      store.errorMsg = String(e);
    }
  }

  async function loadJdks() {
    try {
      jdks = await invoke<JdkInfo[]>("list_jdk_versions");
    } catch (e) {
      store.errorMsg = String(e);
    }
  }

  function updateField(id: string, value: string) {
    fields = fields.map((f) => (f.id === id ? { ...f, value } : f));
  }

  async function doSaveConfig() {
    showSaveConfirm = false;
    if (saving) return;
    saving = true;
    store.errorMsg = "";
    const wasRunning = isRunning;
    try {
      const updates: ConfigFieldUpdate[] = fields
        .filter((field) => field.id !== "jdk_version" || field.value.trim())
        .filter((field) => field.value !== initialFields[field.id])
        .map((field) => ({ id: field.id, value: field.value }));
      if (updates.length === 0) {
        toastSuccess("配置无变化");
        return;
      }
      await invoke("save_config_fields", {
        environmentId: store.activeEnvironmentId,
        component: name,
        updates,
      });
      if (wasRunning) {
        await stopComponent(name);
        await startComponent(name);
        toastSuccess("配置已保存，组件已重启");
      } else {
        toastSuccess("配置已保存");
      }
      if (selected) await loadFields(name, selected.version);
    } catch (e) {
      store.errorMsg = String(e);
    } finally {
      saving = false;
    }
  }
</script>

<PageHeader title={`${selected?.display_name || store.selectedName} · 配置`}>
  {#snippet actions()}
    <Button
      variant="destructive"
      size="sm"
      class="header-btn"
      onclick={() => (showUninstall = true)}
      disabled={Boolean(store.switchingEnvironmentId)}
    >
      <Trash2 size={14} />
      卸载组件
    </Button>
    <Button
      size="sm"
      class="header-btn"
      onclick={() => (showSaveConfirm = true)}
      disabled={fields.length === 0 || Boolean(store.switchingEnvironmentId)}
    >
      <Save size={14} />
      保存配置
    </Button>
  {/snippet}
</PageHeader>

<div class="settings-view">
  <div class="page-body">
    {#if "jdk_version" in fieldMap}
      <div class="field-item">
        <span class="field-label">JDK 版本</span>
        <Select
          class="full field-select"
          value={fieldMap.jdk_version}
          items={jdkItems}
          onSelect={(v) => updateField("jdk_version", v)}
        />
      </div>
    {/if}

    {#if adapter}
      {@const ConfigFields = adapter.configFields}
      <ConfigFields
        component={adapter.id}
        version={selected?.version ?? ""}
        values={fieldMap}
        jdks={jdks}
        onFieldChange={updateField}
      />
    {/if}

    {#if adapter && fields.length === 0}
      <p class="hint">加载字段失败或组件未就绪。</p>
    {/if}

    <p class="hint-inline">保存后若进程正在运行，需重启组件生效。</p>

    {#if store.errorMsg}
      <p class="msg error">{store.errorMsg}</p>
    {/if}
  </div>
</div>

{#if showSaveConfirm}
  <div
    class="overlay"
    role="presentation"
    onclick={(e) => !saving && e.target === e.currentTarget && (showSaveConfirm = false)}
  >
    <div class="modal" role="dialog" aria-modal="true" tabindex="-1">
      <h3 class="modal-title">保存配置</h3>
      <p class="modal-warn">
        {isRunning ? "保存后将自动重启组件以生效。" : "确定保存当前配置？"}
      </p>
      <div class="modal-actions">
        <Button variant="ghost" size="sm" onclick={() => (showSaveConfirm = false)} disabled={saving}>
          取消
        </Button>
        <Button size="sm" onclick={doSaveConfig} disabled={saving}>
          {saving ? "保存中…" : "确认保存"}
        </Button>
      </div>
    </div>
  </div>
{/if}

{#if showUninstall && selected}
  <div
    class="overlay"
    role="presentation"
    onclick={(e) => !store.busy && e.target === e.currentTarget && (showUninstall = false)}
  >
    <div class="modal" role="dialog" aria-modal="true" tabindex="-1">
      <h3 class="modal-title danger">卸载 {selected.display_name || selected.name}</h3>
      <p class="modal-warn">
        将删除组件本体、配置副本与运行日志
        <span class="mono">components/{selected.name}-{selected.version}/</span>。
      </p>
      <label class="toggle-row">
        <input type="checkbox" bind:checked={keepData} />
        <span>保留持久数据 <span class="mono">var/data/</span>（如 HDFS 存储）</span>
      </label>
      <div class="modal-actions">
        <Button variant="ghost" size="sm" onclick={() => (showUninstall = false)} disabled={store.busy}>
          取消
        </Button>
        <Button
          variant="destructive"
          size="sm"
          onclick={() => doUninstall(selected!.name, selected!.version, keepData)}
          disabled={store.busy}
        >
          {store.busy ? "卸载中…" : "确认卸载"}
        </Button>
      </div>
    </div>
  </div>
{/if}
