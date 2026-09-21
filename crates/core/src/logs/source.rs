//! 日志来源：app 按天、组件单文件。
//!
//! app 与组件日志的差异（文件在哪、怎么枚举、是否结构化解析）全部收敛到这里，
//! 读取与跟随（`reader`）只依赖本模块给出的分片列表，不再各写一套。

use std::path::{Path, PathBuf};

use crate::app::{app_log, paths};

/// 一个可读取 / 跟随的日志来源。
#[derive(Debug, Clone)]
pub enum LogSource {
    /// app 操作日志。
    ///
    /// - `date = None`：实时（跟随当天最新文件，跨天自动切换）；
    /// - `date = Some(YYYY-MM-DD)`：看历史某天（静态，不分片合并）。
    App { date: Option<String> },
    /// 组件单个日志文件。
    Component {
        environment_id: String,
        component: String,
        version: String,
        file: PathBuf,
    },
}

impl LogSource {
    /// 该来源的全部分片（读序从旧到新）。
    pub fn parts(&self) -> Result<Vec<PathBuf>, String> {
        match self {
            LogSource::App { date } => {
                app_log::log_parts_for(date.as_deref().unwrap_or(&app_log::today()))
            }
            LogSource::Component {
                environment_id,
                component,
                version,
                file,
            } => Ok(vec![validated_component_file(
                environment_id,
                component,
                version,
                file,
            )?]),
        }
    }

    /// 实时跟随时要跟随的文件。
    ///
    /// app 实时来源取**当天的 base 文件**（`solostack.log.<今天>`，跨天自动切到新
    /// 一天）；历史日期取当天 base 文件；组件即所选文件。
    ///
    /// 刻意不走 `list_log_files().last()`：那按 (日期, 分片号) 升序排，末位是分片号
    /// 最大的**旧轮转片**，而当前文件是分片号 0。
    pub fn current_file(&self) -> Result<Option<PathBuf>, String> {
        match self {
            LogSource::App { date: None } => {
                let file = app_log::log_file()?;
                Ok(file.is_file().then_some(file))
            }
            LogSource::App { date: Some(date) } => {
                Ok(app_log::log_parts_for(date)?.into_iter().last())
            }
            LogSource::Component { .. } => Ok(self.parts()?.into_iter().last()),
        }
    }

    /// 是否支持实时跟随。只有"今天"的 app 日志与组件文件可跟随；历史日期静态。
    pub fn is_live(&self) -> bool {
        matches!(
            self,
            LogSource::App { date: None } | LogSource::Component { .. }
        )
    }

    /// 组件来源关联的环境 ID（app 日志为 None）。
    pub fn environment_id(&self) -> Option<&str> {
        match self {
            LogSource::Component { environment_id, .. } => Some(environment_id),
            LogSource::App { .. } => None,
        }
    }

    /// 是否按结构化 JSONL 解析（app 日志为 true）。
    pub fn structured(&self) -> bool {
        matches!(self, LogSource::App { .. })
    }

    /// app 来源的日期（实时来源取今天）；组件来源返回 None。
    pub fn app_date(&self) -> Option<String> {
        match self {
            LogSource::App { date } => {
                Some(date.clone().unwrap_or_else(app_log::today))
            }
            LogSource::Component { .. } => None,
        }
    }
}

/// 列出某组件可查看的日志文件（按修改时间倒序）。
pub fn list_component_files(
    environment_id: &str,
    name: &str,
    version: &str,
) -> Result<Vec<PathBuf>, String> {
    let dir =
        paths::var_log_instance_dir(environment_id, name, version).map_err(|e| e.to_string())?;
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut files: Vec<(std::time::SystemTime, PathBuf)> = std::fs::read_dir(&dir)
        .map_err(|e| e.to_string())?
        .flatten()
        .filter(|e| e.path().is_file())
        .filter(|e| {
            e.path()
                .extension()
                .and_then(|x| x.to_str())
                .is_some_and(|ext| matches!(ext, "log" | "out" | "err"))
        })
        .map(|e| {
            let mtime = e
                .metadata()
                .and_then(|m| m.modified())
                .unwrap_or(std::time::UNIX_EPOCH);
            (mtime, e.path())
        })
        .collect();
    files.sort_by_key(|f| std::cmp::Reverse(f.0)); // 新的在前
    Ok(files.into_iter().map(|(_, p)| p).collect())
}

/// 校验并返回可安全打开的组件日志文件（唯一路径守卫）。
///
/// 同时检查组件归属、日志扩展名、真实文件和符号链接目标，防止通过日志页
/// 读取组件目录之外的文件。
pub fn validated_component_file(
    environment_id: &str,
    name: &str,
    version: &str,
    path: &Path,
) -> Result<PathBuf, String> {
    if !is_component_file(environment_id, name, version, path) {
        return Err(format!("日志路径不属于组件 {name}"));
    }
    let valid_extension = path
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| matches!(ext, "log" | "out" | "err"));
    if !valid_extension {
        return Err(format!("不支持的日志文件类型: {}", path.display()));
    }
    if !path.is_file() {
        return Err(format!("日志文件不存在: {}", path.display()));
    }

    let root = paths::var_log_instance_dir(environment_id, name, version)
        .map_err(|e| e.to_string())?
        .canonicalize()
        .map_err(|e| format!("解析日志目录失败: {e}"))?;
    let canonical = path
        .canonicalize()
        .map_err(|e| format!("解析日志路径失败: {e}"))?;
    if !canonical.starts_with(&root) {
        return Err(format!("日志路径超出组件日志目录: {}", path.display()));
    }
    Ok(canonical)
}

