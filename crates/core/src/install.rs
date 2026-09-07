use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::config;
use crate::download;
use crate::extract;
use crate::install_config::InstallConfig;
use crate::paths;

/// 安装进度事件。
#[derive(Clone, Debug)]
pub enum ProgressEvent {
    /// 检查包是否已下载（true = 已存在，将跳过下载）。
    Checking(bool),
    /// 下载中（已下载字节, 总字节；total 可能为 0）。
    Downloading(u64, u64),
    /// 解压中。
    Extracting,
    /// 生成配置中。
    Configuring,
    /// 安装完成。
    Done,
}

/// 安装进度回调（各阶段触发）。
pub type InstallProgress = Box<dyn FnMut(ProgressEvent) + Send>;

/// 安装一个组件：写临时 install.json → 下载 → 解压 → 生成配置 → 删除临时 json。
///
/// 返回实例目录（如 `data_root/components/hadoop/hadoop-3.5.0`）。
pub fn install(
    config: &InstallConfig,
    progress: Option<InstallProgress>,
    cancel: &AtomicBool,
) -> Result<std::path::PathBuf, String> {
    let name = &config.component;
    let version = &config.version;

    // 进度回调用 Arc<Mutex> 贯穿各阶段
    let shared: Option<Arc<Mutex<InstallProgress>>> = progress.map(|p| Arc::new(Mutex::new(p)));

    // 1. 计算探活端口 + 写临时 install.json（先落盘，再下载）
    let history_web = if config.history_enabled { config.history_web_port } else { 0 };
    let detect_ports = config::compute_detect_ports(name, config.namenode_web, config.yarn_rm, history_web);
    let temp_json = write_temp_install(config)?;
    let _ = crate::app_log::append(
        crate::app_log::INFO,
        &format!("开始安装 {name} v{version}（源：{}）", config.source_id),
    );

    // 清理守卫：安装未正常完成（报错 / 取消）时删除本次临时产物
    let mut guard = InstallGuard {
        name: name.clone(),
        version: version.clone(),
        temp_json: temp_json.clone(),
        stage: Stage::Downloading,
        done: false,
    };

    // 2. 从下载源解析完整地址，下载到 var/downloads/（支持取消；已下载则复用）
    let url = crate::config_defs::resolve_url(name, &config.source_id, version)?;
    let cached = download::target_path(&url)?.exists();
    emit(&shared, ProgressEvent::Checking(cached));
    let _ = crate::app_log::append(
        crate::app_log::INFO,
        &format!("{name} v{version} {}", if cached { "包已存在，跳过下载" } else { "包不存在，开始下载" }),
    );
    let dl_progress: Option<crate::download::ProgressFn> = match &shared {
        Some(arc) => {
            let arc2 = Arc::clone(arc);
            Some(Box::new(move |bytes, total| {
                let mut g = arc2.lock().unwrap();
                (g.as_mut())(ProgressEvent::Downloading(bytes, total));
            }))
        }
        None => None,
    };
    let archive = match download::download(&url, dl_progress, cancel) {
        Ok(a) => a,
        Err(e) => {
            let _ = crate::app_log::append(crate::app_log::WARN, &format!("{name} v{version} 安装已取消"));
            return Err(e);
        }
    };
    if cancel.load(Ordering::SeqCst) {
        let _ = crate::app_log::append(crate::app_log::WARN, &format!("{name} v{version} 安装已取消"));
        return Err("安装已取消".to_string());
    }
    let _ = crate::app_log::append(crate::app_log::INFO, &format!("{name} v{version} 下载完成"));

    // 3. 解压到实例目录（tar.gz）
    guard.stage = Stage::Extracting;
    emit(&shared, ProgressEvent::Extracting);
    let instance = paths::instance_dir(name, version).map_err(|e| e.to_string())?;
    extract::extract_tar_gz(&archive, &instance)?;
    let _ = crate::app_log::append(crate::app_log::INFO, &format!("{name} v{version} 解压完成"));

    if cancel.load(Ordering::SeqCst) {
        let _ = crate::app_log::append(crate::app_log::WARN, &format!("{name} v{version} 安装已取消"));
        return Err("安装已取消".to_string());
    }

    // 4. 生成配置 + 探活端口 + JAVA_HOME
    guard.stage = Stage::Configuring;
    emit(&shared, ProgressEvent::Configuring);
    apply_install(config, &detect_ports)?;
    let _ = crate::app_log::append(crate::app_log::INFO, &format!("{name} v{version} 配置生成完成"));

    // 5. 删除临时 install.json，通知完成
    let _ = std::fs::remove_file(&temp_json);
    guard.done = true;
    emit(&shared, ProgressEvent::Done);
    let _ = crate::app_log::append(crate::app_log::INFO, &format!("{name} v{version} 安装完成"));

    Ok(instance)
}

