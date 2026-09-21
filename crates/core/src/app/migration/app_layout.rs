//! 旧版应用级目录布局迁移（幂等）。

use crate::app::paths;

const LEGACY_ETC_DIR: &str = "etc";

/// 迁移旧版应用级布局（幂等）：
/// - 根/旧位置 settings.json → app/settings.json
/// - 旧应用日志 → app/log/
/// - 根 downloads/ 与 var/downloads/ → cache/downloads/
/// - 清理废弃的 installs / etc / snapshots / .templates
pub fn migrate_app_layout() -> Result<(), std::io::Error> {
    paths::ensure_app_dirs()?;
    let root = paths::root_dir()?;

    let legacy_settings = root.join(paths::SETTINGS_FILE);
    if legacy_settings.is_file() {
        let target = paths::settings_file()?;
        let target_blank = std::fs::read_to_string(&target)
            .map(|content| content.trim().is_empty())
            .unwrap_or(true);
        if !target.exists() || target_blank {
            let _ = std::fs::rename(&legacy_settings, &target);
        }
    }

    let legacy_app_logs = root.join(paths::VAR_DIR).join("solostack");
    if legacy_app_logs.is_dir() {
        if let Ok(entries) = std::fs::read_dir(&legacy_app_logs) {
            for entry in entries.flatten() {
                let path = entry.path();
                let destination = paths::app_log_dir()?.join(entry.file_name());
                if path.is_file() && !destination.exists() {
                    let _ = std::fs::rename(&path, destination);
                }
            }
        }
        let _ = std::fs::remove_dir_all(&legacy_app_logs);
    }

    for legacy_downloads in [
        root.join(paths::DOWNLOADS_DIR),
        root.join(paths::VAR_DIR).join(paths::DOWNLOADS_DIR),
    ] {
        if legacy_downloads.is_dir() {
            let destination = paths::downloads_dir()?;
            if let Ok(entries) = std::fs::read_dir(&legacy_downloads) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    let target = destination.join(entry.file_name());
                    if !target.exists() {
                        let _ = std::fs::rename(&path, target);
                    }
                }
            }
            let _ = std::fs::remove_dir_all(&legacy_downloads);
        }
    }

    for dir in [
        root.join("installs"),
        root.join(paths::VAR_DIR).join("installs"),
    ] {
        if dir.is_dir() {
            let _ = std::fs::remove_dir_all(&dir);
        }
    }
    for legacy in [LEGACY_ETC_DIR, "snapshots", ".templates"] {
        let path = root.join(legacy);
        if path.is_dir() {
            let _ = std::fs::remove_dir_all(&path);
        }
    }

    Ok(())
}
