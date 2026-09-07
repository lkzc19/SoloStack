//! 日志查看器：内置白名单（名称 + macOS Bundle ID）+ Spotlight 检测是否安装。
//!
//! 不扫描目录，而是用 `mdfind` 按 Bundle ID 查询本机是否存在对应 .app，
//! 只把白名单里真实安装的编辑器渲染出来。

use serde::Serialize;

/// 一个候选编辑器（显示名 + Bundle ID + 图标文件名）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct AppDef {
    pub name: String,
    pub bundle_id: String,
    pub icon: String,
}

/// 内置白名单：显示名称 + macOS Bundle ID + 图标（`static/icons/<icon>.png`）。
const WHITELIST: &[(&str, &str, &str)] = &[
    ("TextEdit", "com.apple.TextEdit", "textedit"),
    ("Visual Studio Code", "com.microsoft.VSCode", "vscode"),
    ("Cursor", "com.todesktop.230313mzl4w4u92", "cursor"),
    ("Sublime Text", "com.sublimetext.4", "sublime"),
    ("Zed", "dev.zed.Zed", "zed"),
];

/// 列出白名单里已安装的编辑器（按白名单顺序）。
pub fn list_available_apps() -> Vec<AppDef> {
    WHITELIST
        .iter()
        .filter(|(_, bundle_id, _)| is_installed(bundle_id))
        .map(|(name, bundle_id, icon)| AppDef {
            name: name.to_string(),
            bundle_id: bundle_id.to_string(),
            icon: icon.to_string(),
        })
        .collect()
}

/// 用 Spotlight（mdfind）按 Bundle ID 检测应用是否安装。
fn is_installed(bundle_id: &str) -> bool {
    let query = format!("kMDItemCFBundleIdentifier == '{}'", bundle_id);
    let out = std::process::Command::new("mdfind").arg(&query).output();
    match out {
        Ok(o) => o.status.success() && !o.stdout.is_empty(),
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whitelist_includes_default_editors() {
        let names: Vec<&str> = WHITELIST.iter().map(|(n, _, _)| *n).collect();
        assert!(names.contains(&"TextEdit"));
        assert!(names.contains(&"Visual Studio Code"));
        assert!(names.contains(&"Sublime Text"));
    }

    #[test]
    fn textedit_installed_on_macos() {
        if cfg!(target_os = "macos") {
            assert!(is_installed("com.apple.TextEdit"), "系统应自带 TextEdit");
        }
    }
}
