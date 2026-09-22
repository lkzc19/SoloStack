//! 数据布局版本标记（`.migration-version`）。

use crate::app::paths;

/// `v0.2.0` 建立的最低支持布局版本。
pub(super) const BASELINE_LAYOUT_VERSION: u32 = 1;

/// 读取布局版本；标记缺失或内容无效时返回 `None`。
pub(super) fn read() -> Result<Option<u32>, String> {
    let path = paths::migration_version_file().map_err(|e| e.to_string())?;
    match std::fs::read_to_string(&path) {
        Ok(content) => Ok(content.trim().parse().ok()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(format!("读取布局版本标记 {} 失败: {error}", path.display())),
    }
}

/// 写入当前布局版本。
pub(super) fn write(version: u32) -> Result<(), String> {
    paths::ensure_app_dirs().map_err(|e| e.to_string())?;
    let path = paths::migration_version_file().map_err(|e| e.to_string())?;
    std::fs::write(&path, format!("{version}\n"))
        .map_err(|e| format!("写入布局版本标记 {} 失败: {e}", path.display()))
}
