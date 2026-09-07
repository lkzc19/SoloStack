use std::path::PathBuf;

use crate::paths;

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
        .filter_map(|e| {
            let mtime = e.metadata().and_then(|m| m.modified()).unwrap_or(std::time::UNIX_EPOCH);
            Some((mtime, e.path()))
        })
        .collect();
    files.sort_by(|a, b| b.0.cmp(&a.0)); // 新的在前
    Ok(files.into_iter().map(|(_, p)| p).collect())
}

/// 读取文件末尾 `lines` 行文本（日志 tail）。
pub fn tail(path: &std::path::Path, lines: usize) -> Result<String, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("读取日志 {} 失败: {e}", path.display()))?;
    let line_count = lines.max(1);
    let tail: Vec<&str> = content.lines().rev().take(line_count).collect();
    let mut out = tail.iter().rev().copied().collect::<Vec<_>>().join("\n");
    if !content.ends_with('\n') {
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
    paths::var_log_instance_dir(name, version)
        .map(|root| path.starts_with(&root))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
