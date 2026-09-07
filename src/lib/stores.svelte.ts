// SoloStack 前端共享状态与操作（module-level runes，跨页面保持）
import { invoke } from "@tauri-apps/api/core";
import { getVersion } from "@tauri-apps/api/app";
import { goto } from "$app/navigation";
import { page } from "$app/state";
import type {
  ComponentInfo,
  ComponentStatusInfo,
  InstallProgressPayload,
  Status,
  ThemeMode,
  UiComponent,
} from "./types";

// 共享状态统一放在一个 $state 对象里（Svelte 5 不允许 reassign 导出的 state，
// 但允许 mutate 其属性；各页面通过 store.xxx 读写）。
export const store = $state({
  components: [] as UiComponent[],
  selectedName: "",
  rootDir: "",
  appVersion: "0.1.0",
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
    store.appVersion = await getVersion();
    await loadMain();
  } catch (e) {
    store.errorMsg = String(e);
  }
}

export async function loadMain() {
  try {
    const [list, root] = await Promise.all([
      invoke<ComponentInfo[]>("list_component_templates"),
      invoke<string>("get_root_dir"),
    ]);
    store.rootDir = root;
    await loadComponents(list);
  } catch (e) {
    store.errorMsg = String(e);
  }
}

export async function refreshComponents() {
  try {
    const list = await invoke<ComponentInfo[]>("list_component_templates");
    await loadComponents(list);
  } catch {
    /* 轮询失败静默，避免打断操作 */
  }
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
    if (target === "running" && (status === "running" || status === "partial")) return true;
    if (target === "stopped" && (!comp || status === "stopped" || status === "not_installed")) return true;
    await sleep(1000);
  }
  return false;
}

export async function loadComponents(list: ComponentInfo[]) {
  const withStatus = await Promise.all(
    list.map(async (c) => {
      let status: Status = "not_installed";
      if (c.installed) {
        const info = await invoke<ComponentStatusInfo>("get_component_status", {
          component: c.name,
        });
        status = normalizeStatus(info.status);
      }
      return { ...c, status, statusText: statusText[status] };
    })
  );
  store.components = withStatus;
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
      p.cached ? "检查包：已存在，跳过下载" : "检查包：不存在，开始下载",
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
  component: string;
  version: string;
  sourceId: string;
  jdkVersion: string;
  ports: { namenode_web: number; yarn_rm: number; history_enabled: boolean; history_web_port: number } | null;
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
      component: params.component,
      version: params.version,
      sourceId: params.sourceId,
      jdkVersion: params.jdkVersion,
      ports: params.ports,
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
    await invoke("start_component", { component: name, service: null });
    // 等端口就绪，期间按钮显示「启动中…」
    const ok = await waitForStatus(name, "running", 60000);
    if (ok) {
      flashSuccess("启动命令已执行");
    } else {
      flashSuccess("启动命令已执行，进程仍在拉起中，稍后自动刷新");
    }
  } catch (e) {
    store.errorMsg = String(e);
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
    await invoke("stop_component", { component: name, service: null });
    // 等进程退出，期间按钮显示「停止中…」
    const ok = await waitForStatus(name, "stopped", 60000);
    if (ok) {
      flashSuccess("停止命令已执行");
    } else {
      flashSuccess("停止命令已执行，进程仍在退出中，稍后自动刷新");
    }
  } catch (e) {
    store.errorMsg = String(e);
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
    await invoke("uninstall_component", { component: name, keepData });
    flashSuccess(`${name} v${version} 已卸载`);
    await refreshComponents();
    goto("/");
  } catch (e) {
    store.errorMsg = String(e);
  } finally {
    store.busy = false;
    store.busyAction = "";
    store.busyComponent = "";
  }
}
