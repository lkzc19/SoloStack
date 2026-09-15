<script lang="ts">
  import Switch from "$lib/components/ui/switch/switch.svelte";
  import type { ConfigFieldsProps } from "$lib/types";

  let { values, onFieldChange }: ConfigFieldsProps = $props();
</script>

<div class="field-pair">
  <div class="field-item">
    <span class="field-label">HDFS WebUI 端口</span>
    <input
      class="input mono"
      type="number"
      min="1024"
      max="65535"
      value={values.namenode_web_port ?? ""}
      oninput={(event) =>
        onFieldChange("namenode_web_port", (event.currentTarget as HTMLInputElement).value)}
    />
  </div>
  <div class="field-item">
    <span class="field-label">YARN WebUI 端口</span>
    <input
      class="input mono"
      type="number"
      min="1024"
      max="65535"
      value={values.yarn_rm_web_port ?? ""}
      oninput={(event) =>
        onFieldChange("yarn_rm_web_port", (event.currentTarget as HTMLInputElement).value)}
    />
  </div>
</div>

<div class="field-pair">
  <div class="field-item">
    <span class="field-label">JobHistory</span>
    <Switch
      checked={values.history_enabled === "true"}
      onCheckedChange={(value) => onFieldChange("history_enabled", value ? "true" : "false")}
    />
  </div>
  {#if values.history_enabled === "true"}
    <div class="field-item">
      <span class="field-label">JobHistory WebUI 端口</span>
      <input
        class="input mono"
        type="number"
        min="1024"
        max="65535"
        value={values.history_web_port ?? ""}
        oninput={(event) =>
          onFieldChange("history_web_port", (event.currentTarget as HTMLInputElement).value)}
      />
    </div>
  {/if}
</div>
