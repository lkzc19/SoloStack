//! 环境模型：用户可见名称、稳定环境 ID 和环境内组件清单。

use std::collections::HashSet;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::{id, paths, settings::Settings};

pub(crate) const SCHEMA_VERSION: u32 = 1;
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
    if super::id::is_environment_id(id) {
        Ok(())
    } else {
        Err(format!("无效的环境 ID: {id}"))
    }
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
        if !super::id::is_environment_id(&id) {
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

    let id = id::new_id();
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
        assert!(id::is_short_id(&environment.id));
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
    fn deleting_one_environment_does_not_affect_another() {
        use crate::test_util::HOME_LOCK;
        let _guard = HOME_LOCK.lock().unwrap();
        let tmp = setup("delete-isolation");

        let mut first = create("环境 A").unwrap();
        first.register_component("hadoop", "3.5.0").unwrap();
        let mut second = create("环境 B").unwrap();
        second.register_component("kafka", "4.3.1").unwrap();
        let first_snapshot = first.clone();
        let second_id = second.id.clone();

        let first_component = paths::instance_dir(&first.id, "hadoop", "3.5.0").unwrap();
        let first_data = paths::var_data_instance_dir(&first.id, "hadoop", "3.5.0").unwrap();
        let second_component = paths::instance_dir(&second_id, "kafka", "4.3.1").unwrap();
        std::fs::create_dir_all(&first_component).unwrap();
        std::fs::create_dir_all(&first_data).unwrap();
        std::fs::create_dir_all(&second_component).unwrap();
        std::fs::write(first_component.join("marker"), "first").unwrap();
        std::fs::write(first_data.join("marker"), "first-data").unwrap();
        std::fs::write(second_component.join("marker"), "second").unwrap();

        second.delete().unwrap();

        assert!(!directory(&second_id).unwrap().exists());
        assert!(first_component.join("marker").is_file());
        assert!(first_data.join("marker").is_file());
        assert_eq!(load(&first.id).unwrap(), first_snapshot);
        assert_eq!(list().unwrap().len(), 1);

        let _ = std::fs::remove_dir_all(&tmp);
    }
}
