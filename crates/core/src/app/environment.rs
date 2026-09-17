//! 环境模型：用户可见名称、稳定环境 ID 和环境内组件清单。

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{paths, settings::Settings};

const SCHEMA_VERSION: u32 = 1;
const MAX_NAME_CHARS: usize = 40;

/// 环境中的一个组件实例。同一环境内 component 必须唯一。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EnvironmentComponent {
    pub component: String,
    pub version: String,
    pub installed_at: String,
}

/// 一个独立运行空间。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Environment {
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub components: Vec<EnvironmentComponent>,
    pub created_at: String,
    pub updated_at: String,
    #[serde(default)]
    pub last_activated_at: Option<String>,
}

fn default_schema_version() -> u32 {
    SCHEMA_VERSION
}

fn now() -> String {
    chrono::Local::now().to_rfc3339()
}

fn validate_id(id: &str) -> Result<(), String> {
    Uuid::parse_str(id)
        .map(|_| ())
        .map_err(|_| format!("无效的环境 ID: {id}"))
}

fn normalize_name(name: &str) -> Result<String, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("环境名称不能为空".to_string());
    }
    if name.chars().count() > MAX_NAME_CHARS {
        return Err(format!("环境名称不能超过 {MAX_NAME_CHARS} 个字符"));
    }
    if name.chars().any(char::is_control) {
        return Err("环境名称不能包含控制字符".to_string());
    }
    Ok(name.to_string())
}

fn ensure_name_available(name: &str, except_id: Option<&str>) -> Result<(), String> {
    let normalized = name.to_lowercase();
    for environment in list()? {
        if Some(environment.id.as_str()) == except_id {
            continue;
        }
        if environment.name.to_lowercase() == normalized {
            return Err(format!("环境名称已存在: {name}"));
        }
    }
    Ok(())
}

/// 环境目录路径。
pub fn directory(id: &str) -> Result<PathBuf, String> {
    paths::environment_dir(id).map_err(|e| e.to_string())
}

/// 环境元数据路径。
pub fn metadata_path(id: &str) -> Result<PathBuf, String> {
    paths::environment_file(id).map_err(|e| e.to_string())
}

/// 读取指定环境。
pub fn load(id: &str) -> Result<Environment, String> {
    validate_id(id)?;
    let path = metadata_path(id)?;
    let content = std::fs::read_to_string(&path)
        .map_err(|e| format!("读取环境 {} 失败: {e}", path.display()))?;
    let environment: Environment = serde_json::from_str(&content)
        .map_err(|e| format!("解析环境 {} 失败: {e}", path.display()))?;
    if environment.id != id {
        return Err(format!(
            "环境 ID 与目录不一致: 目录 {id}，文件 {}",
            environment.id
        ));
    }
    validate_id(&environment.id)?;
    Ok(environment)
}

/// 扫描环境目录并读取全部环境。
pub fn list() -> Result<Vec<Environment>, String> {
    let root = paths::environments_dir().map_err(|e| e.to_string())?;
    if !root.is_dir() {
        return Ok(Vec::new());
    }
    let mut environments = Vec::new();
    for entry in std::fs::read_dir(&root)
        .map_err(|e| format!("读取环境目录 {} 失败: {e}", root.display()))?
    {
        let entry = entry.map_err(|e| format!("读取环境目录项失败: {e}"))?;
        if !entry.path().is_dir() {
            continue;
        }
        let id = entry.file_name().to_string_lossy().to_string();
        if Uuid::parse_str(&id).is_err() {
            continue;
        }
        environments.push(load(&id)?);
    }
    environments.sort_by_key(|environment| environment.name.to_lowercase());
    Ok(environments)
}

/// 创建环境并写入元数据。
pub fn create(name: &str) -> Result<Environment, String> {
    let name = normalize_name(name)?;
    ensure_name_available(&name, None)?;
    paths::ensure_app_dirs().map_err(|e| e.to_string())?;

    let id = Uuid::new_v4().to_string();
    paths::ensure_environment_dirs(&id).map_err(|e| e.to_string())?;
    let timestamp = now();
    let environment = Environment {
        schema_version: SCHEMA_VERSION,
        id,
        name,
        components: Vec::new(),
        created_at: timestamp.clone(),
        updated_at: timestamp,
        last_activated_at: None,
    };
    if let Err(error) = environment.save() {
        let _ = std::fs::remove_dir_all(
            paths::environment_dir(&environment.id).map_err(|e| e.to_string())?,
        );
        return Err(error);
    }
    Ok(environment)
}

