<script lang="ts">
  import { Button as BitsButton } from "bits-ui";

  import { cn } from "$lib/utils.js";
  import type { Snippet } from "svelte";
  import type { HTMLButtonAttributes } from "svelte/elements";

  let {
    variant = "default",
    size = "md",
    class: cls = "",
    children,
    type = "button",
    ...rest
  }: {
    variant?: "default" | "destructive" | "outline" | "secondary" | "ghost";
    size?: "xs" | "sm" | "md" | "icon";
    class?: string;
    children?: Snippet;
    type?: "button" | "submit" | "reset";
  } & HTMLButtonAttributes = $props();

  const variants: Record<string, string> = {
    default:
      "bg-primary text-primary-foreground shadow-sm hover:bg-primary/90",
    destructive:
      "bg-destructive text-destructive-foreground shadow-sm hover:bg-destructive/90",
    outline:
      "border border-border bg-card text-foreground hover:border-primary hover:text-primary",
    secondary: "bg-secondary text-secondary-foreground hover:bg-secondary/70",
    ghost: "hover:bg-accent hover:text-accent-foreground",
  };
  const sizes: Record<string, string> = {
    xs: "h-7 px-3 text-xs",
    sm: "h-8 px-4 text-xs",
    md: "h-10 px-5 text-sm",
    icon: "h-9 w-9",
  };
</script>

<BitsButton.Root
  {type}
  {...rest}
  class={cn(
    "inline-flex shrink-0 cursor-pointer items-center justify-center gap-1.5 whitespace-nowrap rounded-md font-normal transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring disabled:pointer-events-none disabled:opacity-50",
    variants[variant],
    sizes[size],
    cls,
  )}
>
  {@render children?.()}
</BitsButton.Root>
