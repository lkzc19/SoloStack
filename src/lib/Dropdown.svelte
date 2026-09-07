<script lang="ts">
  // 自定义下拉组件（Soft UI 风格）：按钮触发 + 自绘弹出列表
  import type { Snippet } from "svelte";

  let {
    value,
    items,
    placeholder = "请选择",
    onChange,
    class: cls = "",
    outline = false,
    lead,
  }: {
    value: string;
    items: { value: string; label: string; disabled?: boolean; tag?: string; icon?: string }[];
    placeholder?: string;
    onChange?: (value: string) => void;
    class?: string;
    outline?: boolean;
    lead?: Snippet;
  } = $props();

  let open = $state(false);
  let rootEl: HTMLDivElement;
  let menuStyle = $state({ top: "0px", left: "0px", width: "auto" });

  const selected = $derived(items.find((i) => i.value === value) ?? null);

  function toggle() {
    if (!open && rootEl) {
      const rect = rootEl.getBoundingClientRect();
      menuStyle = {
        top: `${rect.bottom + 6}px`,
        left: `${rect.left}px`,
        width: `${rect.width}px`,
      };
    }
    open = !open;
  }

  function pick(v: string) {
    open = false;
    onChange?.(v);
  }
  function isDisabled(item: { value: string; label: string; disabled?: boolean }) {
    return !!item.disabled;
  }
  function hasTag(item: { value: string; label: string; tag?: string }) {
    return !!item.tag;
  }

  // 点击组件外部时关闭。用 setTimeout 延后注册，避免「本次点击」的
  // mousedown 事件在 toggle 打开后立刻把菜单又关掉。
  $effect(() => {
    if (!open) return;
    let added = false;
    const onDoc = (e: MouseEvent) => {
      if (rootEl && !rootEl.contains(e.target as Node)) open = false;
    };
    const timer = setTimeout(() => {
      document.addEventListener("mousedown", onDoc);
      added = true;
    }, 0);
    return () => {
      clearTimeout(timer);
      if (added) document.removeEventListener("mousedown", onDoc);
    };
  });
</script>

