//! 组件查询与启停：已安装列表、运行状态、启停、WebUI 入口、配置字段、托管目录、组件清单。

use solostack_core::component::{self, instances, registry, schema, ConfigFieldUpdate};
use solostack_core::lifecycle::{lock, service};
use solostack_core::package::manifest;

use super::resolve;

/// 列出已安装的组件（从 components/ 目录发现）。
#[tauri::command]
pub fn list_component_templates(environment_id: String) -> Result<Vec<ComponentInfo>, String> {
    Ok(instances::list_installed(&environment_id)
        .into_iter()
        .map(|i| ComponentInfo {
            name: i.name,
            version: i.version,
            display_name: i.display_name,
            installed: true,
        })
        .collect())
}

/// 组件运行状态。
#[tauri::command]
pub async fn get_component_status(
    environment_id: String,
    component: String,
) -> Result<ComponentStatusInfo, String> {
    let i = resolve(&environment_id, &component)?;
    let status = tauri::async_runtime::spawn_blocking(move || {
        service::component_status(&i.environment_id, &i.name, &i.version)
    })
    .await
    .map_err(|e| e.to_string())?;
    Ok(ComponentStatusInfo {
        status: status.to_wire(),
    })
}

/// 启动组件（全启）。后台线程执行，避免格式化/启停序列阻塞 UI。
#[tauri::command]
pub async fn start_component(
    environment_id: String,
    component: String,
    _service: Option<String>,
) -> Result<(), String> {
    let i = resolve(&environment_id, &component)?;
    let guard = lock::begin_environment_operation(&environment_id)?;
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = guard;
        service::start(&i.environment_id, &i.name, &i.version)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// 停止组件（全停）。后台线程执行，避免启停序列阻塞 UI。
#[tauri::command]
pub async fn stop_component(
    environment_id: String,
    component: String,
    _service: Option<String>,
) -> Result<(), String> {
    let i = resolve(&environment_id, &component)?;
    let guard = lock::begin_environment_operation(&environment_id)?;
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = guard;
        service::stop(&i.environment_id, &i.name, &i.version)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// 获取组件的 WebUI 跳转地址（由组件的 Runtime::web_uis 提供）。
#[tauri::command]
pub fn get_web_ui_urls(
    environment_id: String,
    component: String,
) -> Result<Vec<WebUiInfo>, String> {
    let i = resolve(&environment_id, &component)?;
    let urls = registry::by_component(&i.name)
        .map(|c| c.web_uis(&i.environment_id, &i.version))
        .unwrap_or_default()
        .into_iter()
        .map(|w| WebUiInfo {
            name: w.name,
            url: w.url,
        })
        .collect();
    Ok(urls)
}

/// 获取组件的语义化配置字段列表（含当前值，供 GUI 表单渲染）。
///
/// 打开配置页属 `ensure_config` 的触发时机（见 docs/Config-File-Design.md §6.2）：
/// 先校验/补齐配置，缺失的受管文件会直接报错，而不是让用户对着默认值改一个
/// 并不存在的配置。
#[tauri::command]
pub fn list_config_fields(
    environment_id: String,
    component: String,
) -> Result<Vec<schema::ConfigFieldValue>, String> {
    let i = resolve(&environment_id, &component)?;
    let _guard = lock::begin_environment_operation(&environment_id)?;
    component::config_io::prepare_config(&i.environment_id, &i.name, &i.version)?;
    schema::list_fields(&i.environment_id, &i.name, &i.version)
}

/// 一次保存组件的整组语义化配置字段。
#[tauri::command]
pub fn save_config_fields(
    environment_id: String,
    component: String,
    updates: Vec<ConfigFieldUpdate>,
) -> Result<(), String> {
    let i = resolve(&environment_id, &component)?;
    let _guard = lock::begin_environment_operation(&environment_id)?;
    schema::save_fields(&i.environment_id, &i.name, &i.version, &updates)
}

/// 获取组件各托管目录路径。
#[tauri::command]
pub fn get_component_dirs(
    environment_id: String,
    component: String,
) -> Result<ComponentDirs, String> {
    let i = resolve(&environment_id, &component)?;
    let to_str = |r: Result<std::path::PathBuf, std::io::Error>| {
        r.map(|p| p.display().to_string())
            .map_err(|e| e.to_string())
    };
    let log =
        solostack_core::app::paths::var_log_instance_dir(&i.environment_id, &i.name, &i.version)
            .map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&log).map_err(|e| format!("创建日志目录失败: {e}"))?;
    Ok(ComponentDirs {
        instance: to_str(solostack_core::app::paths::instance_dir(
            &i.environment_id,
            &i.name,
            &i.version,
        ))?,
        config: component::config_io::config_dir(&i.environment_id, &i.name, &i.version)?
            .display()
            .to_string(),
        data: to_str(solostack_core::app::paths::var_data_instance_dir(
            &i.environment_id,
            &i.name,
            &i.version,
        ))?,
        log: log.display().to_string(),
    })
}

/// 组件清单列表（安装页源/版本/JDK 数据来源，来自 manifest json）。
#[tauri::command]
pub fn list_component_manifests() -> Result<Vec<ManifestInfo>, String> {
    Ok(manifest::load_all()?
        .into_iter()
        .map(|m| ManifestInfo {
            component: m.component,
            source: m
                .source
                .into_iter()
                .map(|s| SourceDefInfo {
                    name: s.name,
                    versions: s
                        .version
                        .into_iter()
                        .map(|(version, artifact)| (version, artifact.url))
                        .collect(),
                })
                .collect(),
            java_support: m.java_support,
        })
        .collect())
}

/// 已安装组件（GUI 展示）。
#[derive(serde::Serialize)]
pub struct ComponentInfo {
    name: String,
    version: String,
    display_name: String,
    installed: bool,
}

/// 组件状态（GUI 展示）。
#[derive(serde::Serialize)]
pub struct ComponentStatusInfo {
    /// running / stopped / partial / error:*
    status: String,
}

/// WebUI 入口（GUI 展示）。
#[derive(serde::Serialize)]
pub struct WebUiInfo {
    name: String,
    url: String,
}

/// 组件的托管目录（展示真实路径）。
#[derive(serde::Serialize)]
pub struct ComponentDirs {
    instance: String,
    config: String,
    data: String,
    log: String,
}

/// 组件清单（GUI 展示）。
#[derive(serde::Serialize)]
pub struct ManifestInfo {
    component: String,
    source: Vec<SourceDefInfo>,
    java_support: std::collections::BTreeMap<String, Vec<u16>>,
}

/// 下载源（GUI 展示）。
#[derive(serde::Serialize)]
pub struct SourceDefInfo {
    name: String,
    versions: std::collections::BTreeMap<String, String>,
}
