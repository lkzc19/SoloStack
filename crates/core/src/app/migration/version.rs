//! 迁移版本标记（`.migration-version`）：记录已应用的迁移版本，避免每次启动重跑历史迁移。

use crate::app::paths;

// 版本号语义（对应 `migration::STEPS` 的目标版本）：
// - 0 = 旧版全局布局（无标记文件）
// - 1 = 多环境布局（`app/` `cache/` `environments/`，组件按环境隔离）

/// 读取已应用的布局版本。标记缺失或内容无法解析时视为 `0`（旧布局）。
///
/// 无法解析按 `0` 处理是刻意的：迁移步骤都幂等，宁可多跑一次空迁移，也不因一个
/// 标记文件损坏就让升级路径失效。
pub(super) fn current() -> Result<u32, String> {
    let path = paths::migration_version_file().map_err(|e| e.to_string())?;
    match std::fs::read_to_string(&path) {
        Ok(content) => Ok(content.trim().parse().unwrap_or(0)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(0),
        Err(error) => Err(format!("读取布局版本标记 {} 失败: {error}", path.display())),
    }
}

/// 写入已应用的布局版本。
pub(super) fn write(version: u32) -> Result<(), String> {
    paths::ensure_app_dirs().map_err(|e| e.to_string())?;
    let path = paths::migration_version_file().map_err(|e| e.to_string())?;
    std::fs::write(&path, format!("{version}\n"))
        .map_err(|e| format!("写入布局版本标记 {} 失败: {e}", path.display()))
}
