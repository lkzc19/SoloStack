use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::PathBuf;

use crate::app::paths;

/// 列出组件日志文件（`var/log/<组件>/<组件>-<版本>/`，按修改时间倒序）。
pub fn list_log_files(name: &str, version: &str) -> Result<Vec<PathBuf>, String> {
    let dir = paths::var_log_instance_dir(name, version).map_err(|e| e.to_string())?;
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut files: Vec<(std::time::SystemTime, PathBuf)> = std::fs::read_dir(&dir)
        .map_err(|e| e.to_string())?
        .flatten()
        .filter(|e| e.path().is_file())
        .filter(|e| {
            e.path()
                .extension()
                .and_then(|x| x.to_str())
                .is_some_and(|ext| matches!(ext, "log" | "out" | "err"))
        })
        .map(|e| {
            let mtime = e
                .metadata()
                .and_then(|m| m.modified())
                .unwrap_or(std::time::UNIX_EPOCH);
            (mtime, e.path())
        })
        .collect();
    files.sort_by_key(|f| std::cmp::Reverse(f.0)); // 新的在前
    Ok(files.into_iter().map(|(_, p)| p).collect())
}

/// 读取文件末尾 `lines` 行文本（日志 tail）。
pub fn tail(path: &std::path::Path, lines: usize) -> Result<String, String> {
    let line_count = lines.max(1);
    let ends_with_newline = std::fs::File::open(path)
        .and_then(|mut file| {
            let len = file.metadata()?.len();
            if len == 0 {
                return Ok(false);
            }
            file.seek(SeekFrom::End(-1))?;
            let mut byte = [0u8; 1];
            file.read_exact(&mut byte)?;
            Ok(byte[0] == b'\n')
        })
        .unwrap_or(false);
    let file =
        std::fs::File::open(path).map_err(|e| format!("读取日志 {} 失败: {e}", path.display()))?;
    let mut tail = VecDeque::with_capacity(line_count);
    for line in BufReader::new(file).lines() {
        let line = line.map_err(|e| format!("读取日志 {} 失败: {e}", path.display()))?;
        if tail.len() == line_count {
            tail.pop_front();
        }
        tail.push_back(line);
    }
    let mut out = tail.into_iter().collect::<Vec<_>>().join("\n");
    if !ends_with_newline && !out.is_empty() {
        out.push('\n');
    }
    Ok(out)
}

/// 校验日志路径位于 `~/.solostack/` 内（防路径逃逸）。
pub fn ensure_within_root(path: &std::path::Path) -> Result<(), String> {
    if paths::is_within_root(path).map_err(|e| e.to_string())? {
        Ok(())
    } else {
        Err(format!("日志路径超出托管目录: {}", path.display()))
    }
}

/// 校验日志路径属于该组件实例（`var/log/<组件>/<组件>-<版本>/`），防跨组件越权读取。
pub fn is_log_of(name: &str, version: &str, path: &std::path::Path) -> bool {
    // 同 is_within_root：`..` 组件能让词法前缀检查失效，必须显式拒绝
    if path
        .components()
        .any(|c| c == std::path::Component::ParentDir)
    {
        return false;
    }
    paths::var_log_instance_dir(name, version)
        .map(|root| path.starts_with(&root))
        .unwrap_or(false)
}