impl Environment {
    /// 写入环境元数据。先写临时文件再 rename，避免留下半截 JSON。
    pub fn save(&self) -> Result<(), String> {
        validate_id(&self.id)?;
        paths::ensure_environment_dirs(&self.id).map_err(|e| e.to_string())?;
        let path = metadata_path(&self.id)?;
        let temp = path.with_extension("json.tmp");
        let content =
            serde_json::to_string_pretty(self).map_err(|e| format!("序列化环境失败: {e}"))?;
        std::fs::write(&temp, content)
            .map_err(|e| format!("写入环境临时文件 {} 失败: {e}", temp.display()))?;
        if let Err(first_error) = std::fs::rename(&temp, &path) {
            if path.exists() {
                let backup = path.with_extension("json.bak");
                let _ = std::fs::remove_file(&backup);
                std::fs::rename(&path, &backup)
                    .map_err(|e| format!("备份环境文件 {} 失败: {e}", path.display()))?;
                match std::fs::rename(&temp, &path) {
                    Ok(()) => {
                        let _ = std::fs::remove_file(&backup);
                        Ok(())
                    }
                    Err(second_error) => {
                        let _ = std::fs::rename(&backup, &path);
                        Err(format!(
                            "保存环境文件 {} 失败: {second_error}（首次替换错误: {first_error}）",
                            path.display()
                        ))
                    }
                }
            } else {
                Err(format!(
                    "保存环境文件 {} 失败: {first_error}",
                    path.display()
                ))
            }
        } else {
            Ok(())
        }
    }

    /// 重命名环境。目录和环境 ID 保持不变。
    pub fn rename(&mut self, name: &str) -> Result<(), String> {
        let name = normalize_name(name)?;
        ensure_name_available(&name, Some(&self.id))?;
        self.name = name;
        self.updated_at = now();
        self.save()
    }

    /// 查询环境内的组件。
    pub fn component(&self, component: &str) -> Option<&EnvironmentComponent> {
        self.components
            .iter()
            .find(|item| item.component == component)
    }

    /// 安装前检查同组件唯一约束。
    pub fn ensure_can_install(&self, component: &str, version: &str) -> Result<(), String> {
        match self.component(component) {
            Some(existing) => Err(format!(
                "当前环境已安装 {component} v{}，不能再次安装 v{version}；请先卸载",
                existing.version
            )),
            None => Ok(()),
        }
    }

    /// 安装成功后登记组件。
    pub fn register_component(&mut self, component: &str, version: &str) -> Result<(), String> {
        self.ensure_can_install(component, version)?;
        self.components.push(EnvironmentComponent {
            component: component.to_string(),
            version: version.to_string(),
            installed_at: now(),
        });
        self.updated_at = now();
        self.save()
    }

    /// 卸载后移除组件登记。
    pub fn remove_component(&mut self, component: &str) -> Result<(), String> {
        self.components.retain(|item| item.component != component);
        self.updated_at = now();
        self.save()
    }

    /// 标记最近激活时间。
    pub fn mark_activated(&mut self) -> Result<(), String> {
        self.last_activated_at = Some(now());
        self.updated_at = now();
        self.save()
    }

    /// 删除环境目录。
    pub fn delete(self) -> Result<(), String> {
        let dir = directory(&self.id)?;
        if dir.exists() {
            std::fs::remove_dir_all(&dir)
                .map_err(|e| format!("删除环境目录 {} 失败: {e}", dir.display()))?;
        }
        Ok(())
    }
}

/// 当前活动环境 ID。
pub fn active_id() -> Result<Option<String>, String> {
    Settings::load()
        .map(|settings| settings.environment.active_id)
        .map_err(|e| e.to_string())
}

/// 设置活动环境；切换流程必须由上层在验证组件停止后调用。
pub fn set_active_id(id: Option<&str>) -> Result<(), String> {
    if let Some(id) = id {
        load(id)?;
    }
    let mut settings = Settings::load().map_err(|e| e.to_string())?;
    settings.environment.active_id = id.map(str::to_string);
    settings.save().map_err(|e| e.to_string())
}

/// 当前活动环境。
pub fn active() -> Result<Option<Environment>, String> {
    active_id()?.map(|id| load(&id)).transpose()
}

