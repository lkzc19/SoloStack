// 启动与主数据加载 / 环境切换：编排 environment + component 两组操作。

import { invoke } from "@tauri-apps/api/core";
import { getVersion } from "@tauri-apps/api/app";
import { toastSuccess } from "../notifications.svelte.ts";
import { loadComponents } from "./component.svelte.ts";
import { loadEnvironments } from "./environment.svelte.ts";
import { activeEnvironment, store } from "./state.svelte.ts";
import { initTheme } from "./theme.svelte.ts";
import type { ComponentInfo, SettingsInfo } from "../types";

export async function boot() {
  initTheme();
  try {
    const [version, settings] = await Promise.all([
      getVersion(),
      invoke<SettingsInfo>("get_settings"),
    ]);
    store.appVersion = version;
    store.showLogsButton = settings.show_logs_button;
    store.showNotificationsButton = settings.show_notifications_button;
  } catch (e) {
    store.errorMsg = `读取应用信息失败: ${e}`;
  }
  await loadMain();
}

export async function loadMain() {
  try {
    await loadEnvironments();
    const environment = activeEnvironment();
    if (!environment) {
      store.components = [];
      return;
    }
    const [list, root] = await Promise.all([
      invoke<ComponentInfo[]>("list_component_templates", {
        environmentId: environment.id,
      }),
      invoke<string>("get_root_dir"),
    ]);
    store.rootDir = root;
    await loadComponents(environment.id, list);
  } catch (e) {
    store.errorMsg = String(e);
  }
}

export async function switchEnvironment(id: string) {
  const environment = store.environments.find((item) => item.id === id);
  if (!environment || environment.active || store.switchingEnvironmentId) return;
  store.switchingEnvironmentId = id;
  store.errorMsg = "";
  try {
    await invoke("switch_environment", { id });
    await loadMain();
    toastSuccess(`已切换到 ${environment.name}`);
  } catch (error) {
    const msg = String(error);
    store.errorMsg = msg;
  } finally {
    store.switchingEnvironmentId = "";
  }
}
