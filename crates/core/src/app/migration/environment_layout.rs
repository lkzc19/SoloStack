//! 把旧版全局 `components/` 与 `var/` 迁移到环境目录。

use std::collections::BTreeMap;
use std::path::Path;

use super::fs_ops::move_if_exists;
use super::legacy::{scan_legacy_instances, LegacyInstance};
use super::recovery::{recover_stale_migration, rollback_migrated_environment};
use crate::app::environment::{self, Environment, EnvironmentComponent, SCHEMA_VERSION};
use crate::app::{id, paths};

fn now() -> String {
    chrono::Local::now().to_rfc3339()
}

/// 把旧版全局 `components/` 与 `var/` 迁移到环境目录。
///
/// 迁移规则：
/// - 不同组件、且同组件只有一个版本时，可以一起进入默认环境。
/// - 同一组件存在多个版本时，额外版本拆到独立环境。
/// - 使用 staging 目录，失败时回滚已移动路径。
pub fn migrate_environment_layout() -> Result<Vec<Environment>, String> {
    paths::ensure_app_dirs().map_err(|e| e.to_string())?;
    let root = paths::root_dir().map_err(|e| e.to_string())?;
    let environments_root = paths::environments_dir().map_err(|e| e.to_string())?;
    if let Ok(entries) = std::fs::read_dir(&environments_root) {
        for entry in entries.flatten() {
            if entry
                .file_name()
                .to_string_lossy()
                .starts_with(".migration-")
            {
                recover_stale_migration(&root, &entry.path())?;
            }
        }
    }
    let instances = scan_legacy_instances(&root)?;
    let existing = environment::list()?;
    if !existing.is_empty() {
        if instances.is_empty() {
            return Ok(Vec::new());
        }
        return Err(
            "检测到环境目录与旧组件目录同时存在，可能是上次迁移未完成；请保留数据并检查迁移日志"
                .to_string(),
        );
    }
    if instances.is_empty() {
        return Ok(Vec::new());
    }

    let mut grouped: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for instance in instances {
        grouped
            .entry(instance.component)
            .or_default()
            .push(instance.version);
    }

    let mut groups: Vec<Vec<LegacyInstance>> = vec![Vec::new()];
    for (component, mut versions) in grouped {
        versions.sort();
        versions.dedup();
        let mut versions = versions.into_iter();
        if let Some(first) = versions.next() {
            groups[0].push(LegacyInstance {
                component: component.clone(),
                version: first,
            });
        }
        for version in versions {
            groups.push(vec![LegacyInstance {
                component: component.clone(),
                version,
            }]);
        }
    }
    groups.retain(|group| !group.is_empty());

    let mut migrated = Vec::new();
    for (index, group) in groups.into_iter().enumerate() {
        let name = if index == 0 {
            "默认环境".to_string()
        } else if group.len() == 1 {
            format!("{} {}", group[0].component, group[0].version)
        } else {
            format!("迁移环境 {}", index + 1)
        };
        match migrate_group(&root, &name, &group) {
            Ok(environment) => migrated.push(environment),
            Err(error) => {
                let mut rollback_errors = Vec::new();
                for environment in migrated.iter().rev() {
                    if let Err(rollback_error) = rollback_migrated_environment(&root, environment) {
                        rollback_errors.push(rollback_error);
                    }
                }
                if rollback_errors.is_empty() {
                    return Err(error);
                }
                return Err(format!(
                    "{error}；迁移回滚失败: {}",
                    rollback_errors.join("；")
                ));
            }
        }
    }

    if let Some(first) = migrated.first() {
        environment::set_active_id(Some(&first.id))?;
    }
    Ok(migrated)
}

