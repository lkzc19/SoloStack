use serde::{Deserialize, Serialize};

use crate::app::paths;

/// 用户设置。
///
/// 配置按领域分组，日志相关字段统一放在 `log` 下。旧版顶层的
/// `log_viewer` / `log_level` 等字段仍可读取，下一次保存会迁移为新结构。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Settings {
    /// 点击关闭按钮时是否仅隐藏窗口并保留托盘进程。
    pub close_to_tray: bool,
    /// 是否在主页顶部显示实时日志入口。
    pub show_logs_button: bool,
    /// 是否在主页顶部显示通知入口。
    pub show_notifications_button: bool,
    /// 环境相关设置。
    pub environment: EnvironmentSettings,
    /// 应用诊断日志配置。
    pub log: LogSettings,
    /// 通知配置。
    pub notification: NotificationSettings,
}

/// 环境设置。
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct EnvironmentSettings {
    /// 当前活动环境 ID。首次初始化前可以为空。
    #[serde(default)]
    pub active_id: Option<String>,
}

/// 日志查看与持久化配置。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LogSettings {
    /// 打开日志文件的默认应用 Bundle ID。
    #[serde(default = "default_log_viewer")]
    pub viewer: String,
    /// 日志最低级别：error / warn / info / debug / trace。
    #[serde(default = "default_log_level")]
    pub level: String,
    /// 日志保留天数。
    #[serde(default = "default_log_retention_days")]
    pub retention_days: u32,
    /// 日志目录最大占用空间（MB）。
    #[serde(default = "default_log_max_total_mb")]
    pub max_total_mb: u64,
}


/// 通知配置。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NotificationSettings {
    /// 启用的通知类型：component_start, component_stop 等。
    #[serde(default = "default_notification_types")]
    pub types: Vec<String>,
    /// toast 自动关闭时间（毫秒）。
    #[serde(default = "default_toast_dismiss_ms")]
    pub toast_dismiss_ms: u64,
    /// toast 弹出位置：top-right, bottom-left, bottom-right。
    #[serde(default = "default_toast_position")]
    pub toast_position: String,
}

impl Default for NotificationSettings {
    fn default() -> Self {
        Self {
            types: default_notification_types(),
            toast_dismiss_ms: default_toast_dismiss_ms(),
            toast_position: default_toast_position(),
        }
    }
}

fn default_notification_types() -> Vec<String> {
    vec![]
}

fn default_toast_dismiss_ms() -> u64 {
    5000
}

fn default_toast_position() -> String {
    "top-right".to_string()
}

impl Default for LogSettings {
    fn default() -> Self {
        Self {
            viewer: default_log_viewer(),
            level: default_log_level(),
            retention_days: default_log_retention_days(),
            max_total_mb: default_log_max_total_mb(),
        }
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            close_to_tray: default_close_to_tray(),
            show_logs_button: default_show_logs_button(),
            show_notifications_button: default_show_notifications_button(),
            environment: EnvironmentSettings::default(),
            log: LogSettings::default(),
            notification: NotificationSettings::default(),
        }
    }
}

/// 兼容旧版扁平字段的读取结构。
#[derive(Deserialize)]
struct SettingsWire {
    #[serde(default = "default_close_to_tray")]
    close_to_tray: bool,
    #[serde(default = "default_show_logs_button")]
    show_logs_button: bool,
    #[serde(default = "default_show_notifications_button")]
    show_notifications_button: bool,
    #[serde(default)]
    environment: Option<EnvironmentSettings>,
    #[serde(default)]
    log: Option<LogSettings>,
    #[serde(default)]
    notification: Option<NotificationSettings>,
    #[serde(default)]
    log_viewer: Option<String>,
    #[serde(default)]
    log_level: Option<String>,
    #[serde(default)]
    log_retention_days: Option<u32>,
    #[serde(default)]
    log_max_total_mb: Option<u64>,
}

impl<'de> Deserialize<'de> for Settings {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let wire = SettingsWire::deserialize(deserializer)?;
        let has_nested_log = wire.log.is_some();
        let mut log = wire.log.unwrap_or_default();

        // 只有旧格式才用顶层字段覆盖默认值；一旦已有 log，以嵌套结构为准。
        if !has_nested_log {
            if let Some(viewer) = wire.log_viewer {
                log.viewer = viewer;
            }
            if let Some(level) = wire.log_level {
                log.level = level;
            }
            if let Some(days) = wire.log_retention_days {
                log.retention_days = days;
            }
            if let Some(max_total_mb) = wire.log_max_total_mb {
                log.max_total_mb = max_total_mb;
            }
        }

