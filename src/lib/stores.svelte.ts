// SoloStack 前端共享状态与操作（module-level runes，跨页面保持）
import { invoke } from "@tauri-apps/api/core";
import { getVersion } from "@tauri-apps/api/app";
import { goto } from "$app/navigation";
import { page } from "$app/state";
import { validateInstalledAdapters } from "./component-adapters/registry";
import { pushNotification } from "./notifications.svelte.ts";
import type {
  ComponentInfo,
  ComponentStatusInfo,
  EnvironmentInfo,
  InstallProgressPayload,
  Status,
  SettingsInfo,
  ThemeMode,
  UiComponent,
} from "./types";

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
  successMsg: "",

  installing: false,
  installLines: [] as string[],
  installPct: 0,
  installDone: false,
  installFailed: false,
  installComponent: "",
  installVersion: "",
});

const statusText: Record<Status, string> = {
  running: "运行中",
  stopped: "已停止",
  partial: "部分运行",
  error: "异常",
  not_installed: "未安装",
};

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

export function flashSuccess(msg: string) {
  store.successMsg = msg;
  setTimeout(() => {
    if (store.successMsg === msg) store.successMsg = "";
  }, 4200);
}

// ── 外观主题（浅 / 深 / 跟随系统）──────────────────────
// 用 localStorage 持久化；class 策略：给 <html> 挂 .dark（app.css 变量随 html.dark 切换）。
// bits-ui 弹层等会 teleport 到 body，而 token 已挂在 :root，故弹层也能正确取色。
const THEME_KEY = "solostack.theme";
let themeQuery: MediaQueryList | null = null;
let themeListener: ((e: MediaQueryListEvent) => void) | null = null;

const systemPrefersDark = () =>
  typeof window !== "undefined" && window.matchMedia("(prefers-color-scheme: dark)").matches;

/** 当前主题选择（跨页共享；设置页三选一高亮）。 */
export const themeState = $state<{ mode: ThemeMode }>({ mode: "system" });

/** 按当前 mode 给 <html> 挂/摘 .dark；跟随系统时监听系统外观变化。 */
function syncThemeClass() {
  if (typeof document === "undefined") return;
  const dark = themeState.mode === "dark" || (themeState.mode === "system" && systemPrefersDark());
  document.documentElement.classList.toggle("dark", dark);

  const follow = themeState.mode === "system";
  themeQuery ??= window.matchMedia("(prefers-color-scheme: dark)");
  if (follow && !themeListener) {
    themeListener = (e) => document.documentElement.classList.toggle("dark", e.matches);
    themeQuery.addEventListener("change", themeListener);
  } else if (!follow && themeListener && themeQuery) {
    themeQuery.removeEventListener("change", themeListener);
    themeListener = null;
  }
}

/** 启动时调用：读取持久化选择并应用（app.html 内联脚本已保证首帧不闪）。 */
export function initTheme() {
  let saved: string | null = null;
  try {
    saved = localStorage.getItem(THEME_KEY);
  } catch {
    /* 存储不可用时用默认 system */
  }
  themeState.mode =
    saved === "light" || saved === "dark" || saved === "system" ? saved : "system";
  syncThemeClass();
}

/** 设置主题并持久化。 */
export function setThemeMode(mode: ThemeMode) {
  themeState.mode = mode;
  try {
    localStorage.setItem(THEME_KEY, mode);
  } catch {
    /* 忽略存储失败 */
  }
  syncThemeClass();
}

// ── 启动 / 加载 ───────────────────────────────────────
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

export async function refreshComponents() {
  try {
    if (!store.activeEnvironmentId) await loadEnvironments();
    if (!store.activeEnvironmentId) return;
    const list = await invoke<ComponentInfo[]>("list_component_templates", {
      environmentId: store.activeEnvironmentId,
    });
    await loadComponents(store.activeEnvironmentId, list);
  } catch {
    /* 轮询失败静默，避免打断操作 */
  }
}

export async function loadEnvironments() {
  store.environments = await invoke<EnvironmentInfo[]>("list_environments");
  const active = store.environments.find((environment) => environment.active);
  store.activeEnvironmentId = active?.id ?? "";
  store.activeEnvironmentName = active?.name ?? "";
}