<div class="dropdown {cls}" bind:this={rootEl}>
  <button
    type="button"
    class="trigger"
    class:outline
    class:open
    onmousedown={(e) => {
      e.preventDefault();
      toggle();
    }}
    aria-haspopup="listbox"
    aria-expanded={open}
  >
    {#if lead}
      <span class="lead">{@render lead()}</span>
    {/if}
    {#if selected?.icon}
      <img class="trigger-icon" src={`/icons/${selected.icon}.png`} alt="" />
    {/if}
    <span class="label">{selected?.label ?? placeholder}</span>
    {#if selected?.tag}
      <span class="trigger-tag">{selected.tag}</span>
    {/if}
    <svg
      class="chev"
      class:open
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      stroke-width="2"
      stroke-linecap="round"
      stroke-linejoin="round"
    >
      <polyline points="6 9 12 15 18 9"></polyline>
    </svg>
  </button>

  {#if open}
    <ul
      class="menu"
      role="listbox"
      style={`top:${menuStyle.top};left:${menuStyle.left};width:${menuStyle.width}`}
    >
      {#each items as item (item.value)}
        <li>
          <button
            class="item"
            class:active={item.value === value}
            class:disabled={isDisabled(item)}
            onmousedown={(e) => {
              e.preventDefault();
              pick(item.value);
            }}
            disabled={isDisabled(item)}
            role="option"
            aria-selected={item.value === value}
          >
            {#if item.icon}
              <img class="item-icon" src={`/icons/${item.icon}.png`} alt="" />
            {/if}
            <span class="item-label">{item.label}</span>
            {#if hasTag(item)}
              <span class="item-tag">{item.tag}</span>
            {/if}
            {#if item.value === value}
              <span class="check">✓</span>
            {/if}
          </button>
        </li>
      {/each}
      {#if items.length === 0}
        <li class="empty-item">无选项</li>
      {/if}
    </ul>
  {/if}
</div>

<style>
  .dropdown {
    position: relative;
    display: inline-block;
  }
  .dropdown.full {
    width: 100%;
  }
  .dropdown.full .trigger {
    width: 100%;
  }
  .trigger {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    min-width: 130px;
    padding: 9px 14px;
    border: none;
    border-radius: var(--radius);
    background: #f1f5f9;
    color: var(--text);
    font-size: 12px;
    text-align: left;
    cursor: pointer;
    transition: all 0.2s ease-in-out;
  }
  .trigger:hover {
    background: #e2e8f0;
  }
  .trigger.open {
    background: #ffffff;
    box-shadow: 0 0 0 3px rgba(66, 184, 131, 0.25);
  }
  /* outline 变体：与日期按钮一致的白底描边（非输入框感） */
  .trigger.outline {
    background: var(--panel);
    border: 1px solid var(--line);
    color: var(--text);
    min-height: 32px;
    padding: 0 14px;
  }
  .trigger.outline:hover {
    background: #f8fafc;
    border-color: var(--primary);
  }
  .trigger.outline.open {
    background: var(--panel);
    border-color: var(--primary);
  }
  .label {
    flex: 1;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .trigger-icon {
    width: 16px;
    height: 16px;
    flex-shrink: 0;
    border-radius: 4px;
  }
  .lead {
    display: inline-flex;
    align-items: center;
    flex-shrink: 0;
    color: var(--muted);
  }
  .trigger-tag {
    font-size: 10px;
    font-weight: 600;
    letter-spacing: 0.02em;
    color: #047857;
    background: rgba(16, 185, 129, 0.12);
    padding: 2px 7px;
    border-radius: 999px;
    flex-shrink: 0;
  }
  .chev {
    width: 14px;
    height: 14px;
    color: var(--muted);
    flex-shrink: 0;
    transition: transform 0.2s ease;
  }
  .chev.open {
    transform: rotate(180deg);
  }
  .menu {
    position: fixed;
    z-index: 1000;
    min-width: 0;
    box-sizing: border-box;
    max-height: 280px;
    overflow: auto;
    list-style: none;
    margin: 0;
    padding: 6px;
    display: flex;
    flex-direction: column;
    gap: 2px;
    background: #ffffff;
    border-radius: var(--radius);
    box-shadow: 0 12px 32px rgba(15, 23, 42, 0.16);
    animation: menuIn 0.15s ease;
  }
  @keyframes menuIn {
    from {
      opacity: 0;
      transform: translateY(-4px);
    }
    to {
      opacity: 1;
      transform: translateY(0);
    }
  }
  .item {
    width: 100%;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    padding: 8px 12px;
    border: none;
    border-radius: var(--radius);
    background: transparent;
    color: var(--text);
    font-size: 12px;
    cursor: pointer;
    text-align: left;
    transition: all 0.15s ease;
  }
  .item-label {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .item-icon {
    width: 16px;
    height: 16px;
    flex-shrink: 0;
    border-radius: 4px;
  }
  .item-tag {
    font-size: 10px;
    font-weight: 600;
    letter-spacing: 0.02em;
    color: #047857;
    background: rgba(16, 185, 129, 0.12);
    padding: 2px 7px;
    border-radius: 999px;
    flex-shrink: 0;
  }
  .item:hover {
    background: rgba(66, 184, 131, 0.08);
    color: var(--primary-deep);
  }
  .item.active {
    background: rgba(66, 184, 131, 0.1);
    color: var(--primary-deep);
    font-weight: 600;
  }
  .item.disabled {
    color: var(--muted);
    opacity: 0.5;
    cursor: not-allowed;
  }
  .item.disabled:hover {
    background: transparent;
    color: var(--muted);
  }
  .check {
    color: var(--primary);
    font-weight: 700;
  }
  .empty-item {
    padding: 8px 12px;
    color: var(--muted);
    font-size: 12px;
  }
</style>
