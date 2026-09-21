//! 环境管理：创建、重命名、删除、列表与切换。

use solostack_core::app::environment::{self, Environment};
use solostack_core::app::environment_usage::{self, EnvironmentUsage};
use solostack_core::component::registry;
use solostack_core::lifecycle::{environment as environment_lifecycle, lock};

/// 列出全部环境。
#[tauri::command]
pub fn list_environments() -> Result<Vec<EnvironmentInfo>, String> {
    let active_id = environment::active_id()?;
    Ok(environment::list()?
        .into_iter()
        .map(|item| to_info(item, active_id.as_deref()))
        .collect())
}

/// 当前活动环境。
#[tauri::command]
pub fn get_active_environment() -> Result<Option<EnvironmentInfo>, String> {
    let active_id = environment::active_id()?;
    Ok(environment::active()?.map(|item| to_info(item, active_id.as_deref())))
}

/// 环境页概览：组件身份、运行状态和分类磁盘占用。
#[tauri::command]
pub async fn list_environment_overviews() -> Result<Vec<EnvironmentOverview>, String> {
    tauri::async_runtime::spawn_blocking(|| {
        let active_id = environment::active_id()?;
        environment::list()?
            .into_iter()
            .map(|item| environment_overview(item, active_id.as_deref()))
            .collect()
    })
    .await
    .map_err(|error| error.to_string())?
}

/// 创建环境。
#[tauri::command]
pub fn create_environment(name: String) -> Result<EnvironmentInfo, String> {
    let environment = environment::create(&name)?;
    if environment::active_id()?.is_none() {
        environment::set_active_id(Some(&environment.id))?;
    }
    let active_id = environment::active_id()?;
    Ok(to_info(environment, active_id.as_deref()))
}

/// 重命名环境。
#[tauri::command]
pub fn rename_environment(id: String, name: String) -> Result<EnvironmentInfo, String> {
    let mut environment = environment::load(&id)?;
    let _guard = lock::begin_environment_operation(&id)?;
    environment.rename(&name)?;
    let active_id = environment::active_id()?;
    Ok(to_info(environment, active_id.as_deref()))
}

/// 删除非活动环境。删除前停止其中全部组件。
#[tauri::command]
pub async fn delete_environment(id: String) -> Result<(), String> {
    if environment::active_id()?.as_deref() == Some(id.as_str()) {
        return Err("不能删除当前活动环境，请先切换到其他环境".to_string());
    }
    let environment = environment::load(&id)?;
    let guard = lock::begin_environment_operation(&id)?;
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = guard;
        environment_lifecycle::stop_all_components(&environment)?;
        environment.delete()
    })
    .await
    .map_err(|e| e.to_string())?
}

/// 切换活动环境。只有旧环境全部组件停止后才更新活动环境。
#[tauri::command]
pub async fn switch_environment(id: String) -> Result<EnvironmentInfo, String> {
    let environment =
        tauri::async_runtime::spawn_blocking(move || environment_lifecycle::switch_to(&id))
            .await
            .map_err(|e| e.to_string())??;
    let active_id = environment::active_id()?;
    Ok(to_info(environment, active_id.as_deref()))
}

fn to_info(environment: Environment, active_id: Option<&str>) -> EnvironmentInfo {
    EnvironmentInfo {
        id: environment.id.clone(),
        name: environment.name,
        active: active_id == Some(environment.id.as_str()),
        components: environment
            .components
            .into_iter()
            .map(|item| EnvironmentComponentInfo {
                component: item.component,
                version: item.version,
            })
            .collect(),
        created_at: environment.created_at,
        last_activated_at: environment.last_activated_at,
    }
}

fn environment_overview(
    environment: Environment,
    active_id: Option<&str>,
) -> Result<EnvironmentOverview, String> {
    let path = environment::directory(&environment.id)?;
    let usage = environment_usage::measure(&environment.id)?;
    let is_active = active_id == Some(environment.id.as_str());
    // 一次扫描进程表，批量求各组件展示状态（非活跃环境直接全 None）
    let statuses = environment_lifecycle::displayed_component_statuses(&environment, is_active)?;
    let components = environment
        .components
        .iter()
        .zip(statuses)
        .map(|(item, status)| EnvironmentComponentOverview {
            component: item.component.clone(),
            version: item.version.clone(),
            display_name: registry::display_name(&item.component),
            installed_at: item.installed_at.clone(),
            status: status.map(|status| status.to_wire()),
        })
        .collect();

    Ok(EnvironmentOverview {
        id: environment.id.clone(),
        name: environment.name,
        active: is_active,
        path: path.display().to_string(),
        components,
        usage,
        created_at: environment.created_at,
        last_activated_at: environment.last_activated_at,
    })
}

#[derive(serde::Serialize)]
pub struct EnvironmentInfo {
    id: String,
    name: String,
    active: bool,
    components: Vec<EnvironmentComponentInfo>,
    created_at: String,
    last_activated_at: Option<String>,
}

#[derive(serde::Serialize)]
pub struct EnvironmentComponentInfo {
    component: String,
    version: String,
}

#[derive(serde::Serialize)]
pub struct EnvironmentOverview {
    id: String,
    name: String,
    active: bool,
    path: String,
    components: Vec<EnvironmentComponentOverview>,
    usage: EnvironmentUsage,
    created_at: String,
    last_activated_at: Option<String>,
}

#[derive(serde::Serialize)]
pub struct EnvironmentComponentOverview {
    component: String,
    version: String,
    display_name: String,
    installed_at: String,
    /// 组件状态；非活跃环境为 None（不展示，避免误导）。
    status: Option<String>,
}
