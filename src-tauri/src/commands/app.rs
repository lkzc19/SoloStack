//! 应用与设置：架构、数据根目录、用户设置、日志查看器、app 操作日志、本机 JDK。

use solostack_core::app::{app_log, paths};
use solostack_core::platform::{app_scan, arch, jdk};

#[tauri::command]
pub fn get_arch() -> String {
    arch::Arch::current().to_string()
}

#[tauri::command]
pub fn get_root_dir() -> Result<String, String> {
    paths::root_dir()
        .map(|p| p.display().to_string())
        .map_err(|e| e.to_string())
}

/// 当前设置（配置页回显）。
#[tauri::command]
pub fn get_settings() -> Result<SettingsInfo, String> {
    let root = paths::root_dir().map_err(|e| e.to_string())?;
    let settings = solostack_core::app::settings::Settings::load().map_err(|e| e.to_string())?;
    Ok(SettingsInfo {
        data_root: root.display().to_string(),
        log_viewer: settings.log_viewer,
        close_to_tray: settings.close_to_tray,
    })
}

/// 白名单里已安装的编辑器（日志查看器候选）。
#[tauri::command]
pub fn list_apps() -> Vec<app_scan::AppDef> {
    app_scan::list_available_apps()
}

/// 设置打开日志文件的默认应用（存 Bundle ID）。
#[tauri::command]
pub fn set_log_viewer(app: String) -> Result<(), String> {
    let mut settings =
        solostack_core::app::settings::Settings::load().map_err(|e| e.to_string())?;
    settings.log_viewer = app;
    settings.save().map_err(|e| e.to_string())
}

/// 设置点击关闭按钮时是否最小化到托盘。
#[tauri::command]
pub fn set_close_to_tray(close_to_tray: bool) -> Result<(), String> {
    let mut settings =
        solostack_core::app::settings::Settings::load().map_err(|e| e.to_string())?;
    settings.close_to_tray = close_to_tray;
    settings.save().map_err(|e| e.to_string())
}

/// 用设置的默认应用（Bundle ID）打开日志文件。
///
/// 所选应用不可用（如已卸载）时，自动降级为系统默认应用打开（`open <path>`）。
#[tauri::command]
pub fn open_log_file(path: String) -> Result<(), String> {
    let settings = solostack_core::app::settings::Settings::load().map_err(|e| e.to_string())?;
    if !settings.log_viewer.is_empty() {
        let ok = std::process::Command::new("open")
            .args(["-b", &settings.log_viewer, &path])
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if ok {
            return Ok(());
        }
    }
    // 降级：系统默认应用打开
    std::process::Command::new("open")
        .arg(&path)
        .status()
        .map_err(|e| format!("打开日志失败: {e}"))?
        .success()
        .then_some(())
        .ok_or_else(|| "无法打开日志：所选编辑器不可用，且系统没有默认处理程序".to_string())
}

/// 读取 app 操作日志：默认今天的完整日志；`date` 传 `YYYY-MM-DD` 看历史某天。
#[tauri::command]
pub fn get_app_logs(date: Option<String>) -> Result<String, String> {
    let date = date.unwrap_or_else(app_log::today);
    app_log::read_logs_for(&date)
}

/// 列出 app 日志可用日期（倒序，新的在前）。
#[tauri::command]
pub fn list_log_dates() -> Result<Vec<String>, String> {
    app_log::list_log_dates()
}

/// 本机已安装的 JDK 列表（安装页 / 配置页 JDK 下拉，含发行商 + 版本）。
#[tauri::command]
pub fn list_jdk_versions() -> Vec<JdkInfo> {
    jdk::scan()
        .into_iter()
        .map(|j| JdkInfo {
            name: j.name,
            vendor: j.vendor,
            version: j.version,
        })
        .collect()
}

/// 当前设置（GUI 回显）。
#[derive(serde::Serialize)]
pub struct SettingsInfo {
    data_root: String,
    log_viewer: String,
    close_to_tray: bool,
}

/// 本机 JDK（GUI 展示）。
#[derive(serde::Serialize)]
pub struct JdkInfo {
    name: String,
    vendor: String,
    version: String,
}
