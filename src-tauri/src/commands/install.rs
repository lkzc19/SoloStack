//! 安装与卸载：安装组件（带进度事件）、取消安装、卸载、下载包缓存管理。

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use solostack_core::component::install_config::{InstallConfig, InstallParam, InstallParams};
use solostack_core::component::registry;
use solostack_core::lifecycle::{install, lock, uninstall};
use solostack_core::package::{download, manifest};
use tauri::Emitter;

use super::resolve;

/// 当前进行中的安装取消标志（同一时刻至多一个安装）。
static CANCEL_INSTALL: Mutex<Option<Arc<AtomicBool>>> = Mutex::new(None);
static INSTALL_RUNNING: AtomicBool = AtomicBool::new(false);

struct InstallRunningGuard;

impl Drop for InstallRunningGuard {
    fn drop(&mut self) {
        INSTALL_RUNNING.store(false, Ordering::SeqCst);
    }
}

fn begin_install_running() -> Result<InstallRunningGuard, String> {
    INSTALL_RUNNING
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .map(|_| InstallRunningGuard)
        .map_err(|_| "已有安装任务正在执行".to_string())
}

/// 安装组件：解析源 URL → 下载 → 解压 → 生成配置。后台线程执行。
#[tauri::command]
pub async fn install_component(
    app: tauri::AppHandle,
    environment_id: String,
    component: String,
    version: String,
    source_id: String,
    jdk_version: String,
    params: Option<InstallParams>,
) -> Result<(), String> {
    // 组件自定义安装参数：命令层只做透传，具体含义与校验归组件（Component::apply_install_config）
    let config = InstallConfig {
        environment_id: environment_id.clone(),
        component: component.clone(),
        version: version.clone(),
        source_id,
        jdk_version,
        params: params.unwrap_or_default(),
    };

    let install_guard = begin_install_running()?;
    let guard = lock::begin_environment_operation(&environment_id)?;

    // 取消标志：注册到全局，供 `cancel_install` 命令触发
    let cancel = Arc::new(AtomicBool::new(false));
    *CANCEL_INSTALL.lock().unwrap() = Some(cancel.clone());

    // 进度回调 → 推送到前端 "install-progress" 事件
    let app2 = app.clone();
    let progress: Option<install::InstallProgress> = Some(Box::new(move |ev| {
        let payload = match ev {
            install::ProgressEvent::Checking(cached) => InstallProgressPayload {
                phase: "checking".into(),
                bytes: 0,
                total: 0,
                cached,
            },
            install::ProgressEvent::Downloading(bytes, total) => InstallProgressPayload {
                phase: "downloading".into(),
                bytes,
                total,
                cached: false,
            },
            install::ProgressEvent::Extracting => InstallProgressPayload {
                phase: "extracting".into(),
                bytes: 0,
                total: 0,
                cached: false,
            },
            install::ProgressEvent::Configuring => InstallProgressPayload {
                phase: "configuring".into(),
                bytes: 0,
                total: 0,
                cached: false,
            },
            install::ProgressEvent::Done => InstallProgressPayload {
                phase: "done".into(),
                bytes: 0,
                total: 0,
                cached: false,
            },
        };
        let _ = app2.emit("install-progress", payload);
    }));

    let cancel2 = cancel.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        let _install_guard = install_guard;
        let _guard = guard;
        install::install(&config, progress, &cancel2)
    })
    .await;

    *CANCEL_INSTALL.lock().unwrap() = None;

    result.map_err(|e| e.to_string())?.map(|_| ())
}

/// 终止当前进行中的安装。
#[tauri::command]
pub fn cancel_install() -> Result<(), String> {
    let guard = CANCEL_INSTALL.lock().unwrap();
    match guard.as_ref() {
        Some(c) => {
            c.store(true, Ordering::SeqCst);
            Ok(())
        }
        None => Err("当前没有进行中的安装".to_string()),
    }
}

/// 纯净卸载组件实例（幂等）。`keep_data=true` 时保留持久数据目录。
#[tauri::command]
pub async fn uninstall_component(
    environment_id: String,
    component: String,
    keep_data: bool,
) -> Result<(), String> {
    let i = resolve(&environment_id, &component)?;
    let guard = lock::begin_environment_operation(&environment_id)?;
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = guard;
        uninstall::uninstall(&i.environment_id, &i.name, &i.version, keep_data)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// 组件的安装参数声明（id + 默认值），供安装页预填表单。
///
/// 表单布局与文案仍由前端按组件定制（各组件差异大，不做通用化）；
/// 这里只提供「有哪些参数、默认多少」这一事实来源，避免默认值在前端再写一份。
#[tauri::command]
pub fn list_install_params(component: String, version: String) -> Vec<InstallParam> {
    registry::by_component(&component)
        .map(|c| c.install_params(&version))
        .unwrap_or_default()
}

/// 该组件某源某版本的包是否已下载（安装页版本下拉展示用）。
#[tauri::command]
pub async fn is_package_downloaded(
    component: String,
    source_id: String,
    version: String,
) -> Result<bool, String> {
    let artifact = manifest::resolve_artifact(&component, &source_id, &version)?;
    tauri::async_runtime::spawn_blocking(move || {
        download::is_cached(&artifact.url, &artifact.sha256)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// 列出 `cache/downloads/` 下所有已下载的包（名称 + 大小），供设置-缓存 tab 展示。
#[tauri::command]
pub fn list_download_packages() -> Result<Vec<DownloadPackageInfo>, String> {
    Ok(download::list_downloaded_packages()?
        .into_iter()
        .map(|p| DownloadPackageInfo {
            name: p.name,
            size: p.size,
        })
        .collect())
}

/// 删除 `cache/downloads/` 下指定的缓存包。
#[tauri::command]
pub fn delete_download_packages(names: Vec<String>) -> Result<(), String> {
    if INSTALL_RUNNING.load(Ordering::SeqCst) {
        return Err("安装进行中，暂时不能删除包缓存".to_string());
    }
    download::delete_downloaded_packages(&names)
}

/// 安装进度事件负载。
#[derive(serde::Serialize, Clone)]
pub struct InstallProgressPayload {
    phase: String,
    bytes: u64,
    total: u64,
    cached: bool,
}

/// 缓存包信息（GUI 展示用）。
#[derive(serde::Serialize)]
pub struct DownloadPackageInfo {
    name: String,
    size: u64,
}
