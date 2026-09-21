// 组件列表 / 状态刷新与启停、卸载。

import { invoke } from "@tauri-apps/api/core";
import { validateInstalledAdapters } from "../component-adapters/registry";
import { pushNotification, toastSuccess } from "../notifications.svelte.ts";
import { loadEnvironments } from "./environment.svelte.ts";
import { STATUS_TEXT, normalizeStatus, store } from "./state.svelte.ts";
import type { ComponentInfo, ComponentStatusInfo, Status } from "../types";

export async function loadComponents(environmentId: string, list: ComponentInfo[]) {
  validateInstalledAdapters(list);
  const withStatus = await Promise.all(
    list.map(async (c) => {
      let status: Status = "not_installed";
      if (c.installed) {
        const info = await invoke<ComponentStatusInfo>("get_component_status", {
          environmentId,
          component: c.name,
        });
        status = normalizeStatus(info.status);
      }
      return { ...c, status, statusText: STATUS_TEXT[status] };
    })
  );
  if (store.activeEnvironmentId === environmentId) {
    store.components = withStatus;
  }
}

export async function refreshComponents() {
  try {
    if (!store.activeEnvironmentId) {
      await loadEnvironments();
    }
    if (!store.activeEnvironmentId) return;
    const list = await invoke<ComponentInfo[]>("list_component_templates", {
      environmentId: store.activeEnvironmentId,
    });
    await loadComponents(store.activeEnvironmentId, list);
  } catch {
    /* 轮询失败静默，避免打断操作 */
  }
}

function sleep(ms: number) {
  return new Promise((r) => setTimeout(r, ms));
}

/// 轮询等待组件状态变为 target（running / stopped），直到超时。返回是否等到目标状态。
async function waitForStatus(
  name: string,
  target: "running" | "stopped",
  timeoutMs: number
): Promise<boolean> {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    await refreshComponents();
    const comp = store.components.find((c) => c.name === name);
    const status = comp?.status;
    if (target === "running" && status === "running") return true;
    if (target === "stopped" && (!comp || status === "stopped" || status === "not_installed"))
      return true;
    await sleep(1000);
  }
  return false;
}

export async function startComponent(name: string) {
  if (store.busy) return;
  store.busy = true;
  store.busyAction = "start";
  store.busyComponent = name;
  store.errorMsg = "";
  try {
    await invoke("start_component", {
      environmentId: store.activeEnvironmentId,
      component: name,
      service: null,
    });
    // 等端口就绪，期间按钮显示「启动中…」
    const ok = await waitForStatus(name, "running", 60000);
    if (ok) {
      pushNotification(
        "info",
        "组件启动完成",
        `[${store.activeEnvironmentName}] ${name} 已成功启动`,
        "component_start"
      );
    } else {
      toastSuccess("启动命令已执行，进程仍在拉起中，稍后自动刷新");
    }
  } catch (e) {
    const msg = String(e);
    store.errorMsg = msg;
  } finally {
    store.busy = false;
    store.busyAction = "";
    store.busyComponent = "";
  }
}

export async function stopComponent(name: string) {
  if (store.busy) return;
  store.busy = true;
  store.busyAction = "stop";
  store.busyComponent = name;
  store.errorMsg = "";
  try {
    await invoke("stop_component", {
      environmentId: store.activeEnvironmentId,
      component: name,
      service: null,
    });
    // 等进程退出，期间按钮显示「停止中…」
    const ok = await waitForStatus(name, "stopped", 60000);
    if (ok) {
      pushNotification(
        "info",
        "组件停止完成",
        `[${store.activeEnvironmentName}] ${name} 已成功停止`,
        "component_stop"
      );
    } else {
      toastSuccess("停止命令已执行，进程仍在退出中，稍后自动刷新");
    }
  } catch (e) {
    const msg = String(e);
    store.errorMsg = msg;
  } finally {
    store.busy = false;
    store.busyAction = "";
    store.busyComponent = "";
  }
}
