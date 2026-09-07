use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

use sha2::{Digest, Sha256};

use crate::paths;

/// 下载进度回调（接收已下载字节数 + 总字节数，total 可能为 0 表示未知）。
pub type ProgressFn = Box<dyn FnMut(u64, u64)>;

/// 下载目标文件路径（按 URL 文件名落到 `downloads/`）。
pub fn target_path(url: &str) -> Result<PathBuf, String> {
    let file_name = url
        .rsplit('/')
        .next()
        .filter(|s| !s.is_empty())
        .unwrap_or("download");
    let downloads_dir = paths::downloads_dir().map_err(|e| e.to_string())?;
    Ok(downloads_dir.join(file_name))
}

/// 下载单个文件到下载缓存目录（`data_root/downloads/`）。
///
/// - `url`：完整下载地址（由下载源文件 + 组件 + 版本解析）
/// - `progress`：可选进度回调
/// - `cancel`：取消标志，置位后中止下载并删除**未下载完**的部分文件
///
/// 返回下载后的完整文件路径。若包已存在于 `downloads/` 则直接复用（不重复下载）。
/// 取消时返回 `Err("安装已取消")`。
pub fn download(url: &str, mut progress: Option<ProgressFn>, cancel: &AtomicBool) -> Result<PathBuf, String> {
    let target = target_path(url)?;
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    // 已下载过：直接复用缓存包，跳过下载
    if target.exists() {
        return Ok(target);
    }

    let client = reqwest::blocking::Client::builder()
        .user_agent("SoloStack/0.1.0 (macOS; aarch64) Like Mozilla/5.0")
        .build()
        .map_err(|e| format!("创建 HTTP 客户端失败: {e}"))?;
    let resp = client.get(url).send().map_err(|e| format!("请求 {url} 失败: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("下载失败，HTTP {}", resp.status()));
    }
    let total = resp.content_length().unwrap_or(0);

    let mut out = std::fs::File::create(&target).map_err(|e| e.to_string())?;
    let mut downloaded: u64 = 0;
    let mut stream = resp;
    let mut buf = [0u8; 64 * 1024];
    loop {
        if cancel.load(Ordering::SeqCst) {
            // 未下载完：删除部分文件（已完整下载的文件不会走到这里，由调用方决定保留）
            let _ = out.flush();
            let _ = std::fs::remove_file(&target);
            return Err("安装已取消".to_string());
        }
        let n = stream
            .read(&mut buf)
            .map_err(|e| format!("读取响应失败: {e}"))?;
        if n == 0 {
            break;
        }
        out.write_all(&buf[..n]).map_err(|e| e.to_string())?;
        downloaded += n as u64;
        if let Some(cb) = progress.as_mut() {
            cb(downloaded, total);
        }
    }

    Ok(target)
}

/// 计算文件 SHA256（十六进制小写）。
pub fn sha256_of(path: &std::path::Path) -> Result<String, String> {
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    Ok(format!("{:x}", hasher.finalize()))
}

/// 校验文件 SHA256 与期望值是否一致。
pub fn verify_sha256(path: &std::path::Path, expected: &str) -> Result<(), String> {
    let actual = sha256_of(path)?;
    if actual.eq_ignore_ascii_case(expected) {
        Ok(())
    } else {
        Err(format!("SHA256 校验失败: 期望 {expected}, 实际 {actual}"))
    }
}

/// 一个已下载的缓存包（名称 + 字节大小）。
#[derive(Debug, Clone, serde::Serialize)]
pub struct DownloadedPackage {
    pub name: String,
    pub size: u64,
}

/// 列出 `downloads/` 目录下所有已下载的包（按名称排序）。
pub fn list_downloaded_packages() -> Result<Vec<DownloadedPackage>, String> {
    let dir = paths::downloads_dir().map_err(|e| e.to_string())?;
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut pkgs = Vec::new();
    for entry in std::fs::read_dir(&dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
        pkgs.push(DownloadedPackage { name, size });
    }
    pkgs.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(pkgs)
}

/// 删除 `downloads/` 目录下指定名称的缓存包（仅限该目录下的直接文件，防路径逃逸）。
pub fn delete_downloaded_packages(names: &[String]) -> Result<(), String> {
    let dir = paths::downloads_dir().map_err(|e| e.to_string())?;
    for name in names {
        let path = dir.join(name);
        if !path.starts_with(&dir) {
            continue;
        }
        let _ = std::fs::remove_file(&path);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_matches_known_vector() {
        // 空文件的 sha256
        let tmp = std::env::temp_dir().join("solostack-sha-empty");
        std::fs::write(&tmp, b"").unwrap();
        let hash = sha256_of(&tmp).unwrap();
        assert_eq!(
            hash,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn verify_matches_and_mismatch() {
        let tmp = std::env::temp_dir().join("solostack-sha-test");
        std::fs::write(&tmp, b"hello world").unwrap();
        let hash = sha256_of(&tmp).unwrap();
        assert!(verify_sha256(&tmp, &hash).is_ok());
        assert!(verify_sha256(&tmp, "deadbeef").is_err());
        let _ = std::fs::remove_file(&tmp);
    }
}
