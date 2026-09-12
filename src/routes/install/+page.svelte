<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { goto } from "$app/navigation";
  import { onMount } from "svelte";
  import Button from "$lib/components/ui/button/button.svelte";
  import PageHeader from "$lib/PageHeader.svelte";
  import Select from "$lib/components/ui/select/select.svelte";
  import Switch from "$lib/components/ui/switch/switch.svelte";
  import { store, doInstall } from "$lib/stores.svelte.ts";
  import type { ManifestInfo, InstallSource, JdkInfo, InstallParam } from "$lib/types";

  let componentConfigs = $state<ManifestInfo[]>([]);
  let installComponents = $state<string[]>([]);
  let installComponent = $state("");
  let installVersions = $state<string[]>([]);
  let installVersion = $state("");
  let installSources = $state<InstallSource[]>([]);
  let installSourceId = $state("");
  let installJdk = $state("");
  let jdks = $state<JdkInfo[]>([]);
  let installJdkSupported = $state<string[]>([]);
  let installDownloaded = $state<Record<string, boolean>>({});
  // 安装参数：值全部来自组件的声明（list_install_params），前端不再写死默认值。
  // 表单的布局与文案仍按组件定制（各组件字段差异大，不做通用化）。
  let installParams = $state<Record<string, string>>({});

  const kafkaMsgItems = $derived(
    ["1", "10", "50", "100", "500"].map((v) => ({
      value: v,
      label: v === "1" ? "1 MB（默认）" : `${v} MB`,
    }))
  );

  const jdkItems = $derived(
    jdks.map((j) => ({
      value: j.name,
      label: `${j.vendor} JDK ${j.version}`,
      disabled: !installJdkSupported.includes(j.version),
    }))
  );

  const versionItems = $derived(
    installVersions.map((v) => ({
      value: v,
      label: v,
      tag: installDownloaded[v] ? "已下载" : undefined,
    }))
  );

  // 当前组件是否需要 Java（config json 有 java_support 才需要）
  const currentConfig = $derived(componentConfigs.find((c) => c.component === installComponent) ?? null);
  const hasJava = $derived(Object.keys(currentConfig?.java_support ?? {}).length > 0);

  onMount(() => {
    loadInstallData();
  });

  async function loadInstallData() {
    store.errorMsg = "";
    try {
      componentConfigs = await invoke<ManifestInfo[]>("list_component_manifests");
      installComponents = ["hadoop", "kafka"];
      if (!installComponents.includes(installComponent)) {
        installComponent = installComponents[0] ?? "";
      }
      jdks = await invoke<JdkInfo[]>("list_jdk_versions");
      loadComponentData();
    } catch (e) {
      store.errorMsg = String(e);
    }
  }

  function selectInstallComponent(name: string) {
    if (name === installComponent) return;
    installComponent = name;
    loadComponentData();
  }

  function loadComponentData() {
    const cfg = componentConfigs.find((c) => c.component === installComponent);
    installSources = cfg?.source ?? [];
    installSourceId = installSources[0]?.name ?? "";
    installVersion = "";
    refreshVersions();
  }

  function refreshVersions() {
    const cfg = componentConfigs.find((c) => c.component === installComponent);
    const src = cfg?.source.find((s) => s.name === installSourceId);
    const versions = src ? Object.keys(src.versions) : [];
    installVersions = versions;
    if (!installVersions.includes(installVersion)) {
      installVersion = installVersions[0] ?? "";
    }
    refreshJdk();
    refreshDownloaded();
    void refreshInstallParams();
  }

  function paramOf(id: string): string {
    return installParams[id] ?? "";
  }

  function setParam(id: string, value: string) {
    installParams = { ...installParams, [id]: value };
  }

  /// 拉取该组件该版本的参数默认值并预填；切换组件/版本时整体替换（顺带丢掉上一个组件的键）
  async function refreshInstallParams() {
    const comp = installComponent;
    const ver = installVersion;
    if (!comp || !ver) {
      installParams = {};
      return;
    }
    try {
      const declared = await invoke<InstallParam[]>("list_install_params", {
        component: comp,
        version: ver,
      });
      if (comp !== installComponent || ver !== installVersion) return;
      installParams = Object.fromEntries(declared.map((d) => [d.id, d.default]));
    } catch (e) {
      store.errorMsg = String(e);
    }
  }

  async function refreshDownloaded() {
    const comp = installComponent;
    const sid = installSourceId;
    const versions = [...installVersions];
    const map: Record<string, boolean> = {};
    await Promise.all(
      versions.map(async (v) => {
        try {
          map[v] = await invoke<boolean>("is_package_downloaded", {
            component: comp,
            sourceId: sid,
            version: v,
          });
        } catch {
          map[v] = false;
        }
      })
    );
    if (comp === installComponent && sid === installSourceId) {
      installDownloaded = map;
    }
  }

  function refreshJdk() {
    if (!hasJava) {
      installJdkSupported = [];
      installJdk = "";
      return;
    }
    const cfg = componentConfigs.find((c) => c.component === installComponent);
    const supported = (cfg?.java_support[installVersion] ?? []).map(String);
    installJdkSupported = supported;
    const cur = jdks.find((j) => j.name === installJdk);
    if (!cur || !installJdkSupported.includes(cur.version)) {
      const first = jdks.find((j) => installJdkSupported.includes(j.version));
      installJdk = first?.name ?? "";
    }
  }

  function startInstall() {
    if (store.installing || !installComponent || !installVersion) return;
    if (hasJava && !installJdk) return;
    goto("/install/progress");
    void doInstall({
      component: installComponent,
      version: installVersion,
      sourceId: installSourceId,
      jdkVersion: hasJava ? installJdk : "",
      // 参数是「id → 字符串」的通用载体；留空的项由组件回退到它的默认值
      params: { ...installParams },
    });
  }
