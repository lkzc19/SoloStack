<script lang="ts">
  import { Switch as BitsSwitch } from "bits-ui";
  import { cn } from "$lib/utils.js";

  let {
    checked = false,
    onCheckedChange,
    disabled = false,
    class: cls = "",
    label = "开关",
  }: {
    checked?: boolean;
    onCheckedChange?: (v: boolean) => void;
    disabled?: boolean;
    class?: string;
    label?: string;
  } = $props();
</script>

<BitsSwitch.Root
  class={cn(
    "peer inline-flex h-6 w-11 shrink-0 cursor-pointer items-center rounded-full border-2 border-transparent transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 disabled:cursor-not-allowed disabled:opacity-50",
    checked ? "bg-primary" : "bg-input",
    cls,
  )}
  checked={checked}
  onCheckedChange={(v) => onCheckedChange?.(v)}
  disabled={disabled}
  aria-label={label}
>
  <BitsSwitch.Thumb
    class={cn(
      "pointer-events-none block h-5 w-5 rounded-full bg-background shadow-lg ring-0 transition-transform",
      checked ? "translate-x-5" : "translate-x-0",
    )}
  />
</BitsSwitch.Root>
