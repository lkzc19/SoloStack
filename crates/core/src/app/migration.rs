//! SoloStack 旧目录布局迁移。
//!
//! 迁移分为应用级目录和多环境组件目录两部分；组件移动使用 staging、
//! 失败回滚和异常退出恢复，确保旧数据不会被半迁移状态吞掉。

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use super::environment::{self, Environment, EnvironmentComponent, SCHEMA_VERSION};
use super::{id, paths};

const LEGACY_ETC_DIR: &str = "etc";

fn now() -> String {
    chrono::Local::now().to_rfc3339()
}

/// 迁移旧版应用级布局（幂等）：
/// - 根/旧位置 settings.json → app/settings.json
/// - 旧应用日志 → app/log/
/// - 根 downloads/ 与 var/downloads/ → cache/downloads/
/// - 清理废弃的 installs / etc / snapshots / .templates
pub fn migrate_app_layout() -> Result<(), std::io::Error> {
    paths::ensure_app_dirs()?;
    let root = paths::root_dir()?;

    let legacy_settings = root.join(paths::SETTINGS_FILE);
    if legacy_settings.is_file() {
        let target = paths::settings_file()?;
        let target_blank = std::fs::read_to_string(&target)
            .map(|content| content.trim().is_empty())
            .unwrap_or(true);
        if !target.exists() || target_blank {
            let _ = std::fs::rename(&legacy_settings, &target);
        }
    }

    let legacy_app_logs = root.join(paths::VAR_DIR).join("solostack");
    if legacy_app_logs.is_dir() {
        if let Ok(entries) = std::fs::read_dir(&legacy_app_logs) {
            for entry in entries.flatten() {
                let path = entry.path();
                let destination = paths::app_log_dir()?.join(entry.file_name());
                if path.is_file() && !destination.exists() {
                    let _ = std::fs::rename(&path, destination);
                }
            }
        }
        let _ = std::fs::remove_dir_all(&legacy_app_logs);
    }

    for legacy_downloads in [
        root.join(paths::DOWNLOADS_DIR),
        root.join(paths::VAR_DIR).join(paths::DOWNLOADS_DIR),
    ] {
        if legacy_downloads.is_dir() {
            let destination = paths::downloads_dir()?;
            if let Ok(entries) = std::fs::read_dir(&legacy_downloads) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    let target = destination.join(entry.file_name());
                    if !target.exists() {
                        let _ = std::fs::rename(&path, target);
                    }
                }
            }
            let _ = std::fs::remove_dir_all(&legacy_downloads);
        }
    }

    for dir in [
        root.join("installs"),
        root.join(paths::VAR_DIR).join("installs"),
    ] {
        if dir.is_dir() {
            let _ = std::fs::remove_dir_all(&dir);
        }
    }
    for legacy in [LEGACY_ETC_DIR, "snapshots", ".templates"] {
        let path = root.join(legacy);
        if path.is_dir() {
            let _ = std::fs::remove_dir_all(&path);
        }
    }

    Ok(())
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

fn rollback_migrated_environment(root: &Path, environment: &Environment) -> Result<(), String> {
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
fn recover_stale_migration(root: &Path, staging: &Path) -> Result<(), String> {
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

fn restore_move(staged: &Path, original: &Path) -> Result<(), String> {
    if !staged.exists() {
        return Ok(());
    }
    if original.exists() {
        return Err(format!(
            "迁移恢复冲突，源路径和目标路径同时存在: {} / {}",
            original.display(),
            staged.display()
        ));
    }
    if let Some(parent) = original.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("创建迁移恢复目录 {} 失败: {e}", parent.display()))?;
    }
    std::fs::rename(staged, original).map_err(|e| {
        format!(
            "恢复迁移路径 {} 到 {} 失败: {e}",
            staged.display(),
            original.display()
        )
    })
}

#[derive(Debug, Clone)]
struct LegacyInstance {
    component: String,
    version: String,
}