        Ok(Self {
            close_to_tray: wire.close_to_tray,
            show_logs_button: wire.show_logs_button,
            show_notifications_button: wire.show_notifications_button,
            environment: wire.environment.unwrap_or_default(),
            log,
            notification: wire.notification.unwrap_or_default(),
        })
    }
}

fn default_log_viewer() -> String {
    "com.apple.TextEdit".to_string()
}

fn default_close_to_tray() -> bool {
    true
}

fn default_show_logs_button() -> bool {
    true
}

fn default_show_notifications_button() -> bool {
    false
}

fn default_log_level() -> String {
    if cfg!(debug_assertions) {
        "debug".to_string()
    } else {
        "info".to_string()
    }
}

fn default_log_retention_days() -> u32 {
    7
}

fn default_log_max_total_mb() -> u64 {
    1024
}

impl Settings {
    /// 从 `~/.solostack/app/settings.json` 读取；文件不存在或为空时返回默认设置。
    pub fn load() -> Result<Settings, std::io::Error> {
        let path = paths::settings_file()?;
        if !path.exists() {
            return Ok(Settings::default());
        }
        let content = std::fs::read_to_string(&path)?;
        if content.trim().is_empty() {
            return Ok(Settings::default());
        }
        serde_json::from_str(&content)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    }

    /// 写入 `~/.solostack/app/settings.json`。
    pub fn save(&self) -> Result<(), std::io::Error> {
        paths::ensure_dirs()?;
        let path = paths::settings_file()?;
        let content = serde_json::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        std::fs::write(path, content)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_settings_has_textedit() {
        let s = Settings::default();
        assert_eq!(s.log.viewer, "com.apple.TextEdit");
        assert!(s.close_to_tray, "默认关闭窗口时最小化到托盘");
        assert!(s.show_logs_button, "默认显示实时日志入口");
        assert!(!s.show_notifications_button, "默认隐藏通知入口");
        assert_eq!(s.log.level, default_log_level());
        assert_eq!(s.log.retention_days, 7);
        assert_eq!(s.log.max_total_mb, 1024);
    }

    #[test]
    fn save_then_load_roundtrips_nested_log_settings() {
        use crate::test_util::HOME_LOCK;
        let _guard = HOME_LOCK.lock().unwrap();
        let tmp = std::env::temp_dir().join("solostack-settings-roundtrip");
        std::env::set_var("HOME", &tmp);
        let _ = std::fs::remove_dir_all(&tmp);

        let s = Settings {
            close_to_tray: false,
            show_logs_button: false,
            show_notifications_button: false,
            environment: EnvironmentSettings {
                active_id: Some("environment-id".to_string()),
            },
            log: LogSettings {
                viewer: "com.sublimetext.4".into(),
                level: "debug".into(),
                retention_days: 30,
                max_total_mb: 500,
            },
            notification: NotificationSettings::default(),
        };
        s.save().unwrap();

        let loaded = Settings::load().unwrap();
        assert_eq!(loaded, s);
        let raw = std::fs::read_to_string(tmp.join(".solostack/app/settings.json")).unwrap();
        assert!(raw.contains(r#""log""#), "日志配置应位于 log 对象下");
        assert!(
            !raw.contains(r#""log_level""#),
            "保存后不应继续写旧版扁平字段"
        );

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn load_missing_returns_default() {
        use crate::test_util::HOME_LOCK;
        let _guard = HOME_LOCK.lock().unwrap();
        let tmp = std::env::temp_dir().join("solostack-settings-missing");
        std::env::set_var("HOME", &tmp);
        let _ = std::fs::remove_dir_all(&tmp);

        let s = Settings::load().unwrap();
        assert_eq!(s, Settings::default());

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn load_legacy_flat_settings() {
        use crate::test_util::HOME_LOCK;
        let _guard = HOME_LOCK.lock().unwrap();
        let tmp = std::env::temp_dir().join("solostack-settings-legacy-flat");
        std::env::set_var("HOME", &tmp);
        let _ = std::fs::remove_dir_all(&tmp);

        let path = paths::settings_file().unwrap();
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(
            &path,
            r#"{
                "log_viewer": "com.sublimetext.4",
                "close_to_tray": false,
                "log_level": "trace",
                "log_retention_days": 30,
                "log_max_total_mb": 500
            }"#,
        )
        .unwrap();

        let settings = Settings::load().unwrap();
        assert!(!settings.close_to_tray);
        assert_eq!(settings.log.viewer, "com.sublimetext.4");
        assert_eq!(settings.log.level, "trace");
        assert_eq!(settings.log.retention_days, 30);
        assert_eq!(settings.log.max_total_mb, 500);

        let _ = std::fs::remove_dir_all(&tmp);
    }
}