/// 校验并返回可安全打开的组件日志文件。
///
/// 同时检查组件归属、日志扩展名、真实文件和符号链接目标，防止通过日志页
/// 读取组件目录之外的文件。
pub fn validated_log_file(
    name: &str,
    version: &str,
    path: &std::path::Path,
) -> Result<PathBuf, String> {
    if !is_log_of(name, version, path) {
        return Err(format!("日志路径不属于组件 {name}"));
    }
    let valid_extension = path
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| matches!(ext, "log" | "out" | "err"));
    if !valid_extension {
        return Err(format!("不支持的日志文件类型: {}", path.display()));
    }
    if !path.is_file() {
        return Err(format!("日志文件不存在: {}", path.display()));
    }

    let root = paths::var_log_instance_dir(name, version)
        .map_err(|e| e.to_string())?
        .canonicalize()
        .map_err(|e| format!("解析日志目录失败: {e}"))?;
    let canonical = path
        .canonicalize()
        .map_err(|e| format!("解析日志路径失败: {e}"))?;
    if !canonical.starts_with(&root) {
        return Err(format!("日志路径超出组件日志目录: {}", path.display()));
    }
    Ok(canonical)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 回归：词法前缀比较挡不住 `..`，必须在校验层显式拒绝。
    #[test]
    fn path_guards_reject_parent_dir_components() {
        use crate::test_util::HOME_LOCK;
        let _guard = HOME_LOCK.lock().unwrap();
        let tmp = std::env::temp_dir().join("solostack-logs-traversal");
        std::env::set_var("HOME", &tmp);

        let log_root = paths::var_log_instance_dir("hadoop", "3.5.0").unwrap();
        let evil = log_root.join("../../../../../../etc/passwd");
        assert!(
            !is_log_of("hadoop", "3.5.0", &evil),
            "含 `..` 的路径必须被拒（否则能读到任意文件）"
        );
        assert!(ensure_within_root(&evil).is_err());
        // 正常路径不受影响
        assert!(is_log_of("hadoop", "3.5.0", &log_root.join("hadoop.log")));
        assert!(ensure_within_root(&log_root.join("hadoop.log")).is_ok());
    }

    #[test]
    fn tail_reads_last_lines() {
        let tmp = std::env::temp_dir().join("solostack-tail-test.log");
        std::fs::write(&tmp, "1\n2\n3\n4\n5\n").unwrap();
        let out = tail(&tmp, 2).unwrap();
        assert_eq!(out, "4\n5");
        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn tail_missing_file_errors() {
        let r = tail(&std::env::temp_dir().join("no-such-log-file.log"), 10);
        assert!(r.is_err());
    }

    #[test]
    fn list_log_files_var_log_only() {
        use crate::test_util::HOME_LOCK;
        let _guard = HOME_LOCK.lock().unwrap();
        let tmp = std::env::temp_dir().join("solostack-logs-test");
        std::env::set_var("HOME", &tmp);
        let _ = std::fs::remove_dir_all(&tmp);

        let log_dir = paths::var_log_instance_dir("hadoop", "3.5.0").unwrap();
        std::fs::create_dir_all(&log_dir).unwrap();
        std::fs::write(log_dir.join("hadoop-nn.log"), "x\n").unwrap();

        let files = list_log_files("hadoop", "3.5.0").unwrap();
        assert_eq!(files.len(), 1);
        assert!(is_log_of("hadoop", "3.5.0", &files[0]));
        assert!(!is_log_of("kafka", "4.1.0", &files[0]));

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn validated_log_file_rejects_cross_component_and_wrong_extension() {
        use crate::test_util::HOME_LOCK;
        let _guard = HOME_LOCK.lock().unwrap();
        let tmp = std::env::temp_dir().join("solostack-logs-validated");
        std::env::set_var("HOME", &tmp);
        let _ = std::fs::remove_dir_all(&tmp);

        let hadoop = paths::var_log_instance_dir("hadoop", "3.5.0").unwrap();
        let kafka = paths::var_log_instance_dir("kafka", "4.3.1").unwrap();
        std::fs::create_dir_all(&hadoop).unwrap();
        std::fs::create_dir_all(&kafka).unwrap();
        std::fs::write(hadoop.join("hadoop.log"), "ok\n").unwrap();
        std::fs::write(kafka.join("kafka.log"), "ok\n").unwrap();
        std::fs::write(hadoop.join("secret.txt"), "no\n").unwrap();

        assert!(validated_log_file("hadoop", "3.5.0", &hadoop.join("hadoop.log")).is_ok());
        assert!(validated_log_file("hadoop", "3.5.0", &kafka.join("kafka.log")).is_err());
        assert!(validated_log_file("hadoop", "3.5.0", &hadoop.join("secret.txt")).is_err());

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[cfg(unix)]
    #[test]
    fn validated_log_file_rejects_symlink_escape() {
        use crate::test_util::HOME_LOCK;
        let _guard = HOME_LOCK.lock().unwrap();
        let tmp = std::env::temp_dir().join("solostack-logs-symlink");
        std::env::set_var("HOME", &tmp);
        let _ = std::fs::remove_dir_all(&tmp);

        let root = paths::var_log_instance_dir("hadoop", "3.5.0").unwrap();
        std::fs::create_dir_all(&root).unwrap();
        let outside = tmp.join("outside.log");
        std::fs::write(&outside, "secret\n").unwrap();
        std::os::unix::fs::symlink(&outside, root.join("escape.log")).unwrap();

        assert!(validated_log_file("hadoop", "3.5.0", &root.join("escape.log")).is_err());

        let _ = std::fs::remove_dir_all(&tmp);
    }
}
