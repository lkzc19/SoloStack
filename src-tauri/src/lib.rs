// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use tauri::Emitter;

/// 当前进行中的安装取消标志（同一时刻至多一个安装）。
static CANCEL_INSTALL: Mutex<Option<Arc<AtomicBool>>> = Mutex::new(None);

/// 按组件名解析已安装实例（失败报「未安装」）。
fn resolve(component: &str) -> Result<solostack_core::instances::Installed, String> {
    solostack_core::instances::resolve(component)
}

#[tauri::command]
fn get_arch() -> String {
    solostack_core::arch::Arch::current().to_string()
}

#[tauri::command]
fn get_root_dir() -> Result<String, String> {
    solostack_core::paths::root_dir()
        .map(|p| p.display().to_string())
        .map_err(|e| e.to_string())
}

/// 当前设置（配置页回显）。
#[tauri::command]
fn get_settings() -> Result<SettingsInfo, String> {
    let root = solostack_core::paths::root_dir().map_err(|e| e.to_string())?;
    let settings = solostack_core::settings::Settings::load().map_err(|e| e.to_string())?;
    Ok(SettingsInfo {
        data_root: root.display().to_string(),
        log_viewer: settings.log_viewer,
    })
}

/// 白名单里已安装的编辑器（日志查看器候选）。
#[tauri::command]
fn list_apps() -> Vec<solostack_core::app_scan::AppDef> {
    solostack_core::app_scan::list_available_apps()
}

/// 设置打开日志文件的默认应用（存 Bundle ID）。
#[tauri::command]
fn set_log_viewer(app: String) -> Result<(), String> {
    let mut settings = solostack_core::settings::Settings::load().map_err(|e| e.to_string())?;
    settings.log_viewer = app;
    settings.save().map_err(|e| e.to_string())
}

