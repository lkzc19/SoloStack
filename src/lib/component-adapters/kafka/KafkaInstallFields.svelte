<script lang="ts">
  import Select from "$lib/components/ui/select/select.svelte";
  import Switch from "$lib/components/ui/switch/switch.svelte";
  import type { InstallFieldsProps } from "$lib/types";

  let { params, onParamChange }: InstallFieldsProps = $props();

  const messageMaxItems = ["1", "10", "50", "100", "500"].map((value) => ({
    value,
    label: value === "1" ? "1 MB（默认）" : `${value} MB`,
  }));
</script>

<div class="install-fields" style="margin-top: 1.2rem;">
  <div class="install-field">
    <span class="install-label">Broker 端口</span>
    <input
      class="input mono"
      type="number"
      min="1024"
      max="65535"
      value={params.broker_port ?? ""}
      oninput={(event) =>
        onParamChange("broker_port", (event.currentTarget as HTMLInputElement).value)}
    />
  </div>
  <div class="install-field">
    <span class="install-label">默认分区数</span>
    <input
      class="input mono"
      type="number"
      value={params.num_partitions ?? ""}
      oninput={(event) =>
        onParamChange("num_partitions", (event.currentTarget as HTMLInputElement).value)}
    />
  </div>
</div>

<div class="install-fields" style="margin-top: 1.2rem;">
  <div class="install-field">
    <span class="install-label">消息保留时长（小时）</span>
    <input
      class="input mono"
      type="number"
      value={params.retention_hours ?? ""}
      oninput={(event) =>
        onParamChange("retention_hours", (event.currentTarget as HTMLInputElement).value)}
    />
  </div>
  <div class="install-field">
    <span class="install-label">单条消息上限</span>
    <Select
      class="full field-select"
      value={params.message_max_mb ?? "1"}
      items={messageMaxItems}
      onSelect={(value) => onParamChange("message_max_mb", value)}
    />
  </div>
</div>

<div class="install-fields" style="margin-top: 1.2rem;">
  <div class="install-field">
    <span class="install-label">自动创建 Topic</span>
    <Switch
      checked={params.auto_create_topics === "true"}
      onCheckedChange={(value) => onParamChange("auto_create_topics", value ? "true" : "false")}
    />
  </div>
</div>
