<script lang="ts">
  import Select from "$lib/components/ui/select/select.svelte";
  import Switch from "$lib/components/ui/switch/switch.svelte";
  import type { ConfigFieldsProps } from "$lib/types";

  let { values, onFieldChange }: ConfigFieldsProps = $props();

  const messageMaxItems = ["1", "10", "50", "100", "500"].map((value) => ({
    value,
    label: value === "1" ? "1 MB（默认）" : `${value} MB`,
  }));
</script>

<div class="field-pair">
  <div class="field-item">
    <span class="field-label">Broker 端口</span>
    <input
      class="input mono"
      type="number"
      min="1024"
      max="65535"
      value={values.broker_port ?? ""}
      oninput={(event) =>
        onFieldChange("broker_port", (event.currentTarget as HTMLInputElement).value)}
    />
  </div>
  <div class="field-item">
    <span class="field-label">默认分区数</span>
    <input
      class="input mono"
      type="number"
      value={values.num_partitions ?? ""}
      oninput={(event) =>
        onFieldChange("num_partitions", (event.currentTarget as HTMLInputElement).value)}
    />
  </div>
</div>

<div class="field-pair">
  <div class="field-item">
    <span class="field-label">消息保留时长（小时）</span>
    <input
      class="input mono"
      type="number"
      value={values.retention_hours ?? ""}
      oninput={(event) =>
        onFieldChange("retention_hours", (event.currentTarget as HTMLInputElement).value)}
    />
  </div>
  <div class="field-item">
    <span class="field-label">单条消息上限</span>
    <Select
      class="full field-select"
      value={values.message_max_mb ?? "1"}
      items={messageMaxItems}
      onSelect={(value) => onFieldChange("message_max_mb", value)}
    />
  </div>
</div>

<div class="field-item">
  <span class="field-label">自动创建 Topic</span>
  <Switch
    checked={values.auto_create_topics === "true"}
    onCheckedChange={(value) => onFieldChange("auto_create_topics", value ? "true" : "false")}
  />
</div>
