//! 系统托盘与主窗口显隐。

use tauri::{
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    App, AppHandle, Manager, Runtime,
};

const TRAY_ID: &str = "main";
const MENU_SHOW: &str = "show";
const MENU_QUIT: &str = "quit";

/// 创建常驻托盘图标及菜单。
pub fn setup<R: Runtime>(app: &App<R>) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, MENU_SHOW, "显示应用", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, MENU_QUIT, "退出应用", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &quit])?;

    let mut tray = TrayIconBuilder::with_id(TRAY_ID)
        .tooltip("SoloStack")
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| match event.id().as_ref() {
            MENU_SHOW => show_main_window(app),
            MENU_QUIT => app.exit(0),
            _ => {}
        });

    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }

    tray.build(app)?;
    Ok(())
}

/// 显示并聚焦主窗口；从托盘恢复时同时恢复 Dock 图标。
pub fn show_main_window<R: Runtime>(app: &AppHandle<R>) {
    if let Err(error) = show_main_window_result(app) {
        report_tray_error(&format!("显示应用失败: {error}"));
    }
}

fn show_main_window_result<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
    let mut errors = Vec::new();

    #[cfg(target_os = "macos")]
    if let Err(error) = app.set_activation_policy(tauri::ActivationPolicy::Regular) {
        errors.push(format!("恢复 Dock 图标失败: {error}"));
    }

    match app.get_webview_window("main") {
        Some(window) => {
            if let Err(error) = window.show() {
                errors.push(format!("显示主窗口失败: {error}"));
            }
            if let Err(error) = window.unminimize() {
                errors.push(format!("取消最小化失败: {error}"));
            }
            if let Err(error) = window.set_focus() {
                errors.push(format!("聚焦主窗口失败: {error}"));
            }
        }
        None => errors.push("未找到主窗口".to_string()),
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("；"))
    }
}

/// 隐藏主窗口；macOS 下切换为无 Dock 图标的托盘型应用。
pub fn hide_main_window<R: Runtime>(app: &AppHandle<R>) {
    if let Err(error) = hide_main_window_result(app) {
        report_tray_error(&format!("隐藏主窗口失败: {error}"));
    }
}

fn hide_main_window_result<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
    let mut errors = Vec::new();

    #[cfg(target_os = "macos")]
    if let Err(error) = app.set_activation_policy(tauri::ActivationPolicy::Accessory) {
        errors.push(format!("切换托盘型应用失败: {error}"));
    }

    match app.get_webview_window("main") {
        Some(window) => {
            if let Err(error) = window.hide() {
                errors.push(format!("隐藏主窗口失败: {error}"));
            }
        }
        None => errors.push("未找到主窗口".to_string()),
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("；"))
    }
}

fn report_tray_error(message: &str) {
    eprintln!("{message}");
    let _ = solostack_core::app::app_log::error(message);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tray_errors_are_written_to_app_log() {
        let tmp = std::env::temp_dir().join("solostack-tray-error-log");
        std::env::set_var("HOME", &tmp);
        let _ = std::fs::remove_dir_all(&tmp);

        report_tray_error("测试托盘错误");

        let lines = solostack_core::logs::tail(
            &solostack_core::logs::LogSource::App {
                date: Some(solostack_core::app::app_log::today()),
            },
            20,
        )
        .unwrap();
        assert!(
            lines.iter().any(|line| {
                line.level == Some(solostack_core::app::app_log::LogLevel::Error)
                    && line.message.contains("测试托盘错误")
            }),
            "{lines:?}"
        );

        let _ = std::fs::remove_dir_all(&tmp);
    }
}