</script>

<PageHeader title="安装组件" />

<div class="install-view">
  <div class="page-body">
    <div class="component-tabs">
      {#each installComponents as c (c)}
        <button
          class="component-tab"
          class:active={installComponent === c}
          onclick={() => selectInstallComponent(c)}
        >
          {c}
        </button>
      {/each}
    </div>

    <section class="settings-section">
      <div class="install-fields">
        <div class="install-field">
          <span class="install-label">下载源</span>
          <Select
            class="full field-select"
            value={installSourceId}
            items={installSources.map((s) => ({ value: s.name, label: s.name }))}
            onSelect={(v) => {
              installSourceId = v;
              refreshVersions();
            }}
          />
        </div>
        <div class="install-field">
          <span class="install-label">版本</span>
          <Select
            class="full field-select"
            value={installVersion}
            items={versionItems}
            onSelect={(v) => {
              installVersion = v;
              refreshJdk();
              void refreshInstallParams();
            }}
          />
        </div>
      </div>
    </section>

    <section class="settings-section">
      {#if hasJava}
        <div class="install-fields" style="margin-bottom: 1.2rem;">
          <div class="install-field">
            <span class="install-label">JDK 版本</span>
            <Select
              class="full field-select"
              value={installJdk}
              items={jdkItems}
              onSelect={(v) => (installJdk = v)}
            />
            {#if !installJdk}
              <p class="hint-inline">本机没有该组件支持的 JDK，无法安装。</p>
            {/if}
          </div>
        </div>
      {/if}
      {#if installComponent === "hadoop"}
        <div class="install-fields" style="margin-top: 1.2rem;">
          <div class="install-field">
            <span class="install-label">HDFS WebUI 端口</span>
            <input
              class="input mono"
              type="number"
              min="1024"
              max="65535"
              value={paramOf("namenode_web_port")}
              oninput={(e) => setParam("namenode_web_port", e.currentTarget.value)}
            />
          </div>
          <div class="install-field">
            <span class="install-label">YARN WebUI 端口</span>
            <input
              class="input mono"
              type="number"
              min="1024"
              max="65535"
              value={paramOf("yarn_rm_web_port")}
              oninput={(e) => setParam("yarn_rm_web_port", e.currentTarget.value)}
            />
          </div>
        </div>
        <div class="install-fields" style="margin-top: 1.2rem;">
          <div class="install-field">
            <span class="install-label">JobHistory</span>
            <Switch
              checked={paramOf("history_enabled") === "true"}
              onCheckedChange={(v) => setParam("history_enabled", v ? "true" : "false")}
            />
          </div>
          <div class="install-field history-port-field" class:on={paramOf("history_enabled") === "true"}>
            <span class="install-label">JobHistory WebUI 端口</span>
            <input
              class="input mono"
              type="number"
              min="1024"
              max="65535"
              value={paramOf("history_web_port")}
              oninput={(e) => setParam("history_web_port", e.currentTarget.value)}
            />
          </div>
        </div>
      {/if}
      {#if installComponent === "kafka"}
        <div class="install-fields" style="margin-top: 1.2rem;">
          <div class="install-field">
            <span class="install-label">Broker 端口</span>
            <input
              class="input mono"
              type="number"
              min="1024"
              max="65535"
              value={paramOf("broker_port")}
              oninput={(e) => setParam("broker_port", e.currentTarget.value)}
            />
          </div>
          <div class="install-field">
            <span class="install-label">默认分区数</span>
            <input
              class="input mono"
              type="number"
              value={paramOf("num_partitions")}
              oninput={(e) => setParam("num_partitions", e.currentTarget.value)}
            />
          </div>
        </div>
        <div class="install-fields" style="margin-top: 1.2rem;">
          <div class="install-field">
            <span class="install-label">消息保留时长（小时）</span>
            <input
              class="input mono"
              type="number"
              value={paramOf("retention_hours")}
              oninput={(e) => setParam("retention_hours", e.currentTarget.value)}
            />
          </div>
          <div class="install-field">
            <span class="install-label">单条消息上限</span>
            <Select
              class="full field-select"
              value={paramOf("message_max_mb")}
              items={kafkaMsgItems}
              onSelect={(v) => setParam("message_max_mb", v)}
            />
          </div>
        </div>
        <div class="install-fields" style="margin-top: 1.2rem;">
          <div class="install-field">
            <span class="install-label">自动创建 Topic</span>
            <Switch
              checked={paramOf("auto_create_topics") === "true"}
              onCheckedChange={(v) => setParam("auto_create_topics", v ? "true" : "false")}
            />
          </div>
        </div>
      {/if}
    </section>

    {#if store.errorMsg}
      <p class="msg error">{store.errorMsg}</p>
    {/if}

    <div class="install-actions">
      <Button
        size="md"
        onclick={startInstall}
        disabled={store.installing || !installComponent || !installVersion || (hasJava && !installJdk)}
      >
        {store.installing ? "安装中…" : "开始安装"}
      </Button>
      {#if store.installing}
        <span class="log-spinner"></span>
      {/if}
    </div>
  </div>
</div>
