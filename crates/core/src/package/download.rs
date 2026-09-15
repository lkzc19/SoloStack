use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use sha2::{Digest, Sha256};

use crate::app::paths;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(60 * 60);

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
/// - `expected_sha256`：manifest 固定的 SHA256
/// - `progress`：可选进度回调
/// - `cancel`：取消标志，置位后中止下载并删除**未下载完**的部分文件
///
/// 返回下载后的完整文件路径。缓存文件校验通过才复用；无效缓存会删除并重新下载。
/// 取消时返回 `Err("安装已取消")`。
pub fn download(
    url: &str,
    expected_sha256: &str,
    mut progress: Option<ProgressFn>,
    cancel: &AtomicBool,
) -> Result<PathBuf, String> {
    validate_expected_sha256(expected_sha256)?;
    let target = target_path(url)?;
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    // 已下载过：校验通过才复用，损坏文件自动删除并重新下载。
    if target.exists() {
        match verify_sha256(&target, expected_sha256) {
            Ok(()) => return Ok(target),
            Err(_) => {
                std::fs::remove_file(&target)
                    .map_err(|e| format!("删除损坏缓存 {} 失败: {e}", target.display()))?;
            }
        }
    }
    let part = part_path(&target);

    let client = reqwest::blocking::Client::builder()
        .user_agent("SoloStack/0.1.0 (macOS; aarch64) Like Mozilla/5.0")
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(DOWNLOAD_TIMEOUT)
        .build()
        .map_err(|e| format!("创建 HTTP 客户端失败: {e}"))?;
    let resp = client
        .get(url)
        .send()
        .map_err(|e| format!("请求 {url} 失败: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("下载失败，HTTP {}", resp.status()));
    }
    let total = resp.content_length().unwrap_or(0);

    let mut out = std::fs::File::create(&part).map_err(|e| e.to_string())?;
    let mut hasher = Sha256::new();
    let mut downloaded: u64 = 0;
    let mut stream = resp;
    let mut buf = [0u8; 64 * 1024];
    loop {
        if cancel.load(Ordering::SeqCst) {
            // 未下载完：删除半成品（最终路径上不会留下任何东西）
            let _ = out.flush();
            let _ = std::fs::remove_file(&part);
            return Err("安装已取消".to_string());
        }
        let n = match stream.read(&mut buf) {
            Ok(n) => n,
            Err(e) => {
                // 读取失败同样清掉半成品，避免下次被当成有效缓存
                let _ = std::fs::remove_file(&part);
                return Err(format!("读取响应失败: {e}"));
            }
        };
        if n == 0 {
            break;
        }
        if let Err(e) = out.write_all(&buf[..n]) {
            let _ = std::fs::remove_file(&part);
            return Err(e.to_string());
        }
        hasher.update(&buf[..n]);
        downloaded += n as u64;
        if let Some(cb) = progress.as_mut() {
            cb(downloaded, total);
        }
    }

    // 收尾：flush + 落盘成功后才 rename 到最终路径（同目录，原子替换）
    if let Err(e) = out.flush() {
        let _ = std::fs::remove_file(&part);
        return Err(e.to_string());
    }
    drop(out);

    let actual = format!("{:x}", hasher.finalize());
    if !actual.eq_ignore_ascii_case(expected_sha256) {
        let _ = std::fs::remove_file(&part);
        return Err(format!(
            "SHA256 校验失败: 期望 {expected_sha256}, 实际 {actual}"
        ));
    }

    std::fs::rename(&part, &target).map_err(|e| {
        let _ = std::fs::remove_file(&part);
        format!("保存 {} 失败: {e}", target.display())
    })?;

    Ok(target)
}

/// 下载中的半成品路径：`<同名>.part`（与最终文件同目录，便于 rename）。
fn part_path(target: &std::path::Path) -> PathBuf {
    let name = target
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("download");
    target.with_file_name(format!("{name}.part"))
}

/// 计算文件 SHA256（十六进制小写）。
pub fn sha256_of(path: &std::path::Path) -> Result<String, String> {
    let mut file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = file
            .read(&mut buf)
            .map_err(|e| format!("读取 {} 失败: {e}", path.display()))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

/// 校验文件 SHA256 与期望值是否一致。
pub fn verify_sha256(path: &std::path::Path, expected: &str) -> Result<(), String> {
    validate_expected_sha256(expected)?;
    let actual = sha256_of(path)?;
    if actual.eq_ignore_ascii_case(expected) {
        Ok(())
    } else {
        Err(format!("SHA256 校验失败: 期望 {expected}, 实际 {actual}"))
    }
}

/// 缓存文件是否存在且 SHA256 正确。
pub fn is_cached(url: &str, expected_sha256: &str) -> Result<bool, String> {
    validate_expected_sha256(expected_sha256)?;
    let target = target_path(url)?;
    if !target.is_file() {
        return Ok(false);
    }
    Ok(verify_sha256(&target, expected_sha256).is_ok())
}

fn validate_expected_sha256(expected: &str) -> Result<(), String> {
    if expected.len() == 64 && expected.chars().all(|c| c.is_ascii_hexdigit()) {
        Ok(())
    } else {
        Err(format!("无效的 SHA256: {expected}"))
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
        // 下载中的半成品（`.part`）不算已缓存：列出来会让用户以为包已就绪
        if name.ends_with(".part") {
            continue;
        }
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
        // 只允许「纯文件名」，且解析后必须正好是缓存目录的直接子项。
        // 不能只做 starts_with 前缀比较：`../../app/settings.json` 仍满足前缀，
        // 而 remove_file 会在系统调用层解析 `..`，等于能删任意文件。
        if name.is_empty()
            || name.contains('/')
            || name.contains('\\')
            || name == "."
            || name == ".."
        {
            continue;
        }
        let path = dir.join(name);
        if path.parent() != Some(dir.as_path()) {
            continue;
        }
        let _ = std::fs::remove_file(&path);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 回归：`..` / 子路径一律拒绝，仅允许缓存目录下的直接文件。
    #[test]
    fn delete_only_accepts_plain_names_in_downloads_dir() {
        use crate::test_util::HOME_LOCK;
        let _guard = HOME_LOCK.lock().unwrap();
        let tmp = std::env::temp_dir().join("solostack-download-delete-guard");
        std::env::set_var("HOME", &tmp);
        let _ = std::fs::remove_dir_all(&tmp);

        let dir = paths::downloads_dir().unwrap();
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("keep.tgz"), "x").unwrap();
        // 托管目录外的受害者文件
        let victim = tmp.join(".solostack/app/settings.json");
        std::fs::create_dir_all(victim.parent().unwrap()).unwrap();
        std::fs::write(&victim, "{}").unwrap();

        delete_downloaded_packages(&[
            "../../app/settings.json".into(), // 逃逸尝试
            "sub/dir.tgz".into(),             // 子路径
            "..".into(),
            "keep.tgz".into(), // 合法
        ])
        .unwrap();

        assert!(victim.is_file(), "不得删除托管目录外的文件");
        assert!(!dir.join("keep.tgz").exists(), "合法项应被删除");

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn sha256_streaming_matches_known_value() {
        let path = std::env::temp_dir().join("solostack-download-sha256.txt");
        std::fs::write(&path, "abc").unwrap();
        assert_eq!(
            sha256_of(&path).unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn sha256_validation_rejects_malformed_expected_value() {
        let path = std::env::temp_dir().join("solostack-download-sha256-invalid.txt");
        std::fs::write(&path, "abc").unwrap();
        assert!(verify_sha256(&path, "not-a-hash").is_err());
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn cache_is_valid_only_when_sha256_matches() {
        use crate::test_util::HOME_LOCK;
        let _guard = HOME_LOCK.lock().unwrap();
        let tmp = std::env::temp_dir().join("solostack-download-cache-check");
        let _ = std::fs::remove_dir_all(&tmp);
        std::env::set_var("HOME", &tmp);

        let url = "https://example.com/package.tgz";
        let target = target_path(url).unwrap();
        std::fs::create_dir_all(target.parent().unwrap()).unwrap();
        std::fs::write(&target, "abc").unwrap();

        assert!(is_cached(
            url,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        )
        .unwrap());
        assert!(!is_cached(
            url,
            "f118328b2d053497350d5befd82c08db7ffd710327ff52943dd5caaa1b25db21"
        )
        .unwrap());

        let _ = std::fs::remove_dir_all(&tmp);
    }
}
