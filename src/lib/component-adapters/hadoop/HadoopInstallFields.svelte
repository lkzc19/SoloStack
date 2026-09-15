<script lang="ts">
  import Switch from "$lib/components/ui/switch/switch.svelte";
  import type { InstallFieldsProps } from "$lib/types";

  let { params, onParamChange }: InstallFieldsProps = $props();
</script>

<div class="install-fields" style="margin-top: 1.2rem;">
  <div class="install-field">
    <span class="install-label">HDFS WebUI 端口</span>
    <input
      class="input mono"
      type="number"
      min="1024"
      max="65535"
      value={params.namenode_web_port ?? ""}
      oninput={(event) =>
        onParamChange("namenode_web_port", (event.currentTarget as HTMLInputElement).value)}
    />
  </div>
  <div class="install-field">
    <span class="install-label">YARN WebUI 端口</span>
    <input
      class="input mono"
      type="number"
      min="1024"
      max="65535"
      value={params.yarn_rm_web_port ?? ""}
      oninput={(event) =>
        onParamChange("yarn_rm_web_port", (event.currentTarget as HTMLInputElement).value)}
    />
  </div>
</div>

<div class="install-fields" style="margin-top: 1.2rem;">
  <div class="install-field">
    <span class="install-label">JobHistory</span>
    <Switch
      checked={params.history_enabled === "true"}
      onCheckedChange={(value) => onParamChange("history_enabled", value ? "true" : "false")}
    />
  </div>
  <div class="install-field history-port-field" class:on={params.history_enabled === "true"}>
    <span class="install-label">JobHistory WebUI 端口</span>
    <input
      class="input mono"
      type="number"
      min="1024"
      max="65535"
      value={params.history_web_port ?? ""}
      oninput={(event) =>
        onParamChange("history_web_port", (event.currentTarget as HTMLInputElement).value)}
    />
  </div>
</div>
