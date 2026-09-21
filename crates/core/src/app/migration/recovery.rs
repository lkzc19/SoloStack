//! 迁移失败回滚与异常退出后的 staging 恢复。

use std::path::Path;

use super::fs_ops::restore_move;
use crate::app::environment::{self, Environment};
use crate::app::paths;

/// 回滚一个已迁移环境：把它移走的组件 / 数据 / 日志 / 运行态路径移回原位，
/// 再删除该环境目录。
pub(super) fn rollback_migrated_environment(
    root: &Path,
    environment: &Environment,
) -> Result<(), String> {
    for item in &environment.components {
        let instance_name = paths::instance_dir_name(&item.component, &item.version);
        restore_move(
            &paths::instance_dir(&environment.id, &item.component, &item.version)
                .map_err(|e| e.to_string())?,
            &root
                .join(paths::COMPONENTS_DIR)
                .join(&item.component)
                .join(&instance_name),
        )?;
        for kind in [paths::VAR_DATA_DIR, paths::VAR_LOG_DIR] {
            restore_move(
                &paths::var_dir(&environment.id)
                    .map_err(|e| e.to_string())?
                    .join(kind)
                    .join(&item.component)
                    .join(&instance_name),
                &root
                    .join(paths::VAR_DIR)
                    .join(kind)
                    .join(&item.component)
                    .join(&instance_name),
            )?;
        }
        restore_move(
            &paths::runtime_file(&environment.id, &item.component, &item.version)
                .map_err(|e| e.to_string())?,
            &root
                .join(paths::VAR_DIR)
                .join(paths::VAR_RUN_DIR)
                .join(format!("{instance_name}.json")),
        )?;
        restore_move(
            &paths::var_run_instance_dir(&environment.id, &item.component, &item.version)
                .map_err(|e| e.to_string())?,
            &root
                .join(paths::VAR_DIR)
                .join(paths::VAR_RUN_DIR)
                .join(&item.component)
                .join(&instance_name),
        )?;
    }
    std::fs::remove_dir_all(environment::directory(&environment.id)?)
        .map_err(|e| format!("删除已回滚迁移环境失败: {e}"))
}

/// 恢复上一次异常退出留下的 staging 目录。
///
/// staging 中的组件目录结构足以反推出原始位置；只要目标位置不存在，就把数据移回，
/// 绝不直接删除 staging，避免崩溃恢复时丢数据。
pub(super) fn recover_stale_migration(root: &Path, staging: &Path) -> Result<(), String> {
    let components_root = staging.join(paths::COMPONENTS_DIR);
    if components_root.is_dir() {
        for component_entry in
            std::fs::read_dir(&components_root).map_err(|e| format!("读取迁移残留目录失败: {e}"))?
        {
            let component_entry = component_entry.map_err(|e| e.to_string())?;
            if !component_entry.path().is_dir() {
                continue;
            }
            let component = component_entry.file_name().to_string_lossy().to_string();
            let prefix = format!("{component}-");
            for instance_entry in
                std::fs::read_dir(component_entry.path()).map_err(|e| e.to_string())?
            {
                let instance_entry = instance_entry.map_err(|e| e.to_string())?;
                if !instance_entry.path().is_dir() {
                    continue;
                }
                let instance_name = instance_entry.file_name().to_string_lossy().to_string();
                let Some(version) = instance_name.strip_prefix(&prefix) else {
                    continue;
                };
                if version.is_empty() {
                    continue;
                }

                restore_move(
                    &component_entry.path().join(&instance_name),
                    &root
                        .join(paths::COMPONENTS_DIR)
                        .join(&component)
                        .join(&instance_name),
                )?;
                for kind in [paths::VAR_DATA_DIR, paths::VAR_LOG_DIR] {
                    restore_move(
                        &staging
                            .join(paths::VAR_DIR)
                            .join(kind)
                            .join(&component)
                            .join(&instance_name),
                        &root
                            .join(paths::VAR_DIR)
                            .join(kind)
                            .join(&component)
                            .join(&instance_name),
                    )?;
                }
                restore_move(
                    &staging
                        .join(paths::VAR_DIR)
                        .join(paths::VAR_RUN_DIR)
                        .join(format!("{instance_name}.json")),
                    &root
                        .join(paths::VAR_DIR)
                        .join(paths::VAR_RUN_DIR)
                        .join(format!("{instance_name}.json")),
                )?;
                restore_move(
                    &staging
                        .join(paths::VAR_DIR)
                        .join(paths::VAR_RUN_DIR)
                        .join(&component)
                        .join(&instance_name),
                    &root
                        .join(paths::VAR_DIR)
                        .join(paths::VAR_RUN_DIR)
                        .join(&component)
                        .join(&instance_name),
                )?;
            }
        }
    }
    std::fs::remove_dir_all(staging)
        .map_err(|e| format!("清理迁移残留目录 {} 失败: {e}", staging.display()))
}
