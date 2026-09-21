<script lang="ts">
  // 设置页外壳：只负责 tab 切换；各 tab 自成组件、自管状态与加载。
  import PageHeader from "$lib/PageHeader.svelte";
  import { store } from "$lib/stores.svelte.ts";
  import GeneralTab from "./tabs/GeneralTab.svelte";
  import EnvironmentsTab from "./tabs/EnvironmentsTab.svelte";
  import CacheTab from "./tabs/CacheTab.svelte";
  import AdvancedTab from "./tabs/AdvancedTab.svelte";
  import AboutTab from "./tabs/AboutTab.svelte";

  type SettingsTab = "general" | "environments" | "cache" | "advanced" | "about";

  let settingsTab = $state<SettingsTab>("general");

  const TABS: { id: SettingsTab; label: string }[] = [
    { id: "general", label: "通用" },
    { id: "environments", label: "环境" },
    { id: "cache", label: "缓存" },
    { id: "advanced", label: "高级" },
    { id: "about", label: "关于" },
  ];
</script>

<PageHeader title="设置" />

<div class="settings-view">
  <div class="settings-sticky">
    <div class="settings-tabs">
      {#each TABS as tab (tab.id)}
        <button
          class="settings-tab"
          type="button"
          class:active={settingsTab === tab.id}
          onclick={() => (settingsTab = tab.id)}
        >
          {tab.label}
        </button>
      {/each}
    </div>
  </div>

  <div class="page-body">
    {#if settingsTab === "general"}
      <GeneralTab />
    {:else if settingsTab === "environments"}
      <EnvironmentsTab />
    {:else if settingsTab === "cache"}
      <CacheTab />
    {:else if settingsTab === "advanced"}
      <AdvancedTab />
    {:else}
      <AboutTab />
    {/if}
    {#if store.errorMsg}
      <p class="msg error">{store.errorMsg}</p>
    {/if}
  </div>
</div>
