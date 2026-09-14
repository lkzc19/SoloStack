use serde::{Deserialize, Serialize};

use crate::app::paths;

/// 用户设置（下载源已迁至 config json，安装时选择，不再全局配置）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Settings {
    /// 打开日志文件的默认应用 Bundle ID（默认 macOS 自带文本编辑器 TextEdit）。
    #[serde(default = "default_log_viewer")]
    pub log_viewer: String,
    /// 点击关闭按钮时是否仅隐藏窗口并保留托盘进程。
    #[serde(default = "default_close_to_tray")]
    pub close_to_tray: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            log_viewer: default_log_viewer(),
            close_to_tray: default_close_to_tray(),
        }
    }
}

fn default_log_viewer() -> String {
    "com.apple.TextEdit".to_string()
}

fn default_close_to_tray() -> bool {
    true
}

impl Settings {
    /// 从 `~/.solostack/settings.json` 读取；文件不存在或为空时返回默认设置。
    pub fn load() -> Result<Settings, std::io::Error> {
        let path = paths::settings_file()?;
        if !path.exists() {
            return Ok(Settings::default());
        }
        let content = std::fs::read_to_string(&path)?;
        // 空文件（如首次初始化只 touch 了 settings.json）按默认设置处理
        if content.trim().is_empty() {
            return Ok(Settings::default());
        }
        let settings = serde_json::from_str(&content)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        Ok(settings)
    }

    /// 写入 `~/.solostack/app/settings.json`（与 `load` 同一路径，确保目录存在）。
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
        assert_eq!(s.log_viewer, "com.apple.TextEdit");
        assert!(s.close_to_tray, "默认关闭窗口时最小化到托盘");
    }

    /// 存了再读必须拿到同一个值 —— 这条往返路径曾因 save 写根目录、load 读 app/
    /// 而不一致，偏好设置静默失效（既有测试只覆盖了缺失/默认，故漏网）。
    #[test]
    fn save_then_load_roundtrips() {
        use crate::test_util::HOME_LOCK;
        let _guard = HOME_LOCK.lock().unwrap();
        let tmp = std::env::temp_dir().join("solostack-settings-roundtrip");
        std::env::set_var("HOME", &tmp);
        let _ = std::fs::remove_dir_all(&tmp);

        let s = Settings {
            log_viewer: "com.sublimetext.4".into(),
            close_to_tray: false,
        };
        s.save().unwrap();

        let loaded = Settings::load().unwrap();
        assert_eq!(loaded.log_viewer, "com.sublimetext.4");
        assert!(!loaded.close_to_tray);
        assert!(
            tmp.join(".solostack/app/settings.json").is_file(),
            "必须写在 app/（load 读的位置）"
        );
        assert!(
            !tmp.join(".solostack/settings.json").exists(),
            "不应再往根目录写"
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
    }

    #[test]
    fn load_legacy_settings_defaults_close_to_tray() {
        use crate::test_util::HOME_LOCK;
        let _guard = HOME_LOCK.lock().unwrap();
        let tmp = std::env::temp_dir().join("solostack-settings-legacy-tray");
        std::env::set_var("HOME", &tmp);
        let _ = std::fs::remove_dir_all(&tmp);

        let path = paths::settings_file().unwrap();
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, r#"{"log_viewer":"com.apple.TextEdit"}"#).unwrap();

        let settings = Settings::load().unwrap();
        assert!(settings.close_to_tray, "旧设置缺少字段时应默认最小化到托盘");

        let _ = std::fs::remove_dir_all(&tmp);
    }
}
