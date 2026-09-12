use std::fs::File;
use std::path::{Path, PathBuf};

use flate2::read::GzDecoder;

/// 解压 tar.gz 到目标目录，并处理"单层根目录"情况。
///
/// 多数官方包（如 hadoop）压缩包内是 `hadoop-3.5.0/...` 单层目录。
/// 解压时去掉这一层，让内容直接落到目标目录：
///   `components/hadoop/hadoop-3.5.0/<实际文件>`
/// 若目标已存在，先清除（重装覆盖）。
///
/// 解压先落到隐藏的暂存目录，成功后才移动到位 —— 中途失败会清掉暂存目录，
/// 不会在 `components/<组件>/` 下留下任何会被误认成「已安装版本」的残留。
pub fn extract_tar_gz(archive: &Path, dest: &Path) -> Result<(), String> {
    let file = File::open(archive).map_err(|e| format!("打开压缩包失败: {e}"))?;
    let gz = GzDecoder::new(file);
    let mut tar = tar::Archive::new(gz);

    let tmp = staging_dir(dest)?;
    if tmp.exists() {
        std::fs::remove_dir_all(&tmp).map_err(|e| e.to_string())?;
    }
    std::fs::create_dir_all(&tmp).map_err(|e| e.to_string())?;

    if let Err(e) = tar.unpack(&tmp) {
        let _ = std::fs::remove_dir_all(&tmp);
        return Err(format!("解压失败: {e}"));
    }

    match promote(&tmp, dest) {
        Ok(()) => {
            let _ = std::fs::remove_dir_all(&tmp);
            Ok(())
        }
        Err(e) => {
            let _ = std::fs::remove_dir_all(&tmp);
            Err(e)
        }
    }
}

/// 暂存目录：实例目录**同级的隐藏目录**（`components/<组件>/.<实例名>.extracting`）。
///
/// 三点讲究：
/// - 不能改写实例名本身：`dest.with_extension("tmp")` 会把版本号的最后一段当扩展名替换掉
///   （`hadoop-3.5.0` → `hadoop-3.5.tmp`），残留会被实例发现逻辑当成一个「已安装版本」，
///   且同一小版本的两个补丁版会共用同一个暂存目录；
/// - 以 `.` 开头：实例发现按 `<组件>-` 前缀扫描，隐藏目录不会被误认成版本；
/// - 与目标同父目录：保证后面的 `rename` 不跨设备。
fn staging_dir(dest: &Path) -> Result<PathBuf, String> {
    let name = dest
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| format!("目标目录名无效: {}", dest.display()))?;
    let parent = dest
        .parent()
        .ok_or_else(|| format!("目标目录无父目录: {}", dest.display()))?;
    Ok(parent.join(format!(".{name}.extracting")))
}

/// 把暂存目录里的内容提升到目标目录（处理单层根目录包装）。
fn promote(tmp: &Path, dest: &Path) -> Result<(), String> {
    let entries: Vec<_> = std::fs::read_dir(tmp)
        .map_err(|e| e.to_string())?
        .filter_map(Result::ok)
        .collect();

    // 单层根目录：把内层目录提升为目标（去掉 hadoop-3.5.0/ 这一层）
    let source = if entries.len() == 1 && entries[0].path().is_dir() {
        entries[0].path()
    } else {
        tmp.to_path_buf()
    };

    if dest.exists() {
        std::fs::remove_dir_all(dest).map_err(|e| e.to_string())?;
    }
    std::fs::rename(&source, dest).map_err(|e| format!("移动解压内容失败: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// 造一个 tar.gz：内含 `hadoop-3.5.0/bin/hdfs` 单层目录结构。
    fn make_archive(path: &Path) {
        let file = File::create(path).unwrap();
        let enc = flate2::write::GzEncoder::new(file, flate2::Compression::fast());
        let mut tar = tar::Builder::new(enc);
        let mut data = b"#!/bin/sh\n".to_vec();
        let mut header = tar::Header::new_gnu();
        header.set_size(data.len() as u64);
        header.set_mode(0o755);
        header.set_cksum();
        tar.append_data(&mut header, "hadoop-3.5.0/bin/hdfs", &data[..])
            .unwrap();
        data.clear();
        let enc = tar.into_inner().unwrap();
        enc.finish().unwrap().flush().unwrap();
    }

    #[test]
    fn staging_dir_keeps_version_and_is_hidden() {
        let dest = Path::new("/root/components/hadoop/hadoop-3.5.0");
        let tmp = staging_dir(dest).unwrap();
        assert_eq!(
            tmp,
            Path::new("/root/components/hadoop/.hadoop-3.5.0.extracting"),
            "暂存目录必须保留完整版本号且以 . 开头（不被实例发现误认成版本）"
        );
    }

    #[test]
    fn extracts_single_root_and_leaves_no_staging_dir() {
        let base = std::env::temp_dir().join("solostack-extract-test");
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        let archive = base.join("pkg.tgz");
        make_archive(&archive);

        let dest = base.join("hadoop-3.5.0");
        extract_tar_gz(&archive, &dest).unwrap();
        assert!(dest.join("bin/hdfs").is_file(), "单层根目录应被去掉");
        assert!(
            !base.join(".hadoop-3.5.0.extracting").exists(),
            "成功后不应残留暂存目录"
        );
        assert!(
            !base.join("hadoop-3.5.tmp").exists(),
            "不应出现吃掉版本号的旧式暂存目录"
        );

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn failed_extract_leaves_nothing_behind() {
        let base = std::env::temp_dir().join("solostack-extract-fail-test");
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        // 不是合法的 gzip：解压必然失败
        let archive = base.join("broken.tgz");
        std::fs::write(&archive, b"not a gzip").unwrap();

        let dest = base.join("hadoop-3.5.0");
        assert!(extract_tar_gz(&archive, &dest).is_err());
        assert!(!dest.exists(), "失败不应留下目标目录");
        assert!(
            !base.join(".hadoop-3.5.0.extracting").exists(),
            "失败应清掉暂存目录（否则会污染组件目录）"
        );

        let _ = std::fs::remove_dir_all(&base);
    }
}
