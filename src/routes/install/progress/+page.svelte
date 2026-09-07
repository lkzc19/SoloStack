<script lang="ts">
  import { goto } from "$app/navigation";
  import Button from "$lib/components/ui/button/button.svelte";
  import PageHeader from "$lib/PageHeader.svelte";
  import { store, cancelInstall, finishInstall } from "$lib/stores.svelte.ts";
</script>

<PageHeader title={`安装 ${store.installComponent}`} />

<div class="install-view">
  <div class="page-body">
    <div class="install-log">
      {#each store.installLines as line (line)}
        <p class="install-line mono" class:done={line === "安装完成"}>{line}</p>
      {/each}
      {#if store.installing}
        <p class="install-line mono"><span class="log-spinner"></span> 处理中…</p>
      {/if}
    </div>
    {#if store.installing}
      <div class="install-actions">
        <Button variant="destructive" size="sm" onclick={cancelInstall}>终止安装</Button>
      </div>
    {:else if store.installDone}
      <div class="install-actions">
        <Button size="sm" onclick={finishInstall}>完成</Button>
      </div>
    {:else if store.installFailed}
      <div class="install-actions">
        <Button
          size="sm"
          onclick={() => { store.installFailed = false; goto("/install"); }}
        >
          返回安装页
        </Button>
      </div>
    {/if}
  </div>
</div>
