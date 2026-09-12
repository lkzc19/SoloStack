use std::path::{Path, PathBuf};

/// SoloStack 数据根目录名（固定 `~/.solostack`，引导仅确认默认路径）。
pub const ROOT_DIR_NAME: &str = ".solostack";

/// 顶层子目录名。
pub const COMPONENTS_DIR: &str = "components";
pub const VAR_DIR: &str = "var";
/// app 自身目录（设置 + 操作日志）。
pub const APP_DIR: &str = "app";
/// 历史目录名（配置副本机制已废弃，仅迁移时清理用）。
const LEGACY_ETC_DIR: &str = "etc";

/// `var/` 下子目录。
pub const VAR_DATA_DIR: &str = "data";
pub const VAR_LOG_DIR: &str = "log";
pub const VAR_RUN_DIR: &str = "run";
pub const VAR_DOWNLOADS_DIR: &str = "downloads";

/// `app/` 下子目录。
pub const APP_LOG_DIR: &str = "log";

/// 用户设置文件名（存放于 `app/`）。
pub const SETTINGS_FILE: &str = "settings.json";

/// SoloStack 数据根目录：`~/.solostack/`。
pub fn root_dir() -> Result<PathBuf, std::io::Error> {
    let home = dirs::home_dir()
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "无法获取用户主目录"))?;
    Ok(home.join(ROOT_DIR_NAME))
}

fn join_sub(dir: &str) -> Result<PathBuf, std::io::Error> {
    Ok(root_dir()?.join(dir))
}

/// 组件实例目录名：`<组件>-<版本>`（多版本共存的物理基础）。
pub fn instance_dir_name(component: &str, version: &str) -> String {
    format!("{component}-{version}")
}

/// `components/`：所有组件解压目录的根。
pub fn components_dir() -> Result<PathBuf, std::io::Error> {
    join_sub(COMPONENTS_DIR)
}

/// `components/<component>/`：某组件的各版本父目录。
pub fn component_dir(component: &str) -> Result<PathBuf, std::io::Error> {
    Ok(components_dir()?.join(component))
}

/// `components/<component>/<component>-<version>/`：某实例的解压目录。
pub fn instance_dir(component: &str, version: &str) -> Result<PathBuf, std::io::Error> {
    Ok(component_dir(component)?.join(instance_dir_name(component, version)))
}

/// `var/`：可变运行时数据根目录。
pub fn var_dir() -> Result<PathBuf, std::io::Error> {
    join_sub(VAR_DIR)
}

/// `var/data/<component>/<component>-<version>/`：某实例的持久数据目录（HDFS 存储等）。
pub fn var_data_instance_dir(component: &str, version: &str) -> Result<PathBuf, std::io::Error> {
    Ok(var_dir()?
        .join(VAR_DATA_DIR)
        .join(component)
        .join(instance_dir_name(component, version)))
}

/// `var/log/<component>/<component>-<version>/`：某实例的运行日志目录。
pub fn var_log_instance_dir(component: &str, version: &str) -> Result<PathBuf, std::io::Error> {
    Ok(var_dir()?
        .join(VAR_LOG_DIR)
        .join(component)
        .join(instance_dir_name(component, version)))
}

/// `var/run/<component>-<version>.json`：某实例的进程记录文件。
pub fn runtime_file(component: &str, version: &str) -> Result<PathBuf, std::io::Error> {
    Ok(var_run_dir()?.join(format!("{}.json", instance_dir_name(component, version))))
}

/// `var/run/`：进程 PID / 状态记录目录。
pub fn var_run_dir() -> Result<PathBuf, std::io::Error> {
    Ok(var_dir()?.join(VAR_RUN_DIR))
}

/// `var/run/<component>/<component>-<version>/`：某实例的进程 pid 目录（Hadoop HADOOP_PID_DIR）。
pub fn var_run_instance_dir(component: &str, version: &str) -> Result<PathBuf, std::io::Error> {
    Ok(var_run_dir()?
        .join(component)
        .join(instance_dir_name(component, version)))
}

/// `var/downloads/`：安装包下载缓存（为组件服务）。
pub fn downloads_dir() -> Result<PathBuf, std::io::Error> {
    Ok(var_dir()?.join(VAR_DOWNLOADS_DIR))
}

/// `app/`：SoloStack 自身数据目录（设置 + 操作日志）。
pub fn app_dir() -> Result<PathBuf, std::io::Error> {
    join_sub(APP_DIR)
}

