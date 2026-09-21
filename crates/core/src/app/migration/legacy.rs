//! 旧版全局 `components/` 目录中的实例扫描。

use std::path::Path;

use crate::app::paths;

/// 旧版布局里发现的一个组件实例（组件名 + 版本）。
#[derive(Debug, Clone)]
pub(super) struct LegacyInstance {
    pub(super) component: String,
    pub(super) version: String,
}

/// 扫描 `components/<组件>/<组件>-<版本>/` 得到旧实例列表。
pub(super) fn scan_legacy_instances(root: &Path) -> Result<Vec<LegacyInstance>, String> {
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
