//! 环境管理：创建、重命名、删除、列表与切换。

use solostack_core::app::environment::{self, Environment};
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
