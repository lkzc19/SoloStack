use std::fs::File;
use std::path::Path;

use flate2::read::GzDecoder;

/// 解压 tar.gz 到目标目录，并处理"单层根目录"情况。
///
/// 多数官方包（如 hadoop）压缩包内是 `hadoop-3.5.0/...` 单层目录。
/// 解压时去掉这一层，让内容直接落到目标目录：
///   `components/hadoop/hadoop-3.5.0/<实际文件>`
/// 若目标已存在，先清除（重装覆盖）。
pub fn extract_tar_gz(archive: &Path, dest: &Path) -> Result<(), String> {
    let file = File::open(archive).map_err(|e| format!("打开压缩包失败: {e}"))?;
    let gz = GzDecoder::new(file);
    let mut tar = tar::Archive::new(gz);

    // 先解压到临时目录，避免部分写入污染目标
    let tmp = dest.with_extension("tmp");
    if tmp.exists() {
        std::fs::remove_dir_all(&tmp).map_err(|e| e.to_string())?;
    }
    std::fs::create_dir_all(&tmp).map_err(|e| e.to_string())?;

    tar.unpack(&tmp).map_err(|e| format!("解压失败: {e}"))?;

    // 检查是否为单层根目录结构
    let entries: Vec<_> = std::fs::read_dir(&tmp)
        .map_err(|e| e.to_string())?
        .filter_map(Result::ok)
        .collect();

    if entries.len() == 1 && entries[0].path().is_dir() {
        // 单层根目录：把根目录内容提升到目标
        let inner = entries[0].path();
        if dest.exists() {
            std::fs::remove_dir_all(dest).map_err(|e| e.to_string())?;
        }
        std::fs::rename(&inner, dest).map_err(|e| format!("移动解压内容失败: {e}"))?;
        let _ = std::fs::remove_dir_all(&tmp);
    } else {
        // 非单层：整体移动
        if dest.exists() {
            std::fs::remove_dir_all(dest).map_err(|e| e.to_string())?;
        }
        std::fs::rename(&tmp, dest).map_err(|e| format!("移动解压内容失败: {e}"))?;
    }

    Ok(())
}
