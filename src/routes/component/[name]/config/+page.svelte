<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";
  import { page } from "$app/state";
  import PageHeader from "$lib/PageHeader.svelte";
  import Button from "$lib/components/ui/button/button.svelte";
  import Select from "$lib/components/ui/select/select.svelte";
  import Switch from "$lib/components/ui/switch/switch.svelte";
  import {
    store,
    flashSuccess,
    getSelected,
    doUninstall,
    startComponent,
    stopComponent,
  } from "$lib/stores.svelte.ts";
  import type { JdkInfo } from "$lib/types";

  interface ConfigField {
    id: string;
    label: string;
    value_type: string; // "port" | "version"
    value: string;
  }

  const name = $derived(page.params.name ?? "");
  const selected = $derived(getSelected());

  let fields = $state<ConfigField[]>([]);
  let jdks = $state<JdkInfo[]>([]);
  let saving = $state(false);
  let showSaveConfirm = $state(false);
  let showUninstall = $state(false);
  let keepData = $state(true);

  const isRunning = $derived(
    selected !== null && (selected.status === "running" || selected.status === "partial")
  );

  // 列出本机所有 JDK 发行版；value 用目录名（唯一，避免 key 重复）
  const jdkItems = $derived(
    jdks.map((j) => ({
      value: j.name,
      label: `${j.vendor} JDK ${j.version}`,
    }))
  );

  onMount(() => {
    store.selectedName = name;
    loadFields(name);
    loadJdks();
  });

  async function loadFields(component: string) {
    try {
      fields = await invoke<ConfigField[]>("list_config_fields", { component });
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

  function fieldValue(id: string) {
    return fields.find((f) => f.id === id)?.value ?? "";
  }
  function fieldLabel(id: string) {
    return fields.find((f) => f.id === id)?.label ?? "";
  }

  async function doSaveConfig() {
    showSaveConfirm = false;
    if (saving) return;
    saving = true;
    store.errorMsg = "";
    const wasRunning = isRunning;
    try {
      for (const f of fields) {
        await invoke("set_config_field", {
          component: name,
          fieldId: f.id,
          value: f.value,
        });
      }
      // 组件在运行时，保存后自动重启
      if (wasRunning) {
        await stopComponent(name);
        await startComponent(name);
        flashSuccess("配置已保存，组件已重启");
      } else {
        flashSuccess("配置已保存");
      }
      await loadFields(name);
    } catch (e) {
      store.errorMsg = String(e);
    } finally {
      saving = false;
    }
  }
</script>

<PageHeader title={`${selected?.display_name || store.selectedName} · 配置`}>
  {#snippet actions()}
    <Button variant="destructive" size="sm" onclick={() => (showUninstall = true)}>
      卸载组件
    </Button>
    <Button size="sm" onclick={() => (showSaveConfirm = true)} disabled={fields.length === 0}>
      保存配置
    </Button>
  {/snippet}
</PageHeader>

<div class="settings-view">
  <div class="page-body">
    {#if name === "hadoop"}
      <div class="field-item">
        <span class="field-label">{fieldLabel("jdk_version")}</span>
        <Select
          class="full"
          value={fieldValue("jdk_version")}
          items={jdkItems}
          onSelect={(v) => updateField("jdk_version", v)}
        />
      </div>
      <div class="field-pair">
        <div class="field-item">
          <span class="field-label">{fieldLabel("namenode_web_port")}</span>
          <input
            class="input mono"
            type="number"
            value={fieldValue("namenode_web_port")}
            oninput={(e) => updateField("namenode_web_port", (e.currentTarget as HTMLInputElement).value)}
          />
        </div>
        <div class="field-item">
          <span class="field-label">{fieldLabel("yarn_rm_web_port")}</span>
          <input
            class="input mono"
            type="number"
            value={fieldValue("yarn_rm_web_port")}
            oninput={(e) => updateField("yarn_rm_web_port", (e.currentTarget as HTMLInputElement).value)}
          />
        </div>
      </div>
      <div class="field-pair">
        <div class="field-item">
          <span class="field-label">{fieldLabel("history_enabled")}</span>
          <Switch
            checked={fieldValue("history_enabled") === "true"}
            onCheckedChange={(v) => updateField("history_enabled", v ? "true" : "false")}
          />
        </div>
        <div class="field-item history-port-field" class:on={fieldValue("history_enabled") === "true"}>
          <span class="field-label">{fieldLabel("history_web_port")}</span>
          <input
            class="input mono"
            type="number"
            value={fieldValue("history_web_port")}
            oninput={(e) => updateField("history_web_port", (e.currentTarget as HTMLInputElement).value)}
          />
        </div>
      </div>
    {:else}
      {#each fields as field (field.id)}
        <div class="field-item">
          <span class="field-label">{field.label}</span>
          {#if field.value_type === "version"}
            <Select
              class="full"
              value={field.value}
              items={jdkItems}
              onSelect={(v) => updateField(field.id, v)}
            />
          {:else}
            <input
              class="input mono"
              type="number"
              value={field.value}
              oninput={(e) => updateField(field.id, (e.currentTarget as HTMLInputElement).value)}
            />
          {/if}
        </div>
      {/each}
    {/if}

    {#if fields.length === 0}
      <p class="hint">该组件没有可配置的字段。</p>
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
