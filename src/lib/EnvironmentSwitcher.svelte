<script lang="ts">
  import { Popover } from "bits-ui";
  import { ChevronDown, Plus } from "lucide-svelte";
  import Button from "$lib/components/ui/button/button.svelte";
  import { createEnvironment, store, switchEnvironment } from "$lib/stores.svelte.ts";
  import { toastSuccess } from "$lib/notifications.svelte.ts";

  let open = $state(false);
  let triggerRef = $state<HTMLElement | null>(null);
  let contentStyle = $state("");
  let createOpen = $state(false);
  let modalName = $state("");
  let modalBusy = $state(false);
  let modalInput = $state<HTMLInputElement | null>(null);

  const switching = $derived(Boolean(store.switchingEnvironmentId));
  const actionsDisabled = $derived(switching || store.installing || store.busy);
  const activeEnvironment = $derived(
    store.environments.find((environment) => environment.active) ?? null
  );

  $effect(() => {
    if (open && triggerRef) {
      contentStyle = `width:${Math.round(triggerRef.offsetWidth)}px`;
    } else {
      contentStyle = "";
    }
  });

  $effect(() => {
    if (createOpen && modalInput) {
      modalInput.focus();
    }
  });

  function change(id: string) {
    open = false;
    void switchEnvironment(id);
  }

  function openCreate() {
    if (actionsDisabled) return;
    open = false;
    store.errorMsg = "";
    createOpen = true;
    modalName = "";
  }

  function closeModal() {
    if (modalBusy) return;
    createOpen = false;
    modalName = "";
  }

  async function submitEnvironment() {
    const name = modalName.trim();
    if (!name || modalBusy) return;
    modalBusy = true;
    store.errorMsg = "";
    try {
      await createEnvironment(name);
      toastSuccess(`环境“${name}”已创建`);
      createOpen = false;
      modalName = "";
    } catch (error) {
      store.errorMsg = String(error);
    } finally {
      modalBusy = false;
    }
  }
</script>

<Popover.Root bind:open>
  <Popover.Trigger>
    <Button
      variant="outline"
      size="sm"
      type="button"
      class="environment-switcher"
      bind:ref={triggerRef}
    >
      <span class="select-label">
        {switching ? "正在切换…" : activeEnvironment?.name ?? "选择环境"}
      </span>
      <span class="select-chev"><ChevronDown size={14} /></span>
    </Button>
  </Popover.Trigger>

  <Popover.Content
    side="bottom"
    align="end"
    sideOffset={6}
    class="toolbar-menu environment-menu"
    style={contentStyle}
  >
    <div class="environment-option-list">
      {#each store.environments as environment (environment.id)}
        <button
          class="toolbar-menu-item"
          class:active={environment.active}
          type="button"
          disabled={actionsDisabled}
          onclick={() => change(environment.id)}
        >
          <span class="environment-option-label">{environment.name}</span>
          {#if store.switchingEnvironmentId === environment.id}
            <span class="environment-tag">切换中</span>
          {:else}
            <span class="environment-tag">{environment.components.length}</span>
          {/if}
        </button>
      {/each}
    </div>

    <div class="environment-divider"></div>

    <button
      class="toolbar-menu-item environment-create"
      type="button"
      disabled={actionsDisabled}
      onclick={openCreate}
    >
      <span class="environment-option-label">新建环境</span>
      <span class="environment-action-icon"><Plus size={14} /></span>
    </button>
  </Popover.Content>
</Popover.Root>

{#if createOpen}
  <div
    class="overlay"
    role="presentation"
    onclick={(event) => event.target === event.currentTarget && closeModal()}
  >
    <div class="modal" role="dialog" aria-modal="true" tabindex="-1">
      <h3 class="modal-title">新建环境</h3>
      <input
        class="input"
        bind:this={modalInput}
        bind:value={modalName}
        maxlength="40"
        placeholder="输入环境名称"
        disabled={modalBusy}
        onkeydown={(event) => {
          if (event.key === "Enter") void submitEnvironment();
          if (event.key === "Escape") closeModal();
        }}
      />
      {#if store.errorMsg}
        <p class="msg error">{store.errorMsg}</p>
      {/if}
      <div class="modal-actions">
        <Button variant="ghost" size="sm" onclick={closeModal} disabled={modalBusy}>
          取消
        </Button>
        <Button
          size="sm"
          onclick={submitEnvironment}
          disabled={!modalName.trim() || modalBusy}
        >
          {modalBusy ? "保存中…" : "新建"}
        </Button>
      </div>
    </div>
  </div>
{/if}

<style>
  :global(.environment-switcher) {
    height: 34px;
    max-width: min(220px, 38vw);
    min-width: min(150px, 32vw);
    padding-right: 10px;
  }

  :global(.environment-menu .toolbar-menu-item) {
    padding-right: 4px;
  }

  :global(.environment-menu) {
    gap: 0;
  }

  .select-label {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    text-align: left;
    font-weight: 600;
  }

  .select-chev {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 20px;
    height: 20px;
    flex-shrink: 0;
    color: var(--muted);
  }

  .environment-option-label {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    text-align: left;
  }

  .environment-option-list {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .environment-tag {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-width: 20px;
    height: 20px;
    box-sizing: border-box;
    flex-shrink: 0;
    color: var(--muted);
    font-size: 11px;
    font-weight: 500;
  }

  .environment-action-icon {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 20px;
    height: 20px;
    flex-shrink: 0;
  }

  .environment-create:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .environment-create:disabled:hover {
    background: transparent;
    color: var(--text);
  }

  .environment-create {
    border-radius: 7px;
    padding-top: 5px;
    padding-bottom: 5px;
  }

  .environment-divider {
    height: 1px;
    margin: 5px 0;
    flex-shrink: 0;
    background: var(--line);
  }
</style>