/// 路径是否位于该组件实例的日志目录内（词法校验，`..` 显式拒绝）。
fn is_component_file(environment_id: &str, name: &str, version: &str, path: &Path) -> bool {
    // 词法前缀比较挡不住 `..`，必须显式拒绝
    if path.components().any(|c| c == std::path::Component::ParentDir) {
        return false;
    }
    paths::var_log_instance_dir(environment_id, name, version)
        .map(|root| path.starts_with(&root))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ENV_ID: &str = "00000000-0000-4000-8000-000000000001";

    /// 回归：词法前缀比较挡不住 `..`，必须在校验层显式拒绝。
    #[test]
    fn path_guard_rejects_parent_dir_components() {
        use crate::test_util::HOME_LOCK;
        let _guard = HOME_LOCK.lock().unwrap();
        let tmp = std::env::temp_dir().join("solostack-logs-source-traversal");
        std::env::set_var("HOME", &tmp);

        let log_root = paths::var_log_instance_dir(ENV_ID, "hadoop", "3.5.0").unwrap();
        let evil = log_root.join("../../../../../../etc/passwd");
        assert!(
            !is_component_file(ENV_ID, "hadoop", "3.5.0", &evil),
            "含 `..` 的路径必须被拒"
        );
        assert!(is_component_file(
            ENV_ID,
            "hadoop",
            "3.5.0",
            &log_root.join("hadoop.log")
        ));

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn validated_component_file_rejects_cross_component_and_wrong_extension() {
        use crate::test_util::HOME_LOCK;
        let _guard = HOME_LOCK.lock().unwrap();
        let tmp = std::env::temp_dir().join("solostack-logs-source-validated");
        std::env::set_var("HOME", &tmp);
        let _ = std::fs::remove_dir_all(&tmp);

        let hadoop = paths::var_log_instance_dir(ENV_ID, "hadoop", "3.5.0").unwrap();
        let kafka = paths::var_log_instance_dir(ENV_ID, "kafka", "4.3.1").unwrap();
        std::fs::create_dir_all(&hadoop).unwrap();
        std::fs::create_dir_all(&kafka).unwrap();
        std::fs::write(hadoop.join("hadoop.log"), "ok\n").unwrap();
        std::fs::write(kafka.join("kafka.log"), "ok\n").unwrap();
        std::fs::write(hadoop.join("secret.txt"), "no\n").unwrap();

        assert!(
            validated_component_file(ENV_ID, "hadoop", "3.5.0", &hadoop.join("hadoop.log")).is_ok()
        );
        assert!(
            validated_component_file(ENV_ID, "hadoop", "3.5.0", &kafka.join("kafka.log")).is_err()
        );
        assert!(
            validated_component_file(ENV_ID, "hadoop", "3.5.0", &hadoop.join("secret.txt")).is_err()
        );

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[cfg(unix)]
    #[test]
    fn validated_component_file_rejects_symlink_escape() {
        use crate::test_util::HOME_LOCK;
        let _guard = HOME_LOCK.lock().unwrap();
        let tmp = std::env::temp_dir().join("solostack-logs-source-symlink");
        std::env::set_var("HOME", &tmp);
        let _ = std::fs::remove_dir_all(&tmp);

        let root = paths::var_log_instance_dir(ENV_ID, "hadoop", "3.5.0").unwrap();
        std::fs::create_dir_all(&root).unwrap();
        let outside = tmp.join("outside.log");
        std::fs::write(&outside, "secret\n").unwrap();
        std::os::unix::fs::symlink(&outside, root.join("escape.log")).unwrap();

        assert!(
            validated_component_file(ENV_ID, "hadoop", "3.5.0", &root.join("escape.log")).is_err()
        );

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn list_component_files_only_reads_instance_log_dir() {
        use crate::test_util::HOME_LOCK;
        let _guard = HOME_LOCK.lock().unwrap();
        let tmp = std::env::temp_dir().join("solostack-logs-source-list");
        std::env::set_var("HOME", &tmp);
        let _ = std::fs::remove_dir_all(&tmp);

        let log_dir = paths::var_log_instance_dir(ENV_ID, "hadoop", "3.5.0").unwrap();
        std::fs::create_dir_all(&log_dir).unwrap();
        std::fs::write(log_dir.join("hadoop-nn.log"), "x\n").unwrap();

        let files = list_component_files(ENV_ID, "hadoop", "3.5.0").unwrap();
        assert_eq!(files.len(), 1);

        let _ = std::fs::remove_dir_all(&tmp);
    }
}
