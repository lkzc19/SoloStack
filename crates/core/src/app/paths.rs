use std::path::{Path, PathBuf};

use super::id;

/// SoloStack 数据根目录名（固定 `~/.solostack`）。
pub const ROOT_DIR_NAME: &str = ".solostack";

/// 应用级目录。
pub const APP_DIR: &str = "app";
pub const APP_LOG_DIR: &str = "log";
pub const CACHE_DIR: &str = "cache";
pub const DOWNLOADS_DIR: &str = "downloads";
pub const ENVIRONMENTS_DIR: &str = "environments";

/// 环境目录。
pub const COMPONENTS_DIR: &str = "components";
pub const VAR_DIR: &str = "var";
pub const VAR_DATA_DIR: &str = "data";
pub const VAR_LOG_DIR: &str = "log";
pub const VAR_RUN_DIR: &str = "run";

/// 用户设置文件名。
pub const SETTINGS_FILE: &str = "settings.json";
/// 环境元数据文件名。
pub const ENVIRONMENT_FILE: &str = "environment.json";

/// SoloStack 数据根目录：`~/.solostack/`。
pub fn root_dir() -> Result<PathBuf, std::io::Error> {
    let home = dirs::home_dir()
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "无法获取用户主目录"))?;
    Ok(home.join(ROOT_DIR_NAME))
}

fn join_root(dir: &str) -> Result<PathBuf, std::io::Error> {
    Ok(root_dir()?.join(dir))
}

/// 校验环境 ID，防止把用户可控字符串拼进目录导致路径逃逸。
pub fn validate_environment_id(id: &str) -> Result<(), std::io::Error> {
    if id::is_environment_id(id) {
        Ok(())
    } else {
        Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("无效的环境 ID: {id}"),
        ))
    }
}

fn checked_environment_dir(id: &str) -> Result<PathBuf, std::io::Error> {
    validate_environment_id(id)?;
    Ok(environments_dir()?.join(id))
}

/// `environments/`：全部环境目录。
pub fn environments_dir() -> Result<PathBuf, std::io::Error> {
    join_root(ENVIRONMENTS_DIR)
}

/// `environments/<environment-id>/`：单个环境根目录。
pub fn environment_dir(environment_id: &str) -> Result<PathBuf, std::io::Error> {
    checked_environment_dir(environment_id)
}

/// `environments/<environment-id>/environment.json`。
pub fn environment_file(environment_id: &str) -> Result<PathBuf, std::io::Error> {
    Ok(environment_dir(environment_id)?.join(ENVIRONMENT_FILE))
}

/// 组件实例目录名：`<组件>-<版本>`。
pub fn instance_dir_name(component: &str, version: &str) -> String {
    format!("{component}-{version}")
}

/// `environments/<id>/components/`。
pub fn components_dir(environment_id: &str) -> Result<PathBuf, std::io::Error> {
    Ok(environment_dir(environment_id)?.join(COMPONENTS_DIR))
}

/// `environments/<id>/components/<component>/`。
pub fn component_dir(environment_id: &str, component: &str) -> Result<PathBuf, std::io::Error> {
    Ok(components_dir(environment_id)?.join(component))
}

/// `environments/<id>/components/<component>/<component>-<version>/`。
pub fn instance_dir(
    environment_id: &str,
    component: &str,
    version: &str,
) -> Result<PathBuf, std::io::Error> {
    Ok(component_dir(environment_id, component)?.join(instance_dir_name(component, version)))
}

/// `environments/<id>/var/`。
pub fn var_dir(environment_id: &str) -> Result<PathBuf, std::io::Error> {
    Ok(environment_dir(environment_id)?.join(VAR_DIR))
}

/// `environments/<id>/var/data/<component>/<component>-<version>/`。
pub fn var_data_instance_dir(
    environment_id: &str,
    component: &str,
    version: &str,
) -> Result<PathBuf, std::io::Error> {
    Ok(var_dir(environment_id)?
        .join(VAR_DATA_DIR)
        .join(component)
        .join(instance_dir_name(component, version)))
}

/// `environments/<id>/var/log/<component>/<component>-<version>/`。
pub fn var_log_instance_dir(
    environment_id: &str,
    component: &str,
    version: &str,
) -> Result<PathBuf, std::io::Error> {
    Ok(var_dir(environment_id)?
        .join(VAR_LOG_DIR)
        .join(component)
        .join(instance_dir_name(component, version)))
}

/// `environments/<id>/var/run/<component>-<version>.json`。
pub fn runtime_file(
    environment_id: &str,
    component: &str,
    version: &str,
) -> Result<PathBuf, std::io::Error> {
    Ok(
        var_run_dir(environment_id)?
            .join(format!("{}.json", instance_dir_name(component, version))),
    )
}

/// `environments/<id>/var/run/`。
pub fn var_run_dir(environment_id: &str) -> Result<PathBuf, std::io::Error> {
    Ok(var_dir(environment_id)?.join(VAR_RUN_DIR))
}

/// `environments/<id>/var/run/<component>/<component>-<version>/`。
pub fn var_run_instance_dir(
    environment_id: &str,
    component: &str,
    version: &str,
) -> Result<PathBuf, std::io::Error> {
    Ok(var_run_dir(environment_id)?
        .join(component)
        .join(instance_dir_name(component, version)))
}

/// `app/`：SoloStack 自身数据目录。
pub fn app_dir() -> Result<PathBuf, std::io::Error> {
    join_root(APP_DIR)
}

