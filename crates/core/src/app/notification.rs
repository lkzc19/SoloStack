//! 应用内通知：内存存储，重启清空。
//!
//! 设计为轻量级通知中心，不持久化。适合个人桌面应用的错误/警告反馈。
//! 后续需要持久化时可在此模块内扩展，不影响调用方。

use std::sync::Mutex;

use super::id::new_id;

/// 通知级别。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    Info,
    Warn,
    Error,
}

/// 单条通知。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Notification {
    pub id: String,
    pub level: Level,
    pub title: String,
    pub message: String,
    pub created_at: String,
}

/// 最多保留的通知条数。
const MAX_NOTIFICATIONS: usize = 200;

static NOTIFICATIONS: Mutex<Vec<Notification>> = Mutex::new(Vec::new());

/// 推送一条通知，返回通知 ID。
pub fn push(level: Level, title: &str, message: &str) -> String {
    let id = new_id();
    let notification = Notification {
        id: id.clone(),
        level,
        title: title.to_string(),
        message: message.to_string(),
        created_at: chrono::Local::now().to_rfc3339(),
    };
    let mut list = NOTIFICATIONS.lock().unwrap_or_else(|e| e.into_inner());
    list.push(notification);
    // 超出上限时淘汰最旧的
    if list.len() > MAX_NOTIFICATIONS {
        let excess = list.len() - MAX_NOTIFICATIONS;
        list.drain(..excess);
    }
    id
}

/// 便捷方法。
pub fn info(title: &str, message: &str) -> String {
    push(Level::Info, title, message)
}
pub fn warn(title: &str, message: &str) -> String {
    push(Level::Warn, title, message)
}
pub fn error(title: &str, message: &str) -> String {
    push(Level::Error, title, message)
}

/// 列出全部通知（最新在前）。
pub fn list() -> Vec<Notification> {
    let list = NOTIFICATIONS.lock().unwrap_or_else(|e| e.into_inner());
    list.iter().rev().cloned().collect()
}

/// 通知总数。
pub fn count() -> usize {
    let list = NOTIFICATIONS.lock().unwrap_or_else(|e| e.into_inner());
    list.len()
}

/// 清空全部通知。
pub fn clear() {
    let mut list = NOTIFICATIONS.lock().unwrap_or_else(|e| e.into_inner());
    list.clear();
}

/// 删除单条通知，返回是否成功。
pub fn delete(id: &str) -> bool {
    let mut list = NOTIFICATIONS.lock().unwrap_or_else(|e| e.into_inner());
    let before = list.len();
    list.retain(|n| n.id != id);
    list.len() < before
}
