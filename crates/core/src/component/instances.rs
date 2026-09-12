//! 已安装组件发现：从 `components/<组件>/<组件>-<版本>/` 目录结构推导已安装组件。
//!
//! 组件的版本、存在性从实际目录读取；展示名来自组件代码（`component::registry`）。

use crate::app::paths;
use crate::component;

/// 一个已安装的组件实例。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Installed {
    pub name: String,
    pub version: String,
    /// GUI 展示名（来自组件实现，缺省用 name）。
    pub display_name: String,
}

/// 列出已安装的组件（扫 `components/` 下各组件名目录）。
pub fn list_installed() -> Vec<Installed> {
    let mut out = Vec::new();
    let Ok(components) = paths::components_dir() else {
        return out;
    };
    let Ok(entries) = std::fs::read_dir(&components) else {
        return out;
    };
    for e in entries.flatten() {
        let path = e.path();
        if !path.is_dir() {
            continue;
        }
        let name = e.file_name().to_string_lossy().to_string();
        if let Some(ver) = latest_version(&name) {
            out.push(make_installed(&name, &ver));
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// 按组件名解析已安装实例（取最高版本），未安装报错。
pub fn resolve(name: &str) -> Result<Installed, String> {
    latest_version(name)
        .map(|ver| make_installed(name, &ver))
        .ok_or_else(|| format!("组件 {name} 未安装"))
}

/// 组件是否已安装。
pub fn is_installed(name: &str) -> bool {
    latest_version(name).is_some()
}

/// 扫 `components/<name>/` 下 `<name>-<版本>` 子目录，返回最高版本。
fn latest_version(name: &str) -> Option<String> {
    let dir = paths::component_dir(name).ok()?;
    let entries = std::fs::read_dir(&dir).ok()?;
    let prefix = format!("{name}-");
    let mut versions: Vec<String> = entries
        .flatten()
        .filter(|e| e.path().is_dir())
        .filter_map(|e| {
            let file = e.file_name().to_string_lossy().to_string();
            file.strip_prefix(&prefix).map(|v| v.to_string())
        })
        .collect();
    versions.sort_by(|a, b| version_cmp(a, b)); // 升序
    versions.last().cloned()
}

/// 简单版本比较（按点分段的数字比较；非数字段忽略）。`a < b` 语义。
fn version_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    let pa = a.split('.').collect::<Vec<_>>();
    let pb = b.split('.').collect::<Vec<_>>();
    for (x, y) in pa.iter().zip(pb.iter()) {
        let xn = x.parse::<u64>().unwrap_or(0);
        let yn = y.parse::<u64>().unwrap_or(0);
        if xn != yn {
            return xn.cmp(&yn);
        }
    }
    pa.len().cmp(&pb.len())
}

/// 组装 Installed；展示名取组件实现，未注册回退为 id。
fn make_installed(name: &str, version: &str) -> Installed {
    Installed {
        name: name.to_string(),
        version: version.to_string(),
        display_name: component::registry::display_name(name),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_cmp_orders_numeric() {
        assert_eq!(version_cmp("3.5.0", "3.10.0"), std::cmp::Ordering::Less);
        assert_eq!(version_cmp("4.1.0", "4.1.0"), std::cmp::Ordering::Equal);
        assert_eq!(version_cmp("3.10", "3.5"), std::cmp::Ordering::Greater);
    }
}
