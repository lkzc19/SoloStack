//! 环境磁盘占用统计。

use std::path::Path;

use serde::Serialize;

use super::{environment, paths};

/// 一个环境的磁盘占用，按受管用途拆分。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct EnvironmentUsage {
    pub components_bytes: u64,
    pub data_bytes: u64,
    pub log_bytes: u64,
    pub runtime_bytes: u64,
    pub other_bytes: u64,
    pub total_bytes: u64,
}

/// 统计环境目录；共享下载缓存不属于环境，不在这里计算。
pub fn measure(environment_id: &str) -> Result<EnvironmentUsage, String> {
    let root = environment::directory(environment_id)?;
    let components = paths::components_dir(environment_id).map_err(|e| e.to_string())?;
    let var = paths::var_dir(environment_id).map_err(|e| e.to_string())?;
    let data = var.join(paths::VAR_DATA_DIR);
    let log = var.join(paths::VAR_LOG_DIR);
    let runtime = var.join(paths::VAR_RUN_DIR);

    let components_bytes = path_size(&components)?;
    let data_bytes = path_size(&data)?;
    let log_bytes = path_size(&log)?;
    let runtime_bytes = path_size(&runtime)?;
    let total_bytes = path_size(&root)?;
    let known = components_bytes
        .saturating_add(data_bytes)
        .saturating_add(log_bytes)
        .saturating_add(runtime_bytes);

    Ok(EnvironmentUsage {
        components_bytes,
        data_bytes,
        log_bytes,
        runtime_bytes,
        other_bytes: total_bytes.saturating_sub(known),
        total_bytes,
    })
}

/// 统计路径占用。符号链接不跟随，避免把环境外文件算进来。
fn path_size(path: &Path) -> Result<u64, String> {
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(error) => {
            return Err(format!("读取磁盘占用 {} 失败: {error}", path.display()));
        }
    };
    let file_type = metadata.file_type();
    if file_type.is_symlink() {
        return Ok(0);
    }
    if file_type.is_file() {
        return Ok(metadata.len());
    }
    if !file_type.is_dir() {
        return Ok(0);
    }

    let mut total = 0u64;
    for entry in std::fs::read_dir(path)
        .map_err(|error| format!("读取目录 {} 失败: {error}", path.display()))?
    {
        let entry = entry.map_err(|error| format!("读取目录项失败: {error}"))?;
        total = total.saturating_add(path_size(&entry.path())?);
    }
    Ok(total)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn setup(name: &str) -> (PathBuf, environment::Environment) {
        let tmp = std::env::temp_dir().join(format!("solostack-environment-usage-{name}"));
        std::env::set_var("HOME", &tmp);
        let _ = std::fs::remove_dir_all(&tmp);
        let environment = environment::create("默认环境").unwrap();
        (tmp, environment)
    }

    #[test]
    fn usage_splits_managed_directories() {
        use crate::test_util::HOME_LOCK;
        let _guard = HOME_LOCK.lock().unwrap();
        let (tmp, environment) = setup("split");

        let files = [
            (
                paths::instance_dir(&environment.id, "hadoop", "3.5.0")
                    .unwrap()
                    .join("bin/hdfs"),
                11u64,
            ),
            (
                paths::var_data_instance_dir(&environment.id, "hadoop", "3.5.0")
                    .unwrap()
                    .join("block"),
                22u64,
            ),
            (
                paths::var_log_instance_dir(&environment.id, "hadoop", "3.5.0")
                    .unwrap()
                    .join("hadoop.log"),
                33u64,
            ),
            (
                paths::var_run_instance_dir(&environment.id, "hadoop", "3.5.0")
                    .unwrap()
                    .join("namenode.pid"),
                44u64,
            ),
        ];
        for (path, size) in files {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, vec![b'x'; size as usize]).unwrap();
        }

        let usage = measure(&environment.id).unwrap();
        assert_eq!(usage.components_bytes, 11);
        assert_eq!(usage.data_bytes, 22);
        assert_eq!(usage.log_bytes, 33);
        assert_eq!(usage.runtime_bytes, 44);
        assert!(usage.other_bytes >= environment_metadata_size(&environment.id));
        assert_eq!(
            usage.total_bytes,
            usage.components_bytes
                + usage.data_bytes
                + usage.log_bytes
                + usage.runtime_bytes
                + usage.other_bytes
        );

        let _ = std::fs::remove_dir_all(&tmp);
    }

    fn environment_metadata_size(environment_id: &str) -> u64 {
        environment::metadata_path(environment_id)
            .and_then(|path| {
                std::fs::metadata(path)
                    .map(|metadata| metadata.len())
                    .map_err(|error| error.to_string())
            })
            .unwrap_or(0)
    }
}
