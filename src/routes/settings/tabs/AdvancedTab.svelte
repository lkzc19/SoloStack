<script lang="ts">
  // 设置 · 高级：应用诊断日志、通知（弹窗行为与类型）。
  import { invoke } from "@tauri-apps/api/core";
  import { ChevronDown } from "lucide-svelte";
  import { onMount } from "svelte";
  import Select from "$lib/components/ui/select/select.svelte";
  import { store } from "$lib/stores.svelte.ts";
  import { toastConfig, toastSuccess } from "$lib/notifications.svelte.ts";
  import type { SettingsInfo } from "$lib/types";

  let loggingLevel = $state("info");
  let logRetentionDays = $state(7);
  let logMaxTotalMb = $state(1024);
  let notificationTypes = $state<string[]>([]);
  let openSection = $state<string | null>(null);

  const logLevelItems = [
    { value: "error", label: "ERROR" },
    { value: "warn", label: "WARN" },
    { value: "info", label: "INFO" },
    { value: "debug", label: "DEBUG" },
    { value: "trace", label: "TRACE" },
  ];

  onMount(() => {
    void load();
  });

  async function load() {
    try {
      const settings = await invoke<SettingsInfo>("get_settings");
      loggingLevel = settings.log_level;
      logRetentionDays = settings.log_retention_days;
      logMaxTotalMb = settings.log_max_total_mb;
      notificationTypes = settings.notification_types;
    } catch (e) {
      store.errorMsg = String(e);
    }
  }

  async function saveLoggingSettings() {
    try {
      await invoke("set_logging_settings", {
        logLevel: loggingLevel,
        logRetentionDays,
        logMaxTotalMb,
      });
      toastSuccess("日志设置已更新");
    } catch (e) {
      store.errorMsg = String(e);
    }
  }

  async function saveNotificationSettings() {
    try {
      await invoke("set_notification_settings", {
        notificationTypes,
        toastDismissMs: toastConfig.dismissMs,
        toastPosition: toastConfig.position,
      });
      toastSuccess("通知设置已更新");
    } catch (e) {
      store.errorMsg = String(e);
    }
  }

  function toggleNotificationType(type: string) {
    if (notificationTypes.includes(type)) {
      notificationTypes = notificationTypes.filter((t) => t !== type);
    } else {
      notificationTypes = [...notificationTypes, type];
    }
    saveNotificationSettings();
  }

  function toggleSection(section: string) {
    openSection = openSection === section ? null : section;
  }
</script>

