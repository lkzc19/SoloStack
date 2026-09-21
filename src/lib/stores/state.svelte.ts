// 共享状态与纯取值/格式化。
//
// 只放「状态 + 无副作用的读取/格式化」；带副作用的操作按域拆在同目录的
// environment / component / install / main 里，它们都读写这里的 `store`。

import type { EnvironmentInfo, Status, UiComponent } from "../types";

// 共享状态统一放在一个 $state 对象里（Svelte 5 不允许 reassign 导出的 state，
// 但允许 mutate 其属性；各页面通过 store.xxx 读写）。
export const store = $state({
  components: [] as UiComponent[],
  environments: [] as EnvironmentInfo[],
  activeEnvironmentId: "",
  activeEnvironmentName: "",
  switchingEnvironmentId: "",
  selectedName: "",
  rootDir: "",
  appVersion: "",
  showLogsButton: true,
  showNotificationsButton: true,
  busy: false,
  busyAction: "", // 当前操作："start" / "stop" / "uninstall" / ""
  busyComponent: "", // 当前操作的组件名
  errorMsg: "",

  installing: false,
  installLines: [] as string[],
  installPct: 0,
  installDone: false,
  installFailed: false,
  installComponent: "",
  installVersion: "",
});

/** 状态 → 展示文案（唯一一份）。 */
export const STATUS_TEXT: Record<Status, string> = {
  running: "运行中",
  stopped: "已停止",
  partial: "部分运行",
  error: "异常",
  not_installed: "未安装",
};

/** 后端状态串 → 归一化状态（`error:<message>` 等一律归为 error）。 */
export function normalizeStatus(s: string): Status {
  if (s === "running" || s === "stopped" || s === "partial") return s;
  return "error";
}

/** 后端状态串（可能是 `error:<message>`）→ 展示文案。 */
export function statusTextFor(raw: string): string {
  return STATUS_TEXT[normalizeStatus(raw)];
}

// 当前选中的组件（$derived 不能从 module 导出，故导出取值函数）
export function getSelected() {
  return store.components.find((c) => c.name === store.selectedName) ?? null;
}

export function activeEnvironment() {
  return store.environments.find((environment) => environment.active) ?? null;
}

// ── 工具 ──────────────────────────────────────────────
export function fmtBytes(n: number) {
  if (n >= 1024 * 1024 * 1024) return (n / 1024 / 1024 / 1024).toFixed(1) + " GB";
  if (n >= 1024 * 1024) return (n / 1024 / 1024).toFixed(1) + " MB";
  if (n >= 1024) return (n / 1024).toFixed(0) + " KB";
  return n + " B";
}

export const basename = (p: string) => p.split("/").pop() ?? p;
