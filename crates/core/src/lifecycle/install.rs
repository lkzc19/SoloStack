use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use crate::app::paths;
use crate::component::install_config::InstallConfig;
use crate::component::registry;
use crate::package::download;
use crate::package::extract;

/// 安装进度事件。
#[derive(Clone, Debug)]
pub enum ProgressEvent {
    /// 检查本地缓存（true = 文件存在，将在下载阶段校验 SHA256）。
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

/// 安装一个组件：下载 → 解压 → 生成配置（原子；任一步失败回滚已建产物）。
///
/// 返回实例目录（如 `data_root/components/hadoop/hadoop-3.5.0`）。
pub fn install(
    config: &InstallConfig,
    progress: Option<InstallProgress>,
    cancel: &AtomicBool,
) -> Result<std::path::PathBuf, String> {
    let operation = crate::app::app_log::Operation::begin(
        "install",
        &config.component,
        &config.version,
        &format!(
            "开始安装 {} v{}（源：{}）",
            config.component, config.version, config.source_id
        ),
    );
    let result = install_inner(config, progress, cancel);
    operation.finish(&result);
    result
}

fn install_inner(
    config: &InstallConfig,
    progress: Option<InstallProgress>,
    cancel: &AtomicBool,
) -> Result<std::path::PathBuf, String> {
    let name = &config.component;
    let version = &config.version;
    let environment_id = &config.environment_id;

    // 进度回调用 Arc<Mutex> 贯穿各阶段
    let shared: Option<Arc<Mutex<InstallProgress>>> = progress.map(|p| Arc::new(Mutex::new(p)));

    // 1. 组件必须已注册；下载/解压/生成任一失败都会由守卫清理已建产物
    registry::by_component(name).ok_or_else(|| format!("组件 {name} 未注册"))?;
    let environment = crate::app::environment::load(environment_id)?;
    environment.ensure_can_install(name, version)?;

    // 清理守卫：安装未正常完成（报错 / 取消）时删除本次已建实例目录与配置副本
    let mut guard = InstallGuard {
        environment_id: environment_id.clone(),
        name: name.clone(),
        version: version.clone(),
        stage: Stage::Downloading,
        done: false,
    };

    // 2. 从下载源解析完整地址，下载到 cache/downloads/（支持取消；已下载则复用）
    let artifact = crate::package::manifest::resolve_artifact(name, &config.source_id, version)?;
    let cached = download::target_path(&artifact.url)?.exists();
    emit(&shared, ProgressEvent::Checking(cached));
    let _ = crate::app::app_log::info(
        "install.download.begin",
        &format!(
            "{name} v{version} {}",
            if cached {
                "发现缓存包，开始 SHA256 校验"
            } else {
                "未发现缓存包，开始下载"
            }
        ),
    );
    let dl_progress: Option<crate::package::download::ProgressFn> = match &shared {
        Some(arc) => {
            let arc2 = Arc::clone(arc);
            Some(Box::new(move |bytes, total| {
                let mut g = arc2.lock().unwrap();
                (g.as_mut())(ProgressEvent::Downloading(bytes, total));
            }))
        }
        None => None,
    };
    let archive = match download::download(&artifact.url, &artifact.sha256, dl_progress, cancel) {
        Ok(a) => a,
        Err(e) => {
            let cancelled = cancel.load(Ordering::SeqCst) || e.contains("取消");
            let _ = if cancelled {
                crate::app::app_log::warn(
                    "install.cancelled",
                    &format!("{name} v{version} 安装已取消"),
                )
            } else {
                crate::app::app_log::error(
                    "install.download.failed",
                    &format!("{name} v{version} 下载失败: {e}"),
                )
            };
            return Err(e);
        }
    };
    if cancel.load(Ordering::SeqCst) {
        let _ = crate::app::app_log::warn(
            "install.cancelled",
            &format!("{name} v{version} 安装已取消"),
        );
        return Err("安装已取消".to_string());
    }
    let _ = crate::app::app_log::info(
        "install.download.done",
        &format!("{name} v{version} 下载完成"),
    );

    // 3. 解压到实例目录（tar.gz）
    guard.stage = Stage::Extracting;
    emit(&shared, ProgressEvent::Extracting);
    let instance = paths::instance_dir(environment_id, name, version).map_err(|e| e.to_string())?;
    extract::extract_tar_gz(&archive, &instance)?;
    let _ = crate::app::app_log::info(
        "install.extract.done",
        &format!("{name} v{version} 解压完成"),
    );

    if cancel.load(Ordering::SeqCst) {
        let _ = crate::app::app_log::warn(
            "install.cancelled",
            &format!("{name} v{version} 安装已取消"),
        );
        return Err("安装已取消".to_string());
    }

    // 4. 生成配置（含探活端口）+ JAVA_HOME
    guard.stage = Stage::Configuring;
    emit(&shared, ProgressEvent::Configuring);
    apply_install(config)?;
    let _ = crate::app::app_log::info(
        "install.config.done",
        &format!("{name} v{version} 配置生成完成"),
    );

    // 5. 登记环境组件清单；失败时由守卫回滚实例目录
    let mut environment = crate::app::environment::load(environment_id)?;
    environment.register_component(name, version)?;

    // 6. 通知完成
    guard.done = true;
    emit(&shared, ProgressEvent::Done);
    let _ = crate::app::app_log::info("install.done", &format!("{name} v{version} 安装完成"));

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

/// 生成组件配置（组件自己把端口等选项落进官方配置文件）+ JAVA_HOME。
fn apply_install(config: &InstallConfig) -> Result<(), String> {
    let name = &config.component;
    let version = &config.version;
    let c = registry::by_component(name).ok_or_else(|| format!("组件 {name} 未注册"))?;
    crate::component::validate_install_params(name, version, &config.params)?;
    c.apply_install_config(&config.environment_id, version, &config.params)?;
    apply_java_home(config, c)?;
    Ok(())
}

/// 把 JAVA_HOME 写进组件声明的环境文件（hadoop → 官方 hadoop-env.sh；
/// kafka → SoloStack 生成的 solostack-env.sh）。
///
/// JDK 选择策略在 `component::exec` 里唯一实现（安装期禁止「任意本机 JDK」兜底）。
fn apply_java_home(
    config: &InstallConfig,
    c: &dyn crate::component::Component,
) -> Result<(), String> {
    let name = &config.component;
    if !crate::package::manifest::needs_java(name) {
        return Ok(());
    }
    let Some(env_file) = c.java_env_file() else {
        return Ok(());
    };
    let home = crate::component::exec::resolve_jdk_for_install(
        &config.environment_id,
        c,
        &config.version,
        &config.jdk_version,
    )?;
    let path =
        crate::component::config_path(&config.environment_id, name, &config.version, env_file)?;
    let mut plan = crate::config::ConfigPlan::new();
    plan.set(path, "JAVA_HOME", home)?;
    crate::config::apply_plan(&plan)
}

/// 安装阶段（用于清理守卫判断哪些产物已被本次安装改动）。
enum Stage {
    Downloading,
    Extracting,
    Configuring,
}

/// 安装清理守卫：安装未正常完成（报错 / 取消）时，已进入解压 / 配置阶段则
/// 清理实例目录（配置就在实例目录内，一并清掉），并回收空的组件父目录。
/// 不触碰下载缓存（已完整下载的包保留）。
struct InstallGuard {
    environment_id: String,
    name: String,
    version: String,
    stage: Stage,
    done: bool,
}

impl Drop for InstallGuard {
    fn drop(&mut self) {
        if self.done {
            return;
        }
        if matches!(self.stage, Stage::Extracting | Stage::Configuring) {
            if let Ok(dir) = paths::instance_dir(&self.environment_id, &self.name, &self.version) {
                let _ = std::fs::remove_dir_all(&dir);
            }
            if let Ok(dir) = paths::component_dir(&self.environment_id, &self.name) {
                let _ = std::fs::remove_dir(&dir); // 仅在空目录时成功
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::component::install_config::InstallParams;

    fn test_config() -> InstallConfig {
        InstallConfig {
            environment_id: "00000000-0000-4000-8000-000000000001".into(),
            component: "hadoop".into(),
            version: "3.5.0".into(),
            source_id: "清华源".into(),
            jdk_version: String::new(),
            params: InstallParams::new(),
        }
    }

    #[test]
    fn install_rejects_unknown_source() {
        use crate::test_util::HOME_LOCK;
        let _guard = HOME_LOCK.lock().unwrap();
        let tmp = std::env::temp_dir().join("solostack-install-source");
        std::env::set_var("HOME", &tmp);
        let _ = std::fs::remove_dir_all(&tmp);
        let mut cfg = test_config();
        let environment = crate::app::environment::create("默认环境").unwrap();
        cfg.environment_id = environment.id;
        cfg.source_id = "不存在的源".into();
        let cancel = AtomicBool::new(false);
        let err = install(&cfg, None, &cancel);
        assert!(err.is_err());

        let _ = std::fs::remove_dir_all(&tmp);
    }
}