fn scan_legacy_instances(root: &Path) -> Result<Vec<LegacyInstance>, String> {
    let components_root = root.join(paths::COMPONENTS_DIR);
    if !components_root.is_dir() {
        return Ok(Vec::new());
    }
    let mut instances = Vec::new();
    for component_entry in
        std::fs::read_dir(&components_root).map_err(|e| format!("读取旧组件目录失败: {e}"))?
    {
        let component_entry = component_entry.map_err(|e| e.to_string())?;
        if !component_entry.path().is_dir() {
            continue;
        }
        let component = component_entry.file_name().to_string_lossy().to_string();
        let prefix = format!("{component}-");
        for version_entry in std::fs::read_dir(component_entry.path())
            .map_err(|e| format!("读取旧组件 {} 目录失败: {e}", component))?
        {
            let version_entry = version_entry.map_err(|e| e.to_string())?;
            if !version_entry.path().is_dir() {
                continue;
            }
            let directory_name = version_entry.file_name().to_string_lossy().to_string();
            if let Some(version) = directory_name.strip_prefix(&prefix) {
                if !version.is_empty() {
                    instances.push(LegacyInstance {
                        component: component.clone(),
                        version: version.to_string(),
                    });
                }
            }
        }
    }
    Ok(instances)
}

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

