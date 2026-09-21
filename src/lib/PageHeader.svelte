<script lang="ts">
  import { ArrowLeft } from "lucide-svelte";
  import { goto } from "$app/navigation";
  import type { Snippet } from "svelte";

  let {
    title,
    onBack = () => goto("/"),
    actions,
  }: { title: string; onBack?: () => void; actions?: Snippet } = $props();
</script>

<header class="page-header" data-tauri-drag-region>
  <button class="gear-btn" onclick={onBack} aria-label="返回" title="返回">
    <ArrowLeft size={16} />
  </button>
  <h2 class="page-title">{title}</h2>
  <div class="page-header-spacer"></div>
  {#if actions}
    {@render actions()}
  {/if}
</header>
