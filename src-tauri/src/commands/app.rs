//! 应用与设置：架构、数据根目录、用户设置、日志查看器、app 操作日志、本机 JDK。

use solostack_core::app::{app_log, paths};
use solostack_core::logs;
use solostack_core::platform::{app_scan, arch, jdk};

use super::resolve;

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
        log_viewer: settings.log.viewer,
        close_to_tray: settings.close_to_tray,
        show_logs_button: settings.show_logs_button,
        show_notifications_button: settings.show_notifications_button,
        log_level: settings.log.level,
        log_retention_days: settings.log.retention_days,
        log_max_total_mb: settings.log.max_total_mb,
        notification_types: settings.notification.types,
        toast_dismiss_ms: settings.notification.toast_dismiss_ms,
        toast_position: settings.notification.toast_position,
    })
}

/// 保存日志级别、保留期限和容量限制。
#[tauri::command]
pub fn set_logging_settings(
    log_level: String,
    log_retention_days: u32,
    log_max_total_mb: u64,
) -> Result<(), String> {
    if app_log::LogLevel::parse(&log_level).is_none() {
        return Err(format!("不支持的日志级别: {log_level}"));
    }
    if !(1..=365).contains(&log_retention_days) {
        return Err("日志保留天数必须在 1 到 365 之间".to_string());
    }
    if !(10..=10_240).contains(&log_max_total_mb) {
        return Err("日志总容量必须在 10 MB 到 10 GB 之间".to_string());
    }
    let mut settings =
        solostack_core::app::settings::Settings::load().map_err(|e| e.to_string())?;
    settings.log.level = log_level.to_ascii_lowercase();
    settings.log.retention_days = log_retention_days;
    settings.log.max_total_mb = log_max_total_mb;
    settings.save().map_err(|e| e.to_string())
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
    settings.log.viewer = app;
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

/// 设置主页是否显示实时日志入口。
#[tauri::command]
pub fn set_show_logs_button(show_logs_button: bool) -> Result<(), String> {
    let mut settings =
        solostack_core::app::settings::Settings::load().map_err(|e| e.to_string())?;
    settings.show_logs_button = show_logs_button;
    settings.save().map_err(|e| e.to_string())
}

/// 设置主页是否显示通知入口。
#[tauri::command]
pub fn set_show_notifications_button(show_notifications_button: bool) -> Result<(), String> {
    let mut settings =
        solostack_core::app::settings::Settings::load().map_err(|e| e.to_string())?;
    settings.show_notifications_button = show_notifications_button;
    settings.save().map_err(|e| e.to_string())
}

/// 用设置的默认应用（Bundle ID）打开日志文件。
///
/// 所选应用不可用（如已卸载）时，自动降级为系统默认应用打开（`open <path>`）。
#[tauri::command]
pub fn open_log_file(
    environment_id: String,
    component: String,
    path: String,
) -> Result<(), String> {
    let i = resolve(&environment_id, &component)?;
    let path = logs::validated_component_file(
        &i.environment_id,
        &i.name,
        &i.version,
        std::path::Path::new(&path),
    )?
    .display()
    .to_string();
    let settings = solostack_core::app::settings::Settings::load().map_err(|e| e.to_string())?;
    if !settings.log.viewer.is_empty() {
        let ok = std::process::Command::new("open")
            .args(["-b", &settings.log.viewer, &path])
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
    show_logs_button: bool,
    show_notifications_button: bool,
    log_level: String,
    log_retention_days: u32,
    log_max_total_mb: u64,
    notification_types: Vec<String>,
    toast_dismiss_ms: u64,
    toast_position: String,
}


/// 保存通知配置。
#[tauri::command]
pub fn set_notification_settings(
    notification_types: Vec<String>,
    toast_dismiss_ms: u64,
    toast_position: String,
) -> Result<(), String> {
    let valid_positions = ["top-right", "bottom-left", "bottom-right"];
    if !valid_positions.contains(&toast_position.as_str()) {
        return Err(format!("不支持的弹出位置: {toast_position}"));
    }
    if toast_dismiss_ms != 0 && !(1000..=30000).contains(&toast_dismiss_ms) {
        return Err("自动关闭时间必须在 1000 到 30000 毫秒之间".to_string());
    }
    let mut settings = solostack_core::app::settings::Settings::load().map_err(|e| e.to_string())?;
    settings.notification.types = notification_types;
    settings.notification.toast_dismiss_ms = toast_dismiss_ms;
    settings.notification.toast_position = toast_position;
    settings.save().map_err(|e| e.to_string())
}

/// 本机 JDK（GUI 展示）。
#[derive(serde::Serialize)]
pub struct JdkInfo {
    name: String,
    vendor: String,
    version: String,
}
