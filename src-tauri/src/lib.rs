//! Tauri 命令层：把 `solostack_core` 的能力暴露给前端。
//!
//! 本层**不含业务逻辑**：只做参数转换、错误映射，长任务丢进 `spawn_blocking`。
//! 命令按关注点分组在 `commands/` 下：
//!
//! | 模块 | 职责 |
//! |---|---|
//! | `commands::app` | 应用与设置：架构、数据根目录、设置、日志查看器、app 操作日志、本机 JDK |
//! | `commands::component` | 组件查询与启停：列表、状态、启停、WebUI、配置字段、托管目录、组件清单 |
//! | `commands::install` | 安装与卸载：安装（进度事件）、取消、卸载、下载包缓存 |
//! | `commands::logs` | 组件日志：列出、按路径读尾部 |

mod commands;
mod tray;

use commands::{app, component, environment, install, logs};
use tauri::{Manager, WindowEvent};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .setup(|app| {
            if let Err(error) = solostack_core::app::paths::migrate_legacy_layout() {
                let _ = solostack_core::app::app_log::error(
                    "migration.legacy_layout.failed",
                    &format!("旧应用目录迁移失败: {error}"),
                );
            }
            if let Err(error) = solostack_core::app::environment::migrate_legacy_layout() {
                let _ = solostack_core::app::app_log::error(
                    "migration.environment.failed",
                    &format!("环境目录迁移失败: {error}"),
                );
            }
            if let Err(error) = solostack_core::app::environment::ensure_initialized() {
                let _ = solostack_core::app::app_log::error(
                    "migration.environment.initialize_failed",
                    &format!("环境初始化失败: {error}"),
                );
            }
            if let Ok(active_id) = solostack_core::app::environment::active_id() {
                if let Ok(environments) = solostack_core::app::environment::list() {
                    for environment in environments {
                        if Some(environment.id.as_str()) == active_id.as_deref() {
                            continue;
                        }
                        if let Err(error) =
                            solostack_core::lifecycle::environment::stop_all_components(
                                &environment,
                            )
                        {
                            let _ = solostack_core::app::app_log::warn(
                                "environment.startup.stop_failed",
                                &format!(
                                    "启动时停止非活动环境《{}》失败: {error}",
                                    environment.name
                                ),
                            );
                        }
                    }
                }
            }
            tray::setup(app)?;
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                let close_to_tray = solostack_core::app::settings::Settings::load()
                    .map(|settings| settings.close_to_tray)
                    .unwrap_or(true);
                api.prevent_close();
                if close_to_tray {
                    tray::hide_main_window(window.app_handle());
                } else {
                    window.app_handle().exit(0);
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            app::get_arch,
            app::get_root_dir,
            app::list_jdk_versions,
            app::get_app_logs,
            app::list_log_dates,
            app::get_settings,
            app::set_logging_settings,
            app::set_close_to_tray,
            app::list_apps,
            app::set_log_viewer,
            app::open_log_file,
            environment::list_environments,
            environment::get_active_environment,
            environment::list_environment_overviews,
            environment::create_environment,
            environment::rename_environment,
            environment::delete_environment,
            environment::switch_environment,
            component::list_component_manifests,
            component::list_config_fields,
            component::save_config_fields,
            component::list_component_templates,
            component::start_component,
            component::stop_component,
            component::get_component_status,
            component::get_web_ui_urls,
            component::get_component_dirs,
            install::install_component,
            install::list_install_params,
            install::cancel_install,
            install::is_package_downloaded,
            install::list_download_packages,
            install::delete_download_packages,
            install::uninstall_component,
            logs::list_component_logs,
            logs::read_component_log_tail,
            logs::query_app_logs
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