/// 确保至少有一个环境，并修复无效的活动环境 ID。
pub fn ensure_initialized() -> Result<Environment, String> {
    paths::ensure_app_dirs().map_err(|e| e.to_string())?;
    let environments = list()?;
    if environments.is_empty() {
        let environment = create("默认环境")?;
        set_active_id(Some(&environment.id))?;
        return Ok(environment);
    }

    if let Some(active_id) = active_id()? {
        if environments
            .iter()
            .any(|environment| environment.id == active_id)
        {
            return load(&active_id);
        }
    }

    let environment = environments
        .into_iter()
        .next()
        .ok_or_else(|| "没有可用环境".to_string())?;
    set_active_id(Some(&environment.id))?;
    Ok(environment)
}

/// 把旧版全局 `components/` 与 `var/` 迁移到环境目录。
///
/// 迁移规则：
/// - 不同组件、且同组件只有一个版本时，可以一起进入默认环境。
/// - 同一组件存在多个版本时，额外版本拆到独立环境。
/// - 使用 staging 目录，失败时回滚已移动路径。
pub fn migrate_legacy_layout() -> Result<Vec<Environment>, String> {
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
    let existing = list()?;
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
        set_active_id(Some(&first.id))?;
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
    std::fs::remove_dir_all(directory(&environment.id)?)
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
    let id = Uuid::new_v4().to_string();
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
        validate_components(&environment.components)?;
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

/// 校验一组组件清单中 component 不重复。
pub fn validate_components(components: &[EnvironmentComponent]) -> Result<(), String> {
    let mut seen = HashSet::new();
    for component in components {
        if !seen.insert(component.component.as_str()) {
            return Err(format!("同一环境不能重复组件: {}", component.component));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup(name: &str) -> std::path::PathBuf {
        let tmp = std::env::temp_dir().join(format!("solostack-environment-{name}"));
        std::env::set_var("HOME", &tmp);
        let _ = std::fs::remove_dir_all(&tmp);
        tmp
    }

    #[test]
    fn create_and_list_environment() {
        use crate::test_util::HOME_LOCK;
        let _guard = HOME_LOCK.lock().unwrap();
        let tmp = setup("create");

        let environment = create("开发环境").unwrap();
        assert!(Uuid::parse_str(&environment.id).is_ok());
        assert_eq!(list().unwrap().len(), 1);
        assert_eq!(load(&environment.id).unwrap().name, "开发环境");

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn component_name_is_unique_but_different_components_can_coexist() {
        use crate::test_util::HOME_LOCK;
        let _guard = HOME_LOCK.lock().unwrap();
        let tmp = setup("components");
        let mut environment = create("开发环境").unwrap();

        environment.register_component("hadoop", "3.5.0").unwrap();
        environment.register_component("kafka", "4.3.1").unwrap();
        assert_eq!(environment.components.len(), 2);
        assert!(environment.register_component("hadoop", "3.4.0").is_err());

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn rename_keeps_id_and_rejects_duplicate_name() {
        use crate::test_util::HOME_LOCK;
        let _guard = HOME_LOCK.lock().unwrap();
        let tmp = setup("rename");
        let mut first = create("开发环境").unwrap();
        let _second = create("测试环境").unwrap();
        let id = first.id.clone();

        first.rename("生产环境").unwrap();
        assert_eq!(first.id, id);
        assert!(first.rename("测试环境").is_err());

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

        let migrated = migrate_legacy_layout().unwrap();
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
    fn duplicate_legacy_component_versions_split_into_environments() {
        use crate::test_util::HOME_LOCK;
        let _guard = HOME_LOCK.lock().unwrap();
        let tmp = setup("legacy-duplicates");
        let root = tmp.join(paths::ROOT_DIR_NAME);
        std::fs::create_dir_all(root.join("components/hadoop/hadoop-3.5.0")).unwrap();
        std::fs::create_dir_all(root.join("components/hadoop/hadoop-3.4.1")).unwrap();

        let migrated = migrate_legacy_layout().unwrap();
        assert_eq!(migrated.len(), 2);
        assert!(migrated
            .iter()
            .all(|environment| environment.components.len() == 1));

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn stale_migration_is_restored_instead_of_deleted() {
        use crate::test_util::HOME_LOCK;
        let _guard = HOME_LOCK.lock().unwrap();
        let tmp = setup("stale-migration");
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
}
