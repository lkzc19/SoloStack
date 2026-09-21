//! 迁移用的文件移动原语：带目录创建、冲突检测与回滚记录。

use std::path::{Path, PathBuf};

/// 把 `source` 移到 `destination`（存在才移），并记录到 `moves` 供回滚。
pub(super) fn move_if_exists(
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

/// 把 staging 中的数据移回原位（崩溃恢复用）。目标已存在即报冲突，绝不覆盖。
pub(super) fn restore_move(staged: &Path, original: &Path) -> Result<(), String> {
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