/// `app/log/`：SoloStack 自身操作日志目录。
pub fn app_log_dir() -> Result<PathBuf, std::io::Error> {
    Ok(app_dir()?.join(APP_LOG_DIR))
}

/// 设置文件路径：`~/.solostack/app/settings.json`。
pub fn settings_file() -> Result<PathBuf, std::io::Error> {
    Ok(app_dir()?.join(SETTINGS_FILE))
}

/// 确保 `~/.solostack/` 下所有顶层目录存在，返回根目录路径。
pub fn ensure_dirs() -> Result<PathBuf, std::io::Error> {
    let root = root_dir()?;
    for sub in [COMPONENTS_DIR, VAR_DIR, APP_DIR] {
        std::fs::create_dir_all(root.join(sub))?;
    }
    for sub in [VAR_DATA_DIR, VAR_LOG_DIR, VAR_RUN_DIR, VAR_DOWNLOADS_DIR] {
        std::fs::create_dir_all(root.join(VAR_DIR).join(sub))?;
    }
    std::fs::create_dir_all(root.join(APP_DIR).join(APP_LOG_DIR))?;
    Ok(root)
}

/// 校验 `path` 是否位于 `~/.solostack/` 内，防止路径逃逸。
pub fn is_within_root(path: &Path) -> Result<bool, std::io::Error> {
    // 先拒绝 `..` 组件：`Path::starts_with` 是纯词法比较，
    // `~/.solostack/var/log/x/../../../../etc/passwd` 仍会通过前缀检查，
    // 而系统调用层会解析 `..`，于是校验形同虚设。
    if path
        .components()
        .any(|c| c == std::path::Component::ParentDir)
    {
        return Ok(false);
    }
    let root = root_dir()?;
    Ok(path.starts_with(&root))
}

