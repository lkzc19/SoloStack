//! 通知：推送、查询、删除、清空。

use solostack_core::app::notification;
use tauri::Emitter;

/// 列出全部通知（最新在前）。
#[tauri::command]
pub fn list_notifications() -> Vec<notification::Notification> {
    notification::list()
}

/// 通知总数。
#[tauri::command]
pub fn notification_count() -> usize {
    notification::count()
}

/// 清空全部通知。
#[tauri::command]
pub fn clear_notifications() {
    notification::clear();
}

/// 删除单条通知。
#[tauri::command]
pub fn delete_notification(id: String) -> bool {
    notification::delete(&id)
}

/// 前端直接推送一条通知：存储 + 通过事件广播到所有窗口。
#[tauri::command]
pub fn push_notification(app: tauri::AppHandle, level: String, title: String, message: String) -> String {
    let lvl = match level.as_str() {
        "warn" => notification::Level::Warn,
        "error" => notification::Level::Error,
        _ => notification::Level::Info,
    };
    let id = notification::push(lvl, &title, &message);
    // 广播到前端
    if let Some(n) = notification::list().into_iter().find(|n| n.id == id) {
        let _ = app.emit("notification://created", n);
    }
    id
}
