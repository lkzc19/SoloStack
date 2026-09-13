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

  interface FieldValue {
    id: string;
    value: string;
  }

  const name = $derived(page.params.name ?? "");
  const selected = $derived(getSelected());

  // 后端只返回「字段当前值」；表单结构/文案/布局完全由前端定义
  let fields = $state<FieldValue[]>([]);
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

  // 单条消息上限档位（前端定义；后端校验是否接受）
  const kafkaMsgItems = [
    { value: "1", label: "1 MB（默认）" },
    { value: "10", label: "10 MB" },
    { value: "50", label: "50 MB" },
    { value: "100", label: "100 MB" },
    { value: "500", label: "500 MB" },
  ];

  onMount(() => {
    store.selectedName = name;
    loadFields(name);
    loadJdks();
  });

  async function loadFields(component: string) {
    try {
      fields = await invoke<FieldValue[]>("list_config_fields", { component });
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

  function valueOf(id: string) {
    return fields.find((f) => f.id === id)?.value ?? "";
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
      for (const f of fields) {
        // JDK 未解析出来时下拉是空的：空值提交必然失败并中断整次保存（后续字段不再写、
        // 也不会重启组件），而它本身没有要改的内容，直接跳过。
        if (f.id === "jdk_version" && !f.value.trim()) continue;
        await invoke("set_config_field", {
          component: name,
          fieldId: f.id,
          value: f.value,
        });
      }
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
      <!-- Hadoop 显式表单（前端决定布局与文案） -->
      <div class="field-item">
        <span class="field-label">JDK 版本</span>
        <Select
          class="full field-select"
          value={valueOf("jdk_version")}
          items={jdkItems}
          onSelect={(v) => updateField("jdk_version", v)}
        />
      </div>
      <div class="field-pair">
        <div class="field-item">
          <span class="field-label">HDFS WebUI 端口</span>
          <input
            class="input mono"
            type="number"
            min="1024"
            max="65535"
            value={valueOf("namenode_web_port")}
            oninput={(e) => updateField("namenode_web_port", (e.currentTarget as HTMLInputElement).value)}
          />
        </div>
        <div class="field-item">
          <span class="field-label">YARN WebUI 端口</span>
          <input
            class="input mono"
            type="number"
            min="1024"
            max="65535"
            value={valueOf("yarn_rm_web_port")}
            oninput={(e) => updateField("yarn_rm_web_port", (e.currentTarget as HTMLInputElement).value)}
          />
        </div>
      </div>
      <div class="field-pair">
        <div class="field-item">
          <span class="field-label">JobHistory</span>
          <Switch
            checked={valueOf("history_enabled") === "true"}
            onCheckedChange={(v) => updateField("history_enabled", v ? "true" : "false")}
          />
        </div>
        {#if valueOf("history_enabled") === "true"}
          <div class="field-item">
            <span class="field-label">JobHistory WebUI 端口</span>
            <input
              class="input mono"
              type="number"
              min="1024"
              max="65535"
              value={valueOf("history_web_port")}
              oninput={(e) => updateField("history_web_port", (e.currentTarget as HTMLInputElement).value)}
            />
          </div>
        {/if}
      </div>
    {:else if name === "kafka"}
      <!-- Kafka 显式表单 -->
      <div class="field-item">
        <span class="field-label">JDK 版本</span>
        <Select
          class="full field-select"
          value={valueOf("jdk_version")}
          items={jdkItems}
          onSelect={(v) => updateField("jdk_version", v)}
        />
      </div>
      <div class="field-pair">
        <div class="field-item">
          <span class="field-label">Broker 端口</span>
          <input
            class="input mono"
            type="number"
            min="1024"
            max="65535"
            value={valueOf("broker_port")}
            oninput={(e) => updateField("broker_port", (e.currentTarget as HTMLInputElement).value)}
          />
        </div>
        <div class="field-item">
          <span class="field-label">默认分区数</span>
          <input
            class="input mono"
            type="number"
            value={valueOf("num_partitions")}
            oninput={(e) => updateField("num_partitions", (e.currentTarget as HTMLInputElement).value)}
          />
        </div>
      </div>
      <div class="field-pair">
        <div class="field-item">
          <span class="field-label">消息保留时长（小时）</span>
          <input
            class="input mono"
            type="number"
            value={valueOf("retention_hours")}
            oninput={(e) => updateField("retention_hours", (e.currentTarget as HTMLInputElement).value)}
          />
        </div>
        <div class="field-item">
          <span class="field-label">单条消息上限</span>
          <Select
            class="full field-select"
            value={valueOf("message_max_mb")}
            items={kafkaMsgItems}
            onSelect={(v) => updateField("message_max_mb", v)}
          />
        </div>
      </div>
      <div class="field-item">
        <span class="field-label">自动创建 Topic</span>
        <Switch
          checked={valueOf("auto_create_topics") === "true"}
          onCheckedChange={(v) => updateField("auto_create_topics", v ? "true" : "false")}
        />
      </div>
    {:else}
      <p class="hint">该组件没有可配置的字段。</p>
    {/if}

    {#if fields.length === 0 && (name === "hadoop" || name === "kafka")}
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