/// 一次性迁移旧版目录布局（幂等，best-effort）：
///
/// 旧布局 → 新布局：
/// - 根 `settings.json` → `app/settings.json`
/// - `var/solostack/*.log` → `app/log/`
/// - 根 `downloads/*` → `var/downloads/`
/// - 删除历史 `installs/`（含 `var/installs/`，临时 install.json 机制已废弃）
/// - 删除 `etc/`（配置副本机制已废弃，配置改为直接读写组件官方文件）
/// - 删除 `snapshots/`、`.templates/`（快照与模板文件已废弃）
///
/// 已存在的目标不覆盖；尽力而为，失败不阻塞初始化。
pub fn migrate_legacy_layout() -> Result<(), std::io::Error> {
    ensure_dirs()?;
    let root = root_dir()?;

    // 根 settings.json → app/settings.json
    // 历史 bug：save() 曾写到根目录、load() 读 app/，于是真值留在根目录而 app/ 里是个空文件。
    // 故不仅「目标不存在」时搬，目标为空白时也用根目录的值覆盖（覆盖空白文件不丢信息）。
    let legacy_settings = root.join(SETTINGS_FILE);
    if legacy_settings.is_file() {
        let target = settings_file()?;
        let target_blank = std::fs::read_to_string(&target)
            .map(|s| s.trim().is_empty())
            .unwrap_or(true);
        if !target.exists() || target_blank {
            let _ = std::fs::rename(&legacy_settings, &target);
        }
    }

    // var/solostack/*.log → app/log/
    let legacy_app_logs = root.join(VAR_DIR).join("solostack");
    if legacy_app_logs.is_dir() {
        if let Ok(entries) = std::fs::read_dir(&legacy_app_logs) {
            for e in entries.flatten() {
                let p = e.path();
                let name = e.file_name();
                let dest = app_log_dir()?.join(name);
                if p.is_file() && !dest.exists() {
                    let _ = std::fs::rename(&p, dest);
                }
            }
        }
        let _ = std::fs::remove_dir_all(&legacy_app_logs);
    }

    // 根 downloads/ → var/downloads/
    let legacy_downloads = root.join(VAR_DOWNLOADS_DIR);
    if legacy_downloads.is_dir() {
        let dest = downloads_dir()?;
        if let Ok(entries) = std::fs::read_dir(&legacy_downloads) {
            for e in entries.flatten() {
                let p = e.path();
                let target = dest.join(e.file_name());
                if !target.exists() {
                    let _ = std::fs::rename(&p, target);
                }
            }
        }
        let _ = std::fs::remove_dir_all(&legacy_downloads);
    }

    // 删除历史 installs 目录（根/ 与 var/，临时 install.json 机制已废弃，残留即清理）
    for d in [root.join("installs"), root.join(VAR_DIR).join("installs")] {
        if d.is_dir() {
            let _ = std::fs::remove_dir_all(&d);
        }
    }

    // 删除废弃的配置副本目录 etc/（配置现已就地写在组件实例内）
    let legacy_etc = root.join(LEGACY_ETC_DIR);
    if legacy_etc.is_dir() {
        let _ = std::fs::remove_dir_all(&legacy_etc);
    }

    // 删除废弃目录 snapshots/ 与 .templates/
    for d in ["snapshots", ".templates"] {
        let p = root.join(d);
        if p.is_dir() {
            let _ = std::fs::remove_dir_all(&p);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn instance_dir_nests_under_components() {
        use crate::test_util::HOME_LOCK;
        let _guard = HOME_LOCK.lock().unwrap();
        let dir = instance_dir("hadoop", "3.5.0").unwrap();
        assert_eq!(dir, components_dir().unwrap().join("hadoop/hadoop-3.5.0"));
    }

    #[test]
    fn new_layout_paths() {
        use crate::test_util::HOME_LOCK;
        let _guard = HOME_LOCK.lock().unwrap();
        let root = root_dir().unwrap();
        assert_eq!(settings_file().unwrap(), root.join("app/settings.json"));
        assert_eq!(downloads_dir().unwrap(), root.join("var/downloads"));
        assert_eq!(app_log_dir().unwrap(), root.join("app/log"));
    }

    #[test]
    fn var_nests_by_component_instance() {
        use crate::test_util::HOME_LOCK;
        let _guard = HOME_LOCK.lock().unwrap();
        let root = root_dir().unwrap();

        let data = var_data_instance_dir("hadoop", "3.5.0").unwrap();
        assert_eq!(data, root.join("var/data/hadoop/hadoop-3.5.0"));

        let log = var_log_instance_dir("hadoop", "3.5.0").unwrap();
        assert_eq!(log, root.join("var/log/hadoop/hadoop-3.5.0"));
    }

    #[test]
    fn runtime_file_uses_instance_name() {
        use crate::test_util::HOME_LOCK;
        let _guard = HOME_LOCK.lock().unwrap();
        let f = runtime_file("hadoop", "3.5.0").unwrap();
        assert_eq!(f, var_run_dir().unwrap().join("hadoop-3.5.0.json"));
    }

    #[test]
    fn ensure_dirs_creates_all_top_level() {
        use crate::test_util::HOME_LOCK;
        let _guard = HOME_LOCK.lock().unwrap();
        let tmp = std::env::temp_dir().join("solostack-paths-test");
        std::env::set_var("HOME", &tmp);
        let _ = std::fs::remove_dir_all(&tmp);

        let root = ensure_dirs().unwrap();
        for sub in [COMPONENTS_DIR, VAR_DIR, APP_DIR] {
            assert!(root.join(sub).is_dir(), "{sub} 未创建");
        }
        assert!(
            !root.join(LEGACY_ETC_DIR).exists(),
            "不应再创建已废弃的 etc/"
        );
        for sub in [VAR_DATA_DIR, VAR_LOG_DIR, VAR_RUN_DIR, VAR_DOWNLOADS_DIR] {
            assert!(root.join(VAR_DIR).join(sub).is_dir(), "var/{sub} 未创建");
        }
        assert!(
            root.join(APP_DIR).join(APP_LOG_DIR).is_dir(),
            "app/log 未创建"
        );

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn migrate_moves_legacy_layout() {
        use crate::test_util::HOME_LOCK;
        let _guard = HOME_LOCK.lock().unwrap();
        let tmp = std::env::temp_dir().join("solostack-migrate-test");
        std::env::set_var("HOME", &tmp);
        let _ = std::fs::remove_dir_all(&tmp);
        let root = tmp.join(ROOT_DIR_NAME);

        // 造旧布局
        std::fs::create_dir_all(root.join("downloads")).unwrap();
        std::fs::create_dir_all(root.join("installs")).unwrap();
        std::fs::create_dir_all(root.join("var/installs")).unwrap();
        std::fs::create_dir_all(root.join("etc/hadoop/hadoop-3.5.0")).unwrap();
        std::fs::create_dir_all(root.join("snapshots")).unwrap();
        std::fs::create_dir_all(root.join(".templates")).unwrap();
        std::fs::create_dir_all(root.join("var/solostack")).unwrap();
        std::fs::write(root.join("settings.json"), "{}").unwrap();
        std::fs::write(root.join("downloads/a.tgz"), "x").unwrap();
        std::fs::write(root.join("installs/hadoop-AB12-install.json"), "x").unwrap();
        std::fs::write(root.join("var/installs/kafka-CD34-install.json"), "x").unwrap();
        std::fs::write(root.join("etc/hadoop/hadoop-3.5.0/.detect-ports"), "9870").unwrap();
        std::fs::write(root.join("var/solostack/solostack-2026-01-01.log"), "x").unwrap();
        std::fs::write(root.join("snapshots/s.txt"), "x").unwrap();
        std::fs::write(root.join(".templates/hadoop.json"), "{}").unwrap();

        migrate_legacy_layout().unwrap();

        assert!(settings_file().unwrap().is_file(), "settings 应迁到 app/");
        assert!(!root.join("settings.json").exists());
        assert!(
            downloads_dir().unwrap().join("a.tgz").is_file(),
            "downloads 应迁到 var/"
        );
        assert!(!root.join("downloads").exists());
        assert!(!root.join("installs").exists(), "根 installs 应清理");
        assert!(!root.join("var/installs").exists(), "var/installs 应清理");
        assert!(
            !root.join(LEGACY_ETC_DIR).exists(),
            "配置副本 etc/（含 .detect-ports）应整块清理"
        );
        assert!(
            app_log_dir()
                .unwrap()
                .join("solostack-2026-01-01.log")
                .is_file(),
            "日志应迁到 app/log"
        );
        assert!(!root.join("var/solostack").exists());
        assert!(!root.join("snapshots").exists());
        assert!(!root.join(".templates").exists());

        // 幂等：再跑一次不报错、不重复
        migrate_legacy_layout().unwrap();

        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// 复现历史 bug 的现场：真值留在根 settings.json，app/ 下是个空文件。
    /// 迁移必须把真值救到 app/（否则用户的偏好永久失效）。
    #[test]
    fn migrate_rescues_settings_when_app_copy_is_blank() {
        use crate::test_util::HOME_LOCK;
        let _guard = HOME_LOCK.lock().unwrap();
        let tmp = std::env::temp_dir().join("solostack-migrate-settings-test");
        std::env::set_var("HOME", &tmp);
        let _ = std::fs::remove_dir_all(&tmp);

        std::fs::create_dir_all(app_dir().unwrap()).unwrap();
        std::fs::write(app_dir().unwrap().join(SETTINGS_FILE), "").unwrap();
        std::fs::write(
            root_dir().unwrap().join(SETTINGS_FILE),
            r#"{"log_viewer":"com.sublimetext.4"}"#,
        )
        .unwrap();

        migrate_legacy_layout().unwrap();

        let content = std::fs::read_to_string(settings_file().unwrap()).unwrap();
        assert!(content.contains("com.sublimetext.4"), "真值应被救到 app/");
        assert!(!root_dir().unwrap().join(SETTINGS_FILE).exists());

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn is_within_root_detects_escape() {
        use crate::test_util::HOME_LOCK;
        let _guard = HOME_LOCK.lock().unwrap();
        let inside = instance_dir("hadoop", "3.5.0").unwrap();
        assert!(is_within_root(&inside).unwrap());

        let outside = std::env::temp_dir();
        assert!(!is_within_root(&outside).unwrap());
    }

    #[test]
    fn root_dir_is_fixed_home_solostack() {
        use crate::test_util::HOME_LOCK;
        let _guard = HOME_LOCK.lock().unwrap();
        let tmp = std::env::temp_dir().join("solostack-root-test");
        std::env::set_var("HOME", &tmp);
        let _ = std::fs::remove_dir_all(&tmp);

        assert_eq!(root_dir().unwrap(), tmp.join(ROOT_DIR_NAME));
        // 首次初始化以 `app/settings.json` 是否存在为准（migrate / settings 依赖该约定）
        assert!(!settings_file().unwrap().exists());

        let _ = std::fs::remove_dir_all(&tmp);
    }
}
