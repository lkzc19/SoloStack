// 安装 / 卸载：进度事件、发起安装、完成确认、取消、卸载。

import { invoke } from "@tauri-apps/api/core";
import { goto } from "$app/navigation";
import { page } from "$app/state";
import { toastSuccess } from "../notifications.svelte.ts";
import { refreshComponents } from "./component.svelte.ts";
import { loadMain } from "./main.svelte.ts";
import { fmtBytes, store } from "./state.svelte.ts";
import type { InstallProgressPayload } from "../types";

export interface InstallParams {
  environmentId: string;
  component: string;
  version: string;
  sourceId: string;
  jdkVersion: string;
  /// 组件自定义安装参数（id → 字符串值）；留空的项由组件回退默认值
  params: Record<string, string> | null;
}

// ── 安装进度事件 ─────────────────────────────────────
export function handleInstallProgress(p: InstallProgressPayload) {
  if (p.phase === "checking") {
    store.installPct = 0;
    store.installLines = [
      ...store.installLines,
      p.cached ? "检查包：发现缓存，将校验 SHA256" : "检查包：开始下载",
    ];
  } else if (p.phase === "downloading") {
    const realPct = p.total > 0 ? Math.round((p.bytes / p.total) * 100) : 0;
    store.installPct = p.total > 0 ? Math.round((p.bytes / p.total) * 80) : 0;
    const line =
      p.total > 0
        ? `下载中 ${fmtBytes(p.bytes)} / ${fmtBytes(p.total)} (${realPct}%)`
        : `下载中 ${fmtBytes(p.bytes)}`;
    if (
      store.installLines.length > 0 &&
      store.installLines[store.installLines.length - 1].startsWith("下载中")
    ) {
      store.installLines[store.installLines.length - 1] = line;
    } else {
      store.installLines = [...store.installLines, line];
    }
  } else if (p.phase === "extracting") {
    store.installPct = 85;
    store.installLines = [...store.installLines, "开始解压…"];
  } else if (p.phase === "configuring") {
    store.installPct = 95;
    store.installLines = [...store.installLines, "开始配置…"];
  } else if (p.phase === "done") {
    store.installPct = 100;
    store.installLines = [...store.installLines, "安装完成"];
  }
}

// ── 安装操作 ─────────────────────────────────────────
export async function doInstall(params: InstallParams) {
  if (store.installing) return;
  store.installing = true;
  store.installDone = false;
  store.installFailed = false;
  store.errorMsg = "";
  store.installLines = [];
  store.installPct = 0;
  store.installComponent = params.component;
  store.installVersion = params.version;
  try {
    await invoke("install_component", {
      environmentId: params.environmentId,
      component: params.component,
      version: params.version,
      sourceId: params.sourceId,
      jdkVersion: params.jdkVersion,
      params: params.params,
    });
    store.installDone = true;
    await refreshComponents();
    // 若用户已切回主页（后台安装），则无需停留在「完成」确认态
    if (page.url.pathname !== "/install/progress") {
      store.installDone = false;
    }
  } catch (e) {
    const msg = String(e);
    if (msg.includes("安装已取消")) {
      store.errorMsg = "";
      toastSuccess("已终止安装");
      goto("/install");
    } else {
      store.installFailed = true;
      store.installLines = [...store.installLines, `安装失败：${msg}`];
    }
  } finally {
    store.installing = false;
  }
}

export async function finishInstall() {
  store.installDone = false;
  toastSuccess(`${store.installComponent} v${store.installVersion} 安装完成`);
  await loadMain();
  goto("/");
}

export async function cancelInstall() {
  if (!store.installing) return;
  try {
    await invoke("cancel_install");
    store.installLines = [...store.installLines, "正在终止…"];
  } catch (e) {
    store.errorMsg = String(e);
  }
}

export async function doUninstall(name: string, version: string, keepData: boolean) {
  if (store.busy) return;
  store.busy = true;
  store.busyAction = "uninstall";
  store.busyComponent = name;
  store.errorMsg = "";
  try {
    await invoke("uninstall_component", {
      environmentId: store.activeEnvironmentId,
      component: name,
      keepData,
    });
    toastSuccess(`${name} v${version} 已卸载`);
    await loadMain();
    goto("/");
  } catch (e) {
    store.errorMsg = String(e);
  } finally {
    store.busy = false;
    store.busyAction = "";
    store.busyComponent = "";
  }
}