fn move_if_exists(
    source: &Path,
    destination: &Path,
    moves: &mut Vec<(PathBuf, PathBuf)>,
) -> Result<(), String> {
    if !source.exists() {
        return Ok(());
    }
    if destination.exists() {
        return Err(format!("迁移目标已存在: {}", destination.display()));
    }
    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("创建迁移目标目录 {} 失败: {e}", parent.display()))?;
    }
    std::fs::rename(source, destination).map_err(|e| {
        format!(
            "迁移 {} 到 {} 失败: {e}",
            source.display(),
            destination.display()
        )
    })?;
    moves.push((source.to_path_buf(), destination.to_path_buf()));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup(name: &str) -> PathBuf {
        let tmp = std::env::temp_dir().join(format!("solostack-migration-{name}"));
        std::env::set_var("HOME", &tmp);
        let _ = std::fs::remove_dir_all(&tmp);
        tmp
    }

    fn write_fake_hadoop(
        instance: &Path,
        jdk: &Path,
        namenode_web_port: Option<u16>,
        yarn_exit_code: u8,
    ) {
        let config = instance.join("etc/hadoop");
        let bin = instance.join("bin");
        std::fs::create_dir_all(&config).unwrap();
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::create_dir_all(jdk).unwrap();

        for file in [
            "core-site.xml",
            "yarn-site.xml",
            "mapred-site.xml",
            "workers",
        ] {
            let content = if file == "workers" {
                "localhost\n".to_string()
            } else {
                "<?xml version=\"1.0\"?>\n<configuration>\n</configuration>\n".to_string()
            };
            std::fs::write(config.join(file), content).unwrap();
        }
        let namenode_port = namenode_web_port
            .map(|port| {
                format!(
                    "  <property>\n    <name>dfs.namenode.http-address</name>\n    <value>localhost:{port}</value>\n  </property>\n"
                )
            })
            .unwrap_or_default();
        std::fs::write(
            config.join("hdfs-site.xml"),
            format!("<?xml version=\"1.0\"?>\n<configuration>\n{namenode_port}</configuration>\n"),
        )
        .unwrap();
        std::fs::write(
            config.join("hadoop-env.sh"),
            format!("export JAVA_HOME={}\n", jdk.display()),
        )
        .unwrap();
        std::fs::write(bin.join("hdfs"), "exit 0\n").unwrap();
        std::fs::write(bin.join("yarn"), format!("exit {yarn_exit_code}\n")).unwrap();
    }

    fn create_legacy_hadoop(root: &Path, jdk: &Path, version: &str) {
        let instance_name = format!("hadoop-{version}");
        let instance = root
            .join(paths::COMPONENTS_DIR)
            .join("hadoop")
            .join(&instance_name);
        write_fake_hadoop(&instance, jdk, None, 0);
        std::fs::create_dir_all(
            root.join("var/data/hadoop")
                .join(&instance_name)
                .join("name/current"),
        )
        .unwrap();
        std::fs::write(
            root.join("var/data/hadoop")
                .join(&instance_name)
                .join("name/current/VERSION"),
            format!("namespaceID={version}\n"),
        )
        .unwrap();
        std::fs::create_dir_all(root.join("var/log/hadoop").join(&instance_name)).unwrap();
        std::fs::write(
            root.join("var/log/hadoop")
                .join(&instance_name)
                .join("hadoop.log"),
            "legacy log\n",
        )
        .unwrap();
        std::fs::create_dir_all(root.join("var/run/hadoop").join(&instance_name)).unwrap();
        std::fs::write(
            root.join("var/run/hadoop").join(&instance_name).join("pid"),
            "legacy pid\n",
        )
        .unwrap();
    }

    #[test]
    fn migrates_app_level_layout_idempotently() {
        use crate::test_util::HOME_LOCK;
        let _guard = HOME_LOCK.lock().unwrap();
        let tmp = setup("app-layout");
        let root = tmp.join(paths::ROOT_DIR_NAME);

        std::fs::create_dir_all(root.join("downloads")).unwrap();
        std::fs::create_dir_all(root.join("var/downloads")).unwrap();
        std::fs::create_dir_all(root.join("var/solostack")).unwrap();
        std::fs::create_dir_all(root.join("installs")).unwrap();
        std::fs::create_dir_all(root.join("etc/hadoop/hadoop-3.5.0")).unwrap();
        std::fs::write(root.join("settings.json"), "{}").unwrap();
        std::fs::write(root.join("downloads/a.tgz"), "a").unwrap();
        std::fs::write(root.join("var/downloads/b.tgz"), "b").unwrap();
        std::fs::write(root.join("var/solostack/solostack.log"), "log").unwrap();

        migrate_app_layout().unwrap();

        assert!(paths::settings_file().unwrap().is_file());
        assert!(paths::downloads_dir().unwrap().join("a.tgz").is_file());
        assert!(paths::downloads_dir().unwrap().join("b.tgz").is_file());
        assert!(paths::app_log_dir()
            .unwrap()
            .join("solostack.log")
            .is_file());
        assert!(!root.join("etc").exists());
        assert!(!root.join("installs").exists());

        migrate_app_layout().unwrap();
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn legacy_components_migrate_into_one_environment_when_versions_are_unique() {
        use crate::test_util::HOME_LOCK;
        let _guard = HOME_LOCK.lock().unwrap();
        let tmp = setup("legacy-unique");
        let root = tmp.join(paths::ROOT_DIR_NAME);
        std::fs::create_dir_all(root.join("components/hadoop/hadoop-3.5.0")).unwrap();
        std::fs::create_dir_all(root.join("components/kafka/kafka-4.3.1")).unwrap();
        std::fs::create_dir_all(root.join("var/data/hadoop/hadoop-3.5.0")).unwrap();
        std::fs::write(root.join("var/data/hadoop/hadoop-3.5.0/value"), "data").unwrap();

        let migrated = migrate_environment_layout().unwrap();
        assert_eq!(migrated.len(), 1);
        assert_eq!(migrated[0].components.len(), 2);
        assert!(paths::instance_dir(&migrated[0].id, "hadoop", "3.5.0")
            .unwrap()
            .is_dir());
        assert!(
            paths::var_data_instance_dir(&migrated[0].id, "hadoop", "3.5.0")
                .unwrap()
                .join("value")
                .is_file()
        );

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn duplicate_legacy_component_versions_split_and_remain_manageable() {
        use crate::component::{schema, ConfigFieldUpdate};
        use crate::lifecycle::{service, uninstall};
        use crate::test_util::HOME_LOCK;
        let _guard = HOME_LOCK.lock().unwrap();
        let tmp = setup("legacy-duplicates");
        let root = tmp.join(paths::ROOT_DIR_NAME);
        let jdk = tmp.join("fake-jdk");
        create_legacy_hadoop(&root, &jdk, "3.5.0");
        create_legacy_hadoop(&root, &jdk, "3.4.1");

        let migrated = migrate_environment_layout().unwrap();
        assert_eq!(migrated.len(), 2);
        assert!(migrated
            .iter()
            .all(|environment| environment.components.len() == 1));

        for (index, environment) in migrated.iter().enumerate() {
            let version = environment.components[0].version.as_str();
            environment::set_active_id(Some(&environment.id)).unwrap();
            schema::save_fields(
                &environment.id,
                "hadoop",
                version,
                &[ConfigFieldUpdate {
                    id: "namenode_web_port".to_string(),
                    value: (19870 + index as u16).to_string(),
                }],
            )
            .unwrap();
            service::start(&environment.id, "hadoop", version).unwrap();
            service::stop(&environment.id, "hadoop", version).unwrap();
            uninstall::uninstall(&environment.id, "hadoop", version, false).unwrap();
            assert!(environment::load(&environment.id)
                .unwrap()
                .components
                .is_empty());
        }

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn stale_migration_is_restored_instead_of_deleted() {
        use crate::test_util::HOME_LOCK;
        let _guard = HOME_LOCK.lock().unwrap();
        let tmp = setup("stale");
        let root = tmp.join(paths::ROOT_DIR_NAME);
        let staging = root.join("environments/.migration-test");
        let staged_component = staging.join("components/hadoop/hadoop-3.5.0");
        std::fs::create_dir_all(&staged_component).unwrap();
        std::fs::write(staged_component.join("marker"), "data").unwrap();

        recover_stale_migration(&root, &staging).unwrap();

        assert!(root.join("components/hadoop/hadoop-3.5.0/marker").is_file());
        assert!(!staging.exists());

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn migrated_component_supports_config_start_stop_and_uninstall() {
        use crate::component::{schema, ConfigFieldUpdate};
        use crate::lifecycle::{service, uninstall};
        use crate::test_util::HOME_LOCK;
        let _guard = HOME_LOCK.lock().unwrap();
        let tmp = setup("lifecycle");
        let root = tmp.join(paths::ROOT_DIR_NAME);
        let jdk = tmp.join("fake-jdk");
        create_legacy_hadoop(&root, &jdk, "3.5.0");

        let migrated = migrate_environment_layout().unwrap();
        assert_eq!(migrated.len(), 1);
        let environment = &migrated[0];
        environment::set_active_id(Some(&environment.id)).unwrap();

        let migrated_instance = paths::instance_dir(&environment.id, "hadoop", "3.5.0").unwrap();
        assert!(migrated_instance.join("etc/hadoop/hadoop-env.sh").is_file());
        assert!(
            paths::var_data_instance_dir(&environment.id, "hadoop", "3.5.0")
                .unwrap()
                .join("name/current/VERSION")
                .is_file()
        );
        assert!(
            paths::var_log_instance_dir(&environment.id, "hadoop", "3.5.0")
                .unwrap()
                .join("hadoop.log")
                .is_file()
        );

        let fields = schema::list_fields(&environment.id, "hadoop", "3.5.0").unwrap();
        assert!(fields.iter().any(|field| field.id == "namenode_web_port"));
        schema::save_fields(
            &environment.id,
            "hadoop",
            "3.5.0",
            &[ConfigFieldUpdate {
                id: "namenode_web_port".to_string(),
                value: "19870".to_string(),
            }],
        )
        .unwrap();

        service::start(&environment.id, "hadoop", "3.5.0").unwrap();
        service::stop(&environment.id, "hadoop", "3.5.0").unwrap();
        uninstall::uninstall(&environment.id, "hadoop", "3.5.0", false).unwrap();

        assert!(environment::load(&environment.id)
            .unwrap()
            .components
            .is_empty());
        assert!(!migrated_instance.exists());
        assert!(
            !paths::var_data_instance_dir(&environment.id, "hadoop", "3.5.0")
                .unwrap()
                .exists()
        );

        let _ = std::fs::remove_dir_all(&tmp);
    }
}
