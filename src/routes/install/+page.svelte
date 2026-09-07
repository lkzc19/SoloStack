<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { goto } from "$app/navigation";
  import { onMount } from "svelte";
  import Button from "$lib/components/ui/button/button.svelte";
  import PageHeader from "$lib/PageHeader.svelte";
  import Select from "$lib/components/ui/select/select.svelte";
  import Switch from "$lib/components/ui/switch/switch.svelte";
  import { store, doInstall } from "$lib/stores.svelte.ts";
  import type { ComponentConfigInfo, InstallSource, JdkInfo } from "$lib/types";

  let componentConfigs = $state<ComponentConfigInfo[]>([]);
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
  let namenodeWeb = $state(9870);
  let yarnRm = $state(8088);
  let historyEnabled = $state(false);
  let historyWebPort = $state(19888);

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

  onMount(() => {
    loadInstallData();
  });

  async function loadInstallData() {
    store.errorMsg = "";
    try {
      componentConfigs = await invoke<ComponentConfigInfo[]>("list_component_configs");
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
    const cfg = componentConfigs.find((c) => c.id === installComponent);
    installSources = cfg?.source ?? [];
    installSourceId = installSources[0]?.name ?? "";
    installVersion = "";
    refreshVersions();
  }

  function refreshVersions() {
    const cfg = componentConfigs.find((c) => c.id === installComponent);
    const src = cfg?.source.find((s) => s.name === installSourceId);
    const versions = src ? Object.keys(src.versions) : [];
    installVersions = versions;
    if (!installVersions.includes(installVersion)) {
      installVersion = installVersions[0] ?? "";
    }
    refreshJdk();
    refreshDownloaded();
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
    const cfg = componentConfigs.find((c) => c.id === installComponent);
    const supported = (cfg?.java_support[installVersion] ?? []).map(String);
    installJdkSupported = supported;
    const cur = jdks.find((j) => j.name === installJdk);
    if (!cur || !installJdkSupported.includes(cur.version)) {
      const first = jdks.find((j) => installJdkSupported.includes(j.version));
      installJdk = first?.name ?? "";
    }
  }

  function startInstall() {
    if (store.installing || !installComponent || !installVersion || !installJdk) return;
    goto("/install/progress");
    void doInstall({
      component: installComponent,
      version: installVersion,
      sourceId: installSourceId,
      jdkVersion: installJdk,
      ports: installComponent === "hadoop"
        ? { namenode_web: namenodeWeb, yarn_rm: yarnRm, history_enabled: historyEnabled, history_web_port: historyWebPort }
        : null,
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
            class="full"
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
            class="full"
            value={installVersion}
            items={versionItems}
            onSelect={(v) => {
              installVersion = v;
              refreshJdk();
            }}
          />
        </div>
      </div>
    </section>

    <section class="settings-section">
      <div class="install-fields" style="margin-bottom: 1.2rem;">
        <div class="install-field">
          <span class="install-label">JDK 版本</span>
          <Select
            class="full"
            value={installJdk}
            items={jdkItems}
            onSelect={(v) => (installJdk = v)}
          />
          {#if !installJdk}
            <p class="hint-inline">本机没有该组件支持的 JDK，无法安装。</p>
          {/if}
        </div>
      </div>
      {#if installComponent === "hadoop"}
        <div class="install-fields" style="margin-top: 1.2rem;">
          <div class="install-field">
            <span class="install-label">HDFS WebUI 端口</span>
            <input class="input mono" type="number" bind:value={namenodeWeb} />
          </div>
          <div class="install-field">
            <span class="install-label">YARN WebUI 端口</span>
            <input class="input mono" type="number" bind:value={yarnRm} />
          </div>
        </div>
        <div class="install-fields" style="margin-top: 1.2rem;">
          <div class="install-field">
            <span class="install-label">JobHistory</span>
            <Switch checked={historyEnabled} onCheckedChange={(v) => (historyEnabled = v)} />
          </div>
          <div class="install-field history-port-field" class:on={historyEnabled}>
            <span class="install-label">JobHistory WebUI 端口</span>
            <input class="input mono" type="number" bind:value={historyWebPort} />
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
        disabled={store.installing || !installComponent || !installVersion || !installJdk}
      >
        {store.installing ? "安装中…" : "开始安装"}
      </Button>
      {#if store.installing}
        <span class="log-spinner"></span>
      {/if}
    </div>
  </div>
</div>
