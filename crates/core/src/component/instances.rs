//! 已安装组件发现：以环境中的 `environment.json` 为组件清单事实来源。

use crate::app::{environment, paths};
use crate::component;

/// 一个已安装的组件实例。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Installed {
    pub environment_id: String,
    pub name: String,
    pub version: String,
    /// GUI 展示名（来自组件实现，缺省用 name）。
    pub display_name: String,
}

/// 列出指定环境中已安装的组件。
pub fn list_installed(environment_id: &str) -> Vec<Installed> {
    let Ok(environment) = environment::load(environment_id) else {
        return Vec::new();
    };
    let mut out: Vec<Installed> = environment
        .components
        .into_iter()
        .filter(|item| {
            component::registry::by_component(&item.component).is_some()
                && paths::instance_dir(environment_id, &item.component, &item.version)
                    .map(|path| path.is_dir())
                    .unwrap_or(false)
        })
        .map(|item| make_installed(environment_id, &item.component, &item.version))
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// 按环境 + 组件名解析已安装实例。
pub fn resolve(environment_id: &str, name: &str) -> Result<Installed, String> {
    if component::registry::by_component(name).is_none() {
        return Err(format!("不支持的组件: {name}"));
    }
    let environment = environment::load(environment_id)?;
    let item = environment
        .component(name)
        .ok_or_else(|| format!("当前环境未安装组件 {name}"))?;
    let instance =
        paths::instance_dir(environment_id, name, &item.version).map_err(|e| e.to_string())?;
    if !instance.is_dir() {
        return Err(format!(
            "组件 {name} v{} 的目录不存在，请先卸载后重新安装",
            item.version
        ));
    }
    Ok(make_installed(environment_id, name, &item.version))
}

/// 组件是否已安装。
pub fn is_installed(environment_id: &str, name: &str) -> bool {
    resolve(environment_id, name).is_ok()
}

/// 组装 Installed；展示名取组件实现。
fn make_installed(environment_id: &str, name: &str, version: &str) -> Installed {
    Installed {
        environment_id: environment_id.to_string(),
        name: name.to_string(),
        version: version.to_string(),
        display_name: component::registry::display_name(name),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_requires_environment_component_and_directory() {
        use crate::test_util::HOME_LOCK;
        let _guard = HOME_LOCK.lock().unwrap();
        let tmp = std::env::temp_dir().join("solostack-instances-environment");
        std::env::set_var("HOME", &tmp);
        let _ = std::fs::remove_dir_all(&tmp);

        let mut environment = environment::create("开发环境").unwrap();
        environment.register_component("hadoop", "3.5.0").unwrap();
        assert!(resolve(&environment.id, "hadoop").is_err());

        std::fs::create_dir_all(paths::instance_dir(&environment.id, "hadoop", "3.5.0").unwrap())
            .unwrap();
        assert!(resolve(&environment.id, "hadoop").is_ok());

        let _ = std::fs::remove_dir_all(&tmp);
    }
}