/// 把一组旧实例移动进一个新建环境（staging → 提交，失败时回收已移动路径）。
fn migrate_group(root: &Path, name: &str, group: &[LegacyInstance]) -> Result<Environment, String> {
    let id = id::new_id();
    let environments_root = paths::environments_dir().map_err(|e| e.to_string())?;
    let staging = environments_root.join(format!(".migration-{id}"));
    if staging.exists() {
        std::fs::remove_dir_all(&staging).map_err(|e| e.to_string())?;
    }
    for dir in [
        staging.join(paths::COMPONENTS_DIR),
        staging.join(paths::VAR_DIR).join(paths::VAR_DATA_DIR),
        staging.join(paths::VAR_DIR).join(paths::VAR_LOG_DIR),
        staging.join(paths::VAR_DIR).join(paths::VAR_RUN_DIR),
    ] {
        std::fs::create_dir_all(&dir)
            .map_err(|e| format!("创建迁移目录 {} 失败: {e}", dir.display()))?;
    }

    let mut moves = Vec::new();
    let result = (|| {
        for instance in group {
            let instance_name = paths::instance_dir_name(&instance.component, &instance.version);
            let old_component = root
                .join(paths::COMPONENTS_DIR)
                .join(&instance.component)
                .join(&instance_name);
            let new_component = staging
                .join(paths::COMPONENTS_DIR)
                .join(&instance.component)
                .join(&instance_name);
            move_if_exists(&old_component, &new_component, &mut moves)?;

            for kind in [paths::VAR_DATA_DIR, paths::VAR_LOG_DIR] {
                let old = root
                    .join(paths::VAR_DIR)
                    .join(kind)
                    .join(&instance.component)
                    .join(&instance_name);
                let new = staging
                    .join(paths::VAR_DIR)
                    .join(kind)
                    .join(&instance.component)
                    .join(&instance_name);
                move_if_exists(&old, &new, &mut moves)?;
            }

            let old_runtime = root
                .join(paths::VAR_DIR)
                .join(paths::VAR_RUN_DIR)
                .join(format!("{instance_name}.json"));
            let new_runtime = staging
                .join(paths::VAR_DIR)
                .join(paths::VAR_RUN_DIR)
                .join(format!("{instance_name}.json"));
            move_if_exists(&old_runtime, &new_runtime, &mut moves)?;

            let old_pid_dir = root
                .join(paths::VAR_DIR)
                .join(paths::VAR_RUN_DIR)
                .join(&instance.component)
                .join(&instance_name);
            let new_pid_dir = staging
                .join(paths::VAR_DIR)
                .join(paths::VAR_RUN_DIR)
                .join(&instance.component)
                .join(&instance_name);
            move_if_exists(&old_pid_dir, &new_pid_dir, &mut moves)?;
        }

        let timestamp = now();
        let environment = Environment {
            schema_version: SCHEMA_VERSION,
            id: id.clone(),
            name: name.to_string(),
            components: group
                .iter()
                .map(|instance| EnvironmentComponent {
                    component: instance.component.clone(),
                    version: instance.version.clone(),
                    installed_at: timestamp.clone(),
                })
                .collect(),
            created_at: timestamp.clone(),
            updated_at: timestamp,
            last_activated_at: None,
        };
        environment::validate_components(&environment.components)?;
        let content = serde_json::to_string_pretty(&environment)
            .map_err(|e| format!("序列化迁移环境失败: {e}"))?;
        std::fs::write(staging.join(paths::ENVIRONMENT_FILE), content)
            .map_err(|e| format!("写入迁移环境元数据失败: {e}"))?;
        let final_dir = paths::environment_dir(&id).map_err(|e| e.to_string())?;
        std::fs::rename(&staging, &final_dir)
            .map_err(|e| format!("提交迁移环境 {} 失败: {e}", final_dir.display()))?;
        Ok(environment)
    })();

    if result.is_err() {
        for (source, destination) in moves.into_iter().rev() {
            if destination.exists() && !source.exists() {
                if let Some(parent) = source.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                let _ = std::fs::rename(destination, source);
            }
        }
        let _ = std::fs::remove_dir_all(&staging);
    }
    result
}