export async function switchEnvironment(id: string) {
  const environment = store.environments.find((item) => item.id === id);
  if (!environment || environment.active || store.switchingEnvironmentId) return;
  store.switchingEnvironmentId = id;
  store.errorMsg = "";
  try {
    await invoke("switch_environment", { id });
    await loadMain();
    flashSuccess(`已切换到 ${environment.name}`);
  } catch (error) {
    const msg = String(error);
    store.errorMsg = msg;
  } finally {
    store.switchingEnvironmentId = "";
  }
}

export async function createEnvironment(name: string) {
  await invoke("create_environment", { name });
  await loadEnvironments();
}

export async function renameEnvironment(id: string, name: string) {
  await invoke("rename_environment", { id, name });
  await loadEnvironments();
}

export async function deleteEnvironment(id: string) {
  await invoke("delete_environment", { id });
  await loadEnvironments();
}

function sleep(ms: number) {
  return new Promise((r) => setTimeout(r, ms));
}

/// 轮询等待组件状态变为 target（running / stopped），直到超时。返回是否等到目标状态。
async function waitForStatus(name: string, target: "running" | "stopped", timeoutMs: number): Promise<boolean> {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    await refreshComponents();
    const comp = store.components.find((c) => c.name === name);
    const status = comp?.status;
    if (target === "running" && status === "running") return true;
    if (target === "stopped" && (!comp || status === "stopped" || status === "not_installed")) return true;
    await sleep(1000);
  }
  return false;
}

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
      return { ...c, status, statusText: statusText[status] };
    })
  );
  if (store.activeEnvironmentId === environmentId) {
    store.components = withStatus;
  }
}

export function normalizeStatus(s: string): Status {
  if (s === "running" || s === "stopped" || s === "partial") return s;
  return "error";
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
    if (store.installLines.length > 0 && store.installLines[store.installLines.length - 1].startsWith("下载中")) {
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
export interface InstallParams {
  environmentId: string;
  component: string;
  version: string;
  sourceId: string;
  jdkVersion: string;
  /// 组件自定义安装参数（id → 字符串值）；留空的项由组件回退默认值
  params: Record<string, string> | null;
}

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
      flashSuccess("已终止安装");
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
  flashSuccess(`${store.installComponent} v${store.installVersion} 安装完成`);
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

// ── 组件启停 / 卸载 ──────────────────────────────────
export async function startComponent(name: string) {
  if (store.busy) return;
  store.busy = true;
  store.busyAction = "start";
  store.busyComponent = name;
  store.errorMsg = "";
  store.successMsg = "";
  try {
    await invoke("start_component", {
      environmentId: store.activeEnvironmentId,
      component: name,
      service: null,
    });
    // 等端口就绪，期间按钮显示「启动中…」
    const ok = await waitForStatus(name, "running", 60000);
    if (ok) {
      flashSuccess("启动命令已执行");
      pushNotification("info", "组件启动完成", `[${store.activeEnvironmentName}] ${name} 已成功启动`, "component_start");
    } else {
      flashSuccess("启动命令已执行，进程仍在拉起中，稍后自动刷新");
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
  store.successMsg = "";
  try {
    await invoke("stop_component", {
      environmentId: store.activeEnvironmentId,
      component: name,
      service: null,
    });
    // 等进程退出，期间按钮显示「停止中…」
    const ok = await waitForStatus(name, "stopped", 60000);
    if (ok) {
      flashSuccess("停止命令已执行");
      pushNotification("info", "组件停止完成", `[${store.activeEnvironmentName}] ${name} 已成功停止`, "component_stop");
    } else {
      flashSuccess("停止命令已执行，进程仍在退出中，稍后自动刷新");
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

export async function doUninstall(name: string, version: string, keepData: boolean) {
  if (store.busy) return;
  store.busy = true;
  store.busyAction = "uninstall";
  store.busyComponent = name;
  store.errorMsg = "";
  store.successMsg = "";
  try {
    await invoke("uninstall_component", {
      environmentId: store.activeEnvironmentId,
      component: name,
      keepData,
    });
    flashSuccess(`${name} v${version} 已卸载`);
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