/// 触发进度回调（无回调时忽略）。
fn emit(shared: &Option<Arc<Mutex<InstallProgress>>>, ev: ProgressEvent) {
    if let Some(arc) = shared {
        if let Ok(mut guard) = arc.lock() {
            (guard.as_mut())(ev);
        }
    }
}

/// 写临时 install.json 到 `var/installs/`，返回路径。
fn write_temp_install(config: &InstallConfig) -> Result<std::path::PathBuf, String> {
    let dir = paths::installs_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let suffix = install_suffix();
    let path = dir.join(format!("{}-{}-install.json", config.component, suffix));
    let content = serde_json::to_string_pretty(config).map_err(|e| format!("序列化安装配置失败: {e}"))?;
    std::fs::write(&path, content).map_err(|e| format!("写入临时安装配置失败: {e}"))?;
    Ok(path)
}

/// 生成组件配置 + 探活端口 + JAVA_HOME。
fn apply_install(config: &InstallConfig, detect_ports: &[u16]) -> Result<(), String> {
    let name = &config.component;
    let version = &config.version;
    // 先写探活端口（会创建 etc 目录），再 prepare_config（复制 + 生成配置）
    config::write_detect_ports(name, version, detect_ports)?;
    config::prepare_config(name, version)?;
    write_java_home(config)?;
    Ok(())
}

/// 解析用户所选 JDK 路径，写 `.java-home`（通用）+ hadoop-env.sh（hadoop 原生读取）。
fn write_java_home(config: &InstallConfig) -> Result<(), String> {
    let home = resolve_jdk_path(config)?;
    let name = &config.component;
    let version = &config.version;
    config::write_java_home(name, version, &home)?;
    if name == "hadoop" {
        config::write_hadoop_env_java_home(name, version, &home)?;
    }
    Ok(())
}

/// 解析 JDK 路径：优先用户指定（目录名/版本），否则从支持列表挑本机已装的。
fn resolve_jdk_path(config: &InstallConfig) -> Result<String, String> {
    let name = &config.component;
    if !config.jdk_version.is_empty() {
        if let Some(j) = crate::jdk::find_by_name(&config.jdk_version) {
            return Ok(j.path.display().to_string());
        }
        if let Some(j) = crate::jdk::find_matching(&config.jdk_version) {
            return Ok(j.path.display().to_string());
        }
        return Err(format!("本机未找到 JDK {}", config.jdk_version));
    }
    let supported = crate::config_defs::component(name)
        .map(|c| {
            c.java_support
                .get(&config.version)
                .cloned()
                .unwrap_or_default()
                .iter()
                .map(|v| v.to_string())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    for req in supported {
        if let Some(j) = crate::jdk::find_matching(&req) {
            return Ok(j.path.display().to_string());
        }
    }
    Err(format!("本机未找到 {name} v{} 可用的 JDK", config.version))
}

/// 生成 4 位十六进制随机后缀（无 rand 依赖）。
fn install_suffix() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    let v = (nanos ^ std::process::id()) & 0xFFFF;
    format!("{v:04X}")
}

/// 安装阶段（用于清理守卫判断哪些产物已被本次安装改动）。
enum Stage {
    Downloading,
    Extracting,
    Configuring,
}

/// 安装清理守卫：安装未正常完成时，删除临时 install.json；
/// 已进入解压 / 配置阶段则连带清理实例目录与配置副本。
/// 不触碰下载缓存（已完整下载的包保留）。
struct InstallGuard {
    name: String,
    version: String,
    temp_json: std::path::PathBuf,
    stage: Stage,
    done: bool,
}

impl Drop for InstallGuard {
    fn drop(&mut self) {
        if self.done {
            return;
        }
        let _ = std::fs::remove_file(&self.temp_json);
        if matches!(self.stage, Stage::Extracting | Stage::Configuring) {
            if let Ok(dir) = paths::instance_dir(&self.name, &self.version) {
                let _ = std::fs::remove_dir_all(&dir);
            }
            if let Ok(dir) = paths::etc_instance_dir(&self.name, &self.version) {
                let _ = std::fs::remove_dir_all(&dir);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> InstallConfig {
        InstallConfig {
            component: "hadoop".into(),
            version: "3.5.0".into(),
            source_id: "清华源".into(),
            jdk_version: String::new(),
            namenode_web: 0,
            yarn_rm: 0,
            history_enabled: false,
            history_web_port: 0,
        }
    }

    #[test]
    fn install_suffix_is_4_hex() {
        let s = install_suffix();
        assert_eq!(s.len(), 4);
        assert!(s.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn install_rejects_unknown_source() {
        let mut cfg = test_config();
        cfg.source_id = "不存在的源".into();
        let cancel = AtomicBool::new(false);
        let err = install(&cfg, None, &cancel);
        assert!(err.is_err());
    }
}