/// 用设置的默认应用（Bundle ID）打开日志文件。
///
/// 所选应用不可用（如已卸载）时，自动降级为系统默认应用打开（`open <path>`）。
#[tauri::command]
fn open_log_file(path: String) -> Result<(), String> {
    let settings = solostack_core::settings::Settings::load().map_err(|e| e.to_string())?;
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

/// 获取组件的语义化配置字段列表（含当前值，供 GUI 表单渲染）。
#[tauri::command]
fn list_config_fields(component: String) -> Result<Vec<solostack_core::config_schema::FieldMeta>, String> {
    let i = resolve(&component)?;
    solostack_core::config_schema::list_fields(&i.name, &i.version)
}

/// 设置组件的某个语义化配置字段。
#[tauri::command]
fn set_config_field(component: String, field_id: String, value: String) -> Result<(), String> {
    let i = resolve(&component)?;
    solostack_core::config_schema::set_field(&i.name, &i.version, &field_id, &value)
}

/// 列出已安装的组件（从 components/ 目录发现）。
#[tauri::command]
fn list_component_templates() -> Result<Vec<ComponentInfo>, String> {
    Ok(solostack_core::instances::list_installed()
        .into_iter()
        .map(|i| ComponentInfo {
            name: i.name,
            version: i.version,
            display_name: i.display_name,
            category: i.category,
            installed: true,
        })
        .collect())
}

/// 当前设置（GUI 配置页）。
#[derive(serde::Serialize)]
struct SettingsInfo {
    data_root: String,
    log_viewer: String,
}

/// 已安装组件（GUI 展示）。
#[derive(serde::Serialize)]
struct ComponentInfo {
    name: String,
    version: String,
    display_name: String,
    category: String,
    installed: bool,
}

/// Web UI 入口（GUI 展示用）。
#[derive(serde::Serialize)]
struct WebUiInfo {
    name: String,
    url: String,
}

/// 获取组件的 WebUI 跳转地址（读探活端口，含占用避让后的实际端口）。
#[tauri::command]
fn get_web_ui_urls(component: String) -> Result<Vec<WebUiInfo>, String> {
    let i = resolve(&component)?;
    let ports = solostack_core::config::read_detect_ports(&i.name, &i.version);
    let urls = match i.name.as_str() {
        "hadoop" => {
            let nn = ports.first().copied().unwrap_or(9870);
            let rm = ports.get(2).copied().unwrap_or(8088);
            let mut list = vec![
                WebUiInfo { name: "HDFS".into(), url: format!("http://localhost:{nn}") },
                WebUiInfo { name: "YARN".into(), url: format!("http://localhost:{rm}") },
            ];
            if ports.len() > 4 {
                let hs = ports[4];
                list.push(WebUiInfo { name: "JobHistory".into(), url: format!("http://localhost:{hs}") });
            }
            list
        }
        _ => vec![],
    };
    Ok(urls)
}

/// 启动组件（全启）。
#[tauri::command]
fn start_component(component: String, _service: Option<String>) -> Result<(), String> {
    let i = resolve(&component)?;
    solostack_core::service::start(&i.name, &i.version)
}

/// 停止组件（全停）。
#[tauri::command]
fn stop_component(component: String, _service: Option<String>) -> Result<(), String> {
    let i = resolve(&component)?;
    solostack_core::service::stop(&i.name, &i.version)
}

/// 组件运行状态。
#[tauri::command]
fn get_component_status(component: String) -> Result<ComponentStatusInfo, String> {
    let i = resolve(&component)?;
    Ok(ComponentStatusInfo {
        status: status_str(&solostack_core::service::component_status(&i.name, &i.version)),
    })
}

/// 组件状态信息（GUI 展示用）。
#[derive(serde::Serialize)]
struct ComponentStatusInfo {
    status: String, // running / stopped / partial / error
}

/// 状态枚举 → 前端字符串。
fn status_str(s: &solostack_core::service::Status) -> String {
    match s {
        solostack_core::service::Status::Running => "running".to_string(),
        solostack_core::service::Status::Stopped => "stopped".to_string(),
        solostack_core::service::Status::Partial => "partial".to_string(),
        solostack_core::service::Status::Error(e) => format!("error:{e}"),
    }
}

/// 安装组件：解析源 URL → 下载 → 解压 → 生成配置。后台线程执行。
#[tauri::command]
async fn install_component(
    app: tauri::AppHandle,
    component: String,
    version: String,
    source_id: String,
    jdk_version: String,
    ports: Option<HadoopPorts>,
) -> Result<(), String> {
    let (namenode_web, yarn_rm, history_enabled, history_web_port) = match &ports {
        Some(p) => (p.namenode_web, p.yarn_rm, p.history_enabled, p.history_web_port),
        None => (0, 0, false, 0),
    };
    let config = solostack_core::install_config::InstallConfig {
        component: component.clone(),
        version: version.clone(),
        source_id,
        jdk_version,
        namenode_web,
        yarn_rm,
        history_enabled,
        history_web_port,
    };

    // 取消标志：注册到全局，供 `cancel_install` 命令触发
    let cancel = Arc::new(AtomicBool::new(false));
    *CANCEL_INSTALL.lock().unwrap() = Some(cancel.clone());

    // 进度回调 → 推送到前端 "install-progress" 事件
    let app2 = app.clone();
    let progress: Option<solostack_core::install::InstallProgress> = Some(Box::new(move |ev| {
        let payload = match ev {
            solostack_core::install::ProgressEvent::Checking(cached) => InstallProgressPayload {
                phase: "checking".into(),
                bytes: 0,
                total: 0,
                cached,
            },
            solostack_core::install::ProgressEvent::Downloading(bytes, total) => InstallProgressPayload {
                phase: "downloading".into(),
                bytes,
                total,
                cached: false,
            },
            solostack_core::install::ProgressEvent::Extracting => InstallProgressPayload {
                phase: "extracting".into(),
                bytes: 0,
                total: 0,
                cached: false,
            },
            solostack_core::install::ProgressEvent::Configuring => InstallProgressPayload {
                phase: "configuring".into(),
                bytes: 0,
                total: 0,
                cached: false,
            },
            solostack_core::install::ProgressEvent::Done => InstallProgressPayload {
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
        solostack_core::install::install(&config, progress, &cancel2)
    })
    .await;

    *CANCEL_INSTALL.lock().unwrap() = None;

    result.map_err(|e| e.to_string())?.map(|_| ())
}

/// 终止当前进行中的安装。
#[tauri::command]
fn cancel_install() -> Result<(), String> {
    let guard = CANCEL_INSTALL.lock().unwrap();
    match guard.as_ref() {
        Some(c) => {
            c.store(true, Ordering::SeqCst);
            Ok(())
        }
        None => Err("当前没有进行中的安装".to_string()),
    }
}

/// 该组件某源某版本的包是否已下载（安装页版本下拉展示用）。
#[tauri::command]
fn is_package_downloaded(component: String, source_id: String, version: String) -> Result<bool, String> {
    let url = solostack_core::config_defs::resolve_url(&component, &source_id, &version)?;
    Ok(solostack_core::download::target_path(&url)?.exists())
}

/// 列出 `var/downloads/` 下所有已下载的包（名称 + 大小），供设置-缓存 tab 展示。
#[tauri::command]
fn list_download_packages() -> Result<Vec<DownloadPackageInfo>, String> {
    Ok(solostack_core::download::list_downloaded_packages()?
        .into_iter()
        .map(|p| DownloadPackageInfo { name: p.name, size: p.size })
        .collect())
}

/// 删除 `var/downloads/` 下指定的缓存包。
#[tauri::command]
fn delete_download_packages(names: Vec<String>) -> Result<(), String> {
    solostack_core::download::delete_downloaded_packages(&names)
}

/// 缓存包信息（GUI 展示用）。
#[derive(serde::Serialize)]
struct DownloadPackageInfo {
    name: String,
    size: u64,
}

/// hadoop 可配置的 WebUI 端口 + 历史服务器。
#[derive(serde::Deserialize)]
struct HadoopPorts {
    namenode_web: u16,
    yarn_rm: u16,
    #[serde(default)]
    history_enabled: bool,
    #[serde(default)]
    history_web_port: u16,
}

/// 安装进度事件负载。
#[derive(serde::Serialize, Clone)]
struct InstallProgressPayload {
    phase: String,
    bytes: u64,
    total: u64,
    cached: bool,
}

/// 本机已安装的 JDK 列表（安装页 JDK 下拉，含发行商 + 版本）。
#[tauri::command]
fn list_jdk_versions() -> Vec<JdkInfo> {
    solostack_core::jdk::scan()
        .into_iter()
        .map(|j| JdkInfo { name: j.name, vendor: j.vendor, version: j.version })
        .collect()
}

/// 本机 JDK（GUI 展示）。
#[derive(serde::Serialize)]
struct JdkInfo {
    name: String,
    vendor: String,
    version: String,
}

/// 读取 app 操作日志：默认今天的完整日志；`date` 传 `YYYY-MM-DD` 看历史某天。
#[tauri::command]
fn get_app_logs(date: Option<String>) -> Result<String, String> {
    let date = date.unwrap_or_else(solostack_core::app_log::today);
    solostack_core::app_log::read_logs_for(&date)
}

/// 列出 app 日志可用日期（倒序，新的在前）。
#[tauri::command]
fn list_log_dates() -> Result<Vec<String>, String> {
    solostack_core::app_log::list_log_dates()
}

/// 组件配置列表（安装页源/版本/JDK 数据来源，来自 config json）。
#[tauri::command]
fn list_component_configs() -> Result<Vec<ComponentConfigInfo>, String> {
    Ok(solostack_core::config_defs::load_all()?
        .into_iter()
        .map(|c| ComponentConfigInfo {
            id: c.id,
            source: c.source.into_iter().map(|s| SourceDefInfo { name: s.name, versions: s.version }).collect(),
            java_support: c.java_support,
        })
        .collect())
}

/// 组件配置（GUI 展示）。
#[derive(serde::Serialize)]
struct ComponentConfigInfo {
    id: String,
    source: Vec<SourceDefInfo>,
    java_support: std::collections::BTreeMap<String, Vec<u16>>,
}

/// 下载源（GUI 展示）。
#[derive(serde::Serialize)]
struct SourceDefInfo {
    name: String,
    versions: std::collections::BTreeMap<String, String>,
}

/// 纯净卸载组件实例（幂等）。`keep_data=true` 时保留持久数据目录。
#[tauri::command]
async fn uninstall_component(component: String, keep_data: bool) -> Result<(), String> {
    let i = resolve(&component)?;
    tauri::async_runtime::spawn_blocking(move || {
        solostack_core::uninstall::uninstall(&i.name, &i.version, keep_data)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// 列出组件日志文件（完整路径，按修改时间倒序）。
#[tauri::command]
fn list_component_logs(component: String) -> Result<Vec<String>, String> {
    let i = resolve(&component)?;
    solostack_core::logs::list_log_files(&i.name, &i.version)
        .map(|v| v.into_iter().map(|p| p.display().to_string()).collect())
}

/// 读取日志文件末尾若干行。仅允许该组件名下的日志路径。
#[tauri::command]
fn read_component_log_tail(component: String, path: String, lines: usize) -> Result<String, String> {
    let i = resolve(&component)?;
    let log_path = std::path::PathBuf::from(&path);
    solostack_core::logs::ensure_within_root(&log_path)?;
    if !solostack_core::logs::is_log_of(&i.name, &i.version, &log_path) {
        return Err(format!("日志路径不属于组件 {component}"));
    }
    solostack_core::logs::tail(&log_path, lines)
}

/// 组件的托管目录（展示真实路径）。
#[derive(serde::Serialize)]
struct ComponentDirs {
    instance: String,
    etc: String,
    data: String,
    log: String,
}

/// 获取组件各托管目录路径。
#[tauri::command]
fn get_component_dirs(component: String) -> Result<ComponentDirs, String> {
    let i = resolve(&component)?;
    let to_str = |r: Result<std::path::PathBuf, std::io::Error>| {
        r.map(|p| p.display().to_string()).map_err(|e| e.to_string())
    };
    let log = solostack_core::paths::var_log_instance_dir(&i.name, &i.version)
        .map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&log).map_err(|e| format!("创建日志目录失败: {e}"))?;
    Ok(ComponentDirs {
        instance: to_str(solostack_core::paths::instance_dir(&i.name, &i.version))?,
        etc: to_str(solostack_core::paths::etc_instance_dir(&i.name, &i.version))?,
        data: to_str(solostack_core::paths::var_data_instance_dir(&i.name, &i.version))?,
        log: log.display().to_string(),
    })
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|_app| {
            let _ = solostack_core::paths::migrate_legacy_layout();
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_arch,
            get_root_dir,
            list_jdk_versions,
            list_component_configs,
            get_app_logs,
            list_log_dates,
            get_settings,
            list_apps,
            set_log_viewer,
            open_log_file,
            list_config_fields,
            set_config_field,
            list_component_templates,
            start_component,
            stop_component,
            get_component_status,
            get_web_ui_urls,
            install_component,
            cancel_install,
            is_package_downloaded,
            list_download_packages,
            delete_download_packages,
            uninstall_component,
            list_component_logs,
            read_component_log_tail,
            get_component_dirs
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
