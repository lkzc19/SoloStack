// 外观主题（浅 / 深 / 跟随系统）。
//
// 用 localStorage 持久化；class 策略：给 <html> 挂 .dark（app.css 变量随 html.dark
// 切换）。bits-ui 弹层等会 teleport 到 body，而 token 已挂在 :root，故弹层也能正确取色。

import type { ThemeMode } from "../types";

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
