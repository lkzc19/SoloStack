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

use commands::{app, component, install, logs};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|_app| {
            let _ = solostack_core::app::paths::migrate_legacy_layout();
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            app::get_arch,
            app::get_root_dir,
            app::list_jdk_versions,
            app::get_app_logs,
            app::list_log_dates,
            app::get_settings,
            app::list_apps,
            app::set_log_viewer,
            app::open_log_file,
            component::list_component_manifests,
            component::list_config_fields,
            component::set_config_field,
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
            logs::read_component_log_tail
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
