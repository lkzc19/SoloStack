<script lang="ts">
  // 可折叠搜索框，供页头使用。
  //
  // 收起：一个 34×34 的放大镜按钮（与页头其它按钮等大）；
  // 展开：同一个容器变宽成输入框，带宽度过渡（所以收起/展开是同一个元素改宽度，
  //       而不是两个元素互换 —— 否则无法过渡）；
  // 失焦且没有内容时自动收回。
  import { Search } from "lucide-svelte";

  let {
    value = $bindable(""),
    placeholder = "搜索关键词",
  }: { value?: string; placeholder?: string } = $props();

  let open = $state(false);
  let input = $state<HTMLInputElement | null>(null);

  // 有内容时保持展开，否则用户看不到自己输入的关键字
  const expanded = $derived(open || value.trim().length > 0);

  function expand() {
    open = true;
    // preventScroll：容器是 overflow:hidden（滚动容器），展开瞬间输入框还是 0 宽，
    // 默认的"聚焦即滚动到可视区"会把放大镜图标挤出去再弹回来，看着像抖一下。
    queueMicrotask(() => input?.focus({ preventScroll: true }));
  }

  function collapseIfEmpty() {
    if (!value.trim()) open = false;
  }
</script>

<div class="search-toggle" class:open={expanded}>
  <button
    class="search-toggle-trigger"
    type="button"
    title="搜索"
    aria-label="搜索"
    aria-expanded={expanded}
    onclick={expand}
  >
    <Search size={14} />
  </button>
  <input
    bind:this={input}
    bind:value
    {placeholder}
    spellcheck="false"
    tabindex={expanded ? 0 : -1}
    onblur={collapseIfEmpty}
  />
</div>
