<script lang="ts">
  import { Popover } from "bits-ui";
  import { Check, ChevronDown } from "lucide-svelte";
  import Button from "../button/button.svelte";

  let {
    value = "",
    items = [] as Item[],
    placeholder = "请选择",
    onSelect,
    class: cls = "",
    align = "start",
  }: {
    value?: string;
    items?: Item[];
    placeholder?: string;
    onSelect?: (v: string) => void;
    class?: string;
    align?: "start" | "center" | "end";
  } = $props();

  interface Item {
    value: string;
    label: string;
    disabled?: boolean;
    icon?: string;
    tag?: string;
  }

  const selected = $derived(items.find((i) => i.value === value) ?? null);
  // 触发器宽度：原 API 用 class="full" 表示占满
  const triggerClass = $derived(cls.replace("full", "w-full"));

  let open = $state(false);
  let triggerRef = $state<HTMLElement | null>(null);
  let contentStyle = $state("");

  // 下拉弹层与触发器等宽，保证与同表单里的输入框对齐
  $effect(() => {
    if (open && triggerRef) {
      contentStyle = `width:${Math.round(triggerRef.offsetWidth)}px`;
    } else {
      contentStyle = "";
    }
  });

  function pick(it: Item) {
    if (it.disabled) return;
    onSelect?.(it.value);
    open = false;
  }
</script>

<Popover.Root bind:open>
  <Popover.Trigger>
    <Button
      variant="outline"
      size="md"
      type="button"
      class={triggerClass}
      bind:ref={triggerRef}
    >
      {#if selected?.icon}
        <img class="select-item-icon" src={`/icons/${selected.icon}.png`} alt="" />
      {/if}
      <span class="select-label">{selected?.label ?? placeholder}</span>
      {#if selected?.tag}
        <span class="select-tag">{selected.tag}</span>
      {/if}
      <ChevronDown size={14} class="select-chev" />
    </Button>
  </Popover.Trigger>
  <Popover.Content
    side="bottom"
    align={align}
    sideOffset={6}
    class="toolbar-menu"
    style={contentStyle}
  >
    {#each items as it (it.value)}
      <button
        class="toolbar-menu-item"
        class:active={it.value === value}
        class:select-disabled={it.disabled}
        type="button"
        disabled={it.disabled}
        onclick={() => pick(it)}
      >
        {#if it.icon}
          <img class="select-item-icon" src={`/icons/${it.icon}.png`} alt="" />
        {/if}
        <span class="select-opt-label">{it.label}</span>
        {#if it.tag}
          <span class="select-tag">{it.tag}</span>
        {/if}
        {#if it.value === value}
          <Check size={13} />
        {/if}
      </button>
    {/each}
  </Popover.Content>
</Popover.Root>

<style>
  .select-label {
    flex: 1;
    text-align: left;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .select-item-icon {
    width: 15px;
    height: 15px;
    border-radius: 3px;
    flex-shrink: 0;
  }
  .select-tag {
    font-size: 10px;
    font-weight: 600;
    color: #047857;
    background: rgba(16, 185, 129, 0.12);
    padding: 1px 7px;
    border-radius: 999px;
    flex-shrink: 0;
  }
  .select-opt-label {
    flex: 1;
    text-align: left;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  :global(.toolbar-menu-item.select-disabled) {
    opacity: 0.5;
    cursor: not-allowed;
  }
  :global(.toolbar-menu-item.select-disabled:hover) {
    background: transparent;
    color: var(--text);
  }
</style>
