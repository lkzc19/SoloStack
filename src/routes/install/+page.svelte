<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { goto } from "$app/navigation";
  import { onMount } from "svelte";
  import Button from "$lib/components/ui/button/button.svelte";
  import PageHeader from "$lib/PageHeader.svelte";
  import Select from "$lib/components/ui/select/select.svelte";
  import {
    componentAdapter,
    supportedComponentIds,
    validateComponentRegistry,
    validateInstallParamIds,
  } from "$lib/component-adapters/registry";
  import { store, doInstall } from "$lib/stores.svelte.ts";
  import type { ManifestInfo, InstallSource, JdkInfo, InstallParam } from "$lib/types";

  let componentConfigs = $state<ManifestInfo[]>([]);
  let installComponents = $state<string[]>(supportedComponentIds());
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
  const installAdapter = $derived(componentAdapter(installComponent));

  onMount(() => {
    loadInstallData();
  });

  async function loadInstallData() {
    store.errorMsg = "";
    try {
      componentConfigs = await invoke<ManifestInfo[]>("list_component_manifests");
      validateComponentRegistry(componentConfigs);
      installComponents = supportedComponentIds();
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
      validateInstallParamIds(comp, ver, declared.map((param) => param.id));
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
          {componentAdapter(c)?.displayName ?? c}
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
      {#if installAdapter}
        {@const InstallFields = installAdapter.installFields}
        <InstallFields
          component={installAdapter.id}
          version={installVersion}
          params={installParams}
          onParamChange={setParam}
        />
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