<section class="settings-section">
  <div class="settings-accordion-list">
    <div class="settings-accordion" class:open={openSection === "logging"}>
      <button
        class="settings-accordion-header"
        type="button"
        aria-expanded={openSection === "logging"}
        onclick={() => toggleSection("logging")}
      >
        <span class="settings-accordion-copy">
          <span class="settings-field-title">应用诊断日志</span>
          <span class="settings-field-desc">设置日志记录级别、保留期限和容量限制。</span>
        </span>
        <ChevronDown size={16} class="settings-accordion-chevron" />
      </button>
      {#if openSection === "logging"}
        <div class="settings-accordion-content">
          <div class="settings-accordion-field-row">
            <div class="settings-field-copy">
              <div class="settings-accordion-label">日志级别</div>
              <p class="settings-field-desc">设置输出的最低日志级别</p>
            </div>
            <Select
              value={loggingLevel}
              items={logLevelItems}
              class="settings-log-level-select"
              size="sm"
              onSelect={(value) => {
                loggingLevel = value;
                saveLoggingSettings();
              }}
            />
          </div>
          <div class="log-level-help">
            <div class="log-level-help-title">日志级别说明：</div>
            <div class="log-level-help-list">
              <div class="log-level-help-item">
                <code class="log-level-error">ERROR</code>
                <span>仅记录操作失败、脚本失败等严重错误</span>
              </div>
              <div class="log-level-help-item">
                <code class="log-level-warn">WARN</code>
                <span>记录错误、脚本 stderr、取消操作和可恢复异常</span>
              </div>
              <div class="log-level-help-item">
                <code class="log-level-info">INFO</code>
                <span>记录安装、启停、卸载等一般操作信息（发布版默认）</span>
              </div>
              <div class="log-level-help-item">
                <code class="log-level-debug">DEBUG</code>
                <span>记录脚本命令、stdout 和执行细节（开发版默认）</span>
              </div>
              <div class="log-level-help-item">
                <code class="log-level-trace">TRACE</code>
                <span>预留的最详细级别，目前没有业务代码主动写入</span>
              </div>
            </div>
          </div>
          <div class="settings-accordion-inline-fields">
            <label class="settings-inline-field">
              <span class="settings-inline-label">保留天数</span>
              <input
                class="settings-inline-input"
                type="number"
                min="1"
                max="365"
                bind:value={logRetentionDays}
                onchange={saveLoggingSettings}
              />
            </label>
            <label class="settings-inline-field">
              <span class="settings-inline-label">总容量上限</span>
              <span class="settings-inline-number">
                <input
                  type="number"
                  min="10"
                  max="10240"
                  bind:value={logMaxTotalMb}
                  onchange={saveLoggingSettings}
                />
                <span>MB</span>
              </span>
            </label>
          </div>
        </div>
      {/if}
    </div>
    <div class="settings-accordion" class:open={openSection === "notification"}>
      <button
        class="settings-accordion-header"
        type="button"
        aria-expanded={openSection === "notification"}
        onclick={() => toggleSection("notification")}
      >
        <span class="settings-accordion-copy">
          <span class="settings-field-title">通知</span>
          <span class="settings-field-desc">设置通知类型、弹窗行为和显示位置。</span>
        </span>
        <ChevronDown size={16} class="settings-accordion-chevron" />
      </button>
      {#if openSection === "notification"}
        <div class="settings-accordion-content">
          <div class="settings-accordion-field-row">
            <div class="settings-field-copy">
              <div class="settings-accordion-label">自动关闭</div>
              <p class="settings-field-desc">通知弹窗显示时长</p>
            </div>
            <Select
              class="notif-select"
              value={toastConfig.dismissMs === 0 ? "0" : String(toastConfig.dismissMs / 1000)}
              items={[
                { value: "3", label: "3 秒" },
                { value: "5", label: "5 秒" },
                { value: "8", label: "8 秒" },
                { value: "0", label: "不自动关闭" },
              ]}
              size="sm"
              onSelect={(value) => {
                toastConfig.dismissMs = Number(value) * 1000;
                saveNotificationSettings();
              }}
            />
          </div>
          <div class="settings-accordion-field-row">
            <div class="settings-field-copy">
              <div class="settings-accordion-label">弹出位置</div>
              <p class="settings-field-desc">通知弹窗在屏幕上的显示位置</p>
            </div>
            <Select
              class="notif-select"
              value={toastConfig.position}
              items={[
                { value: "top-right", label: "右上" },
                { value: "bottom-left", label: "左下" },
                { value: "bottom-right", label: "右下" },
              ]}
              size="sm"
              onSelect={(value) => {
                toastConfig.position = value;
                saveNotificationSettings();
              }}
            />
          </div>
          <div class="settings-accordion-field-row">
            <div class="settings-field-copy">
              <div class="settings-accordion-label">通知类型</div>
              <p class="settings-field-desc">勾选后将在对应事件发生时弹出通知</p>
            </div>
            <div class="notification-type-toggles">
              <button
                class="notif-type-chip"
                class:active={notificationTypes.includes("component_start")}
                onclick={() => toggleNotificationType("component_start")}
                type="button"
              >
                组件启动完成
              </button>
              <button
                class="notif-type-chip"
                class:active={notificationTypes.includes("component_stop")}
                onclick={() => toggleNotificationType("component_stop")}
                type="button"
              >
                组件停止完成
              </button>
            </div>
          </div>
        </div>
      {/if}
    </div>
  </div>
</section>