/// `app/log/`：SoloStack 自身操作日志目录。
pub fn app_log_dir() -> Result<PathBuf, std::io::Error> {
    Ok(app_dir()?.join(APP_LOG_DIR))
}

/// `cache/`：应用级共享缓存。
pub fn cache_dir() -> Result<PathBuf, std::io::Error> {
    join_root(CACHE_DIR)
}

/// `cache/downloads/`：所有环境共享的安装包缓存。
pub fn downloads_dir() -> Result<PathBuf, std::io::Error> {
    Ok(cache_dir()?.join(DOWNLOADS_DIR))
}

/// 设置文件路径：`~/.solostack/app/settings.json`。
pub fn settings_file() -> Result<PathBuf, std::io::Error> {
    Ok(app_dir()?.join(SETTINGS_FILE))
}

/// 创建应用级目录。
pub fn ensure_app_dirs() -> Result<PathBuf, std::io::Error> {
    let root = root_dir()?;
    for dir in [APP_DIR, CACHE_DIR, ENVIRONMENTS_DIR] {
        std::fs::create_dir_all(root.join(dir))?;
    }
    std::fs::create_dir_all(root.join(APP_DIR).join(APP_LOG_DIR))?;
    std::fs::create_dir_all(root.join(CACHE_DIR).join(DOWNLOADS_DIR))?;
    Ok(root)
}

/// 创建单个环境的完整目录。
pub fn ensure_environment_dirs(environment_id: &str) -> Result<PathBuf, std::io::Error> {
    ensure_app_dirs()?;
    let root = environment_dir(environment_id)?;
    for dir in [COMPONENTS_DIR, VAR_DIR] {
        std::fs::create_dir_all(root.join(dir))?;
    }
    for dir in [VAR_DATA_DIR, VAR_LOG_DIR, VAR_RUN_DIR] {
        std::fs::create_dir_all(root.join(VAR_DIR).join(dir))?;
    }
    Ok(root)
}

/// 兼容旧调用：初始化全部应用级目录。
pub fn ensure_dirs() -> Result<PathBuf, std::io::Error> {
    ensure_app_dirs()
}

/// 校验路径位于整个 SoloStack 根目录内。
pub fn is_within_root(path: &Path) -> Result<bool, std::io::Error> {
    if path
        .components()
        .any(|component| component == std::path::Component::ParentDir)
    {
        return Ok(false);
    }
    Ok(path.starts_with(root_dir()?))
}

/// 校验路径位于指定环境目录内。
pub fn is_within_environment(environment_id: &str, path: &Path) -> Result<bool, std::io::Error> {
    if path
        .components()
        .any(|component| component == std::path::Component::ParentDir)
    {
        return Ok(false);
    }
    Ok(path.starts_with(environment_dir(environment_id)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn environment_id() -> &'static str {
        "00000000-0000-4000-8000-000000000001"
    }

    #[test]
    fn root_dir_ends_with_solostack() {
        use crate::test_util::HOME_LOCK;
        let _guard = HOME_LOCK.lock().unwrap();
        let root = root_dir().unwrap();
        assert_eq!(root.file_name().unwrap(), ROOT_DIR_NAME);
        assert!(root.parent().is_some());
    }

    #[test]
    fn instance_dir_name_joins_component_version() {
        assert_eq!(instance_dir_name("hadoop", "3.5.0"), "hadoop-3.5.0");
    }

    #[test]
    fn environment_instance_paths_are_isolated() {
        use crate::test_util::HOME_LOCK;
        let _guard = HOME_LOCK.lock().unwrap();
        let first = instance_dir(environment_id(), "hadoop", "3.5.0").unwrap();
        let second =
            instance_dir("00000000-0000-4000-8000-000000000002", "hadoop", "3.5.0").unwrap();
        assert_ne!(first, second);
        assert!(first.starts_with(environment_dir(environment_id()).unwrap()));
    }

    #[test]
    fn shared_cache_is_outside_environment() {
        use crate::test_util::HOME_LOCK;
        let _guard = HOME_LOCK.lock().unwrap();
        let cache = downloads_dir().unwrap();
        assert_eq!(cache, root_dir().unwrap().join("cache/downloads"));
        assert!(!cache.starts_with(environment_dir(environment_id()).unwrap()));
    }

    #[test]
    fn invalid_environment_id_is_rejected() {
        assert!(environment_dir("../../etc").is_err());
        assert!(environment_dir("hadoop").is_err());
    }

    #[test]
    fn ensure_environment_dirs_creates_expected_layout() {
        use crate::test_util::HOME_LOCK;
        let _guard = HOME_LOCK.lock().unwrap();
        let tmp = std::env::temp_dir().join("solostack-paths-environment");
        std::env::set_var("HOME", &tmp);
        let _ = std::fs::remove_dir_all(&tmp);

        let root = ensure_environment_dirs(environment_id()).unwrap();
        for dir in ["components", "var/data", "var/log", "var/run"] {
            assert!(root.join(dir).is_dir(), "{dir} 未创建");
        }
        assert!(downloads_dir().unwrap().is_dir());

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn is_within_environment_rejects_escape() {
        use crate::test_util::HOME_LOCK;
        let _guard = HOME_LOCK.lock().unwrap();
        let inside = instance_dir(environment_id(), "hadoop", "3.5.0").unwrap();
        assert!(is_within_environment(environment_id(), &inside).unwrap());

        let outside = std::env::temp_dir();
        assert!(!is_within_environment(environment_id(), &outside).unwrap());
    }
}
