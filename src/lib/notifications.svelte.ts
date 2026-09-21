import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { AppNotification, NotificationLevel, SettingsInfo } from "./types";

/** 通知中心状态 */
export const notifications = $state({
  items: [] as AppNotification[],
  count: 0,

});

/** 是否有未读通知（红点）。用 $state 对象：Svelte 5 不允许 reassign 导出的 state。 */
export const unread = $state({ value: false });


/** toast 队列 */
export const toasts = $state<AppNotification[]>([]);

/** 通知与 toast 配置 */
export const toastConfig = $state({
  position: "top-right",
  enabledTypes: ["component_start", "component_stop"] as string[],
  dismissMs: 5000,
});

const MAX_TOASTS = 5;

let unlistenCreated: (() => void) | undefined;

/** 初始化：拉取历史 + 加载设置 + 监听实时推送 */
export async function initNotifications() {
  try {
    const [items, count, settings] = await Promise.all([
      invoke<AppNotification[]>("list_notifications"),
      invoke<number>("notification_count"),
      invoke<SettingsInfo>("get_settings"),
    ]);
    notifications.items = items;
    notifications.count = count;
    toastConfig.position = settings.toast_position;
    toastConfig.dismissMs = settings.toast_dismiss_ms;
    toastConfig.enabledTypes = settings.notification_types;
  } catch {
    /* 静默 */
  }
  unlistenCreated?.();
  unlistenCreated = await listen<AppNotification>(
    "notification://created",
    (e) => {
      const n = e.payload;
      notifications.items.unshift(n);
      notifications.count++;
      unread.value = true;
      showToast(n);
    }
  );
}

export function cleanupNotifications() {
  unlistenCreated?.();
  unlistenCreated = undefined;
}

/** 进入通知页时标记已读，红点消失 */
export function markSeen() {
  unread.value = false;
}

/** 直接弹一条成功 toast：不入通知中心、不受通知类型开关影响（用于页面操作反馈）。 */
export function toastSuccess(message: string) {
  showToast({
    id: crypto.randomUUID().slice(0, 8),
    level: "info",
    title: message,
    message: "",
    created_at: new Date().toISOString(),
  });
}
/** 推送一条通知（后端 push 后自动通过事件到达前端，此方法供前端直接调用） */
export async function pushNotification(
  level: NotificationLevel,
  title: string,
  message: string,
  type?: string
) {
  if (type && !toastConfig.enabledTypes.includes(type)) {
    return;
  }
  try {
    await invoke("push_notification", { level, title, message });
  } catch {
    const local: AppNotification = {
      id: crypto.randomUUID().slice(0, 8),
      level,
      title,
      message,
      created_at: new Date().toISOString(),
    };
    notifications.items.unshift(local);
    notifications.count++;
    showToast(local);
  }
}

function showToast(n: AppNotification) {
  toasts.push(n);
  if (toasts.length > MAX_TOASTS) toasts.shift();
  if (toastConfig.dismissMs > 0) {
    setTimeout(() => dismissToast(n.id), toastConfig.dismissMs);
  }
}

export function dismissToast(id: string) {
  const idx = toasts.findIndex((t) => t.id === id);
  if (idx !== -1) toasts.splice(idx, 1);
}

export async function clearAll() {
  await invoke("clear_notifications");
  notifications.items = [];
  notifications.count = 0;
}

export async function deleteNotification(id: string) {
  const ok = await invoke<boolean>("delete_notification", { id });
  if (ok) {
    const idx = notifications.items.findIndex((i) => i.id === id);
    if (idx !== -1) {
      notifications.items.splice(idx, 1);
      notifications.count = Math.max(0, notifications.count - 1);
    }
  }
}
