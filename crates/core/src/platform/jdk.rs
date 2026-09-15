use std::path::{Path, PathBuf};

/// 本机已安装的 JDK。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Jdk {
    /// 目录名（如 `azul-17.0.18`）。
    pub name: String,
    /// 主版本号（如 `17`）。
    pub version: String,
    /// JDK home 路径（`Contents/Home` 或直接根目录）。
    pub path: PathBuf,
    /// 发行商（如 Azul / OpenJDK / Amazon Corretto）。
    pub vendor: String,
}

/// 扫描本机 JDK，返回系统级 + 用户级 + brew 三个位置的 JDK 列表。
///
/// 结果按主版本号升序排列。
pub fn scan() -> Vec<Jdk> {
    let mut jdks = Vec::new();
    if let Some(home) = std::env::var_os("JAVA_HOME").map(PathBuf::from) {
        if let Some(jdk) = jdk_from_env_home(&home) {
            jdks.push(jdk);
        }
    }
    for dir in candidate_dirs() {
        scan_dir(&dir, &mut jdks);
    }
    jdks.sort_by(|a, b| {
        a.version
            .cmp(&b.version)
            .then_with(|| a.path.cmp(&b.path))
            // 同一路径同时由扫描目录和环境变量发现时，保留名称更完整的扫描结果。
            .then_with(|| (a.name == "JAVA_HOME").cmp(&(b.name == "JAVA_HOME")))
    });
    jdks.dedup_by(|a, b| a.path == b.path);
    jdks
}

/// 把当前 `JAVA_HOME` 作为候选，支持 CI 使用 setup-java 注入的 JDK。
fn jdk_from_env_home(home: &Path) -> Option<Jdk> {
    let version = read_release_version(home)?;
    Some(Jdk {
        name: "JAVA_HOME".to_string(),
        version,
        path: home.to_path_buf(),
        vendor: infer_vendor(
            home.file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("JAVA_HOME"),
        ),
    })
}

/// 三个候选扫描位置（macOS 惯例）。
fn candidate_dirs() -> Vec<PathBuf> {
    let home = dirs::home_dir();
    let mut dirs: Vec<PathBuf> =
        std::iter::once(PathBuf::from("/Library/Java/JavaVirtualMachines"))
            .chain(home.map(|h| h.join("Library/Java/JavaVirtualMachines")))
            .chain(std::iter::once(PathBuf::from("/opt/homebrew/opt")))
            .collect();
    dirs.retain(|d| d.is_dir());
    dirs
}

/// 扫描单个目录，把识别出的 JDK 追加到 `out`。
fn scan_dir(dir: &Path, out: &mut Vec<Jdk>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        // brew 的 opt 目录是符号链接，跳过非常规 openjdk 前缀
        if name.starts_with("openjdk") {
            if let Some(jdk) = jdk_from_brew(&path, &name) {
                out.push(jdk);
            }
            continue;
        }
        if let Some(jdk) = jdk_from_standard(&path, &name) {
            out.push(jdk);
        }
    }
}

/// 标准 JavaVirtualMachines 布局：`<root>/Contents/Home`。
fn jdk_from_standard(root: &Path, name: &str) -> Option<Jdk> {
    let home = root.join("Contents/Home");
    if !home.is_dir() {
        return None;
    }
    // JDK 9+ 有 release 文件；JDK 8 没有，回退从目录名提取主版本
    let version = read_release_version(&home).or_else(|| version_from_dir_name(name))?;
    Some(Jdk {
        name: name.to_string(),
        version,
        path: home,
        vendor: infer_vendor(name),
    })
}

/// 从 JDK 目录名提取主版本（JDK 8 无 release 文件时回退用）。
///
/// `corretto-1.8.0_422` → `1`；`azul-17.0.18` → `17`；`openjdk@21` → `21`。
fn version_from_dir_name(name: &str) -> Option<String> {
    let lower = name.to_lowercase();
    let bytes = lower.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        if c == b'-' || c == b'@' {
            let start = i + 1;
            let mut j = start;
            while j < bytes.len() && (bytes[j].is_ascii_digit() || bytes[j] == b'.') {
                j += 1;
            }
            if j > start {
                let seg = &lower[start..j];
                return Some(major_version(seg));
            }
        }
        i += 1;
    }
    None
}

/// brew 布局：`/opt/homebrew/opt/openjdk@17`（符号链接指向 Cellar）。
fn jdk_from_brew(link: &Path, name: &str) -> Option<Jdk> {
    let home = link.join("libexec/openjdk.jdk/Contents/Home");
    if !home.is_dir() {
        return None;
    }
    let version = read_release_version(&home)?;
    Some(Jdk {
        name: name.to_string(),
        version,
        path: home,
        vendor: infer_vendor(name),
    })
}

/// 从目录名推断发行商（如 azul-17 → Azul、openjdk@17 → OpenJDK）。
fn infer_vendor(name: &str) -> String {
    let lower = name.to_lowercase();
    if lower.starts_with("azul") {
        "Azul".to_string()
    } else if lower.starts_with("corretto") {
        "Amazon Corretto".to_string()
    } else if lower.contains("temurin") {
        "Eclipse Temurin".to_string()
    } else if lower.starts_with("zulu") {
        "Azul Zulu".to_string()
    } else if lower.starts_with("openjdk") {
        "OpenJDK".to_string()
    } else if lower.starts_with("oracle") {
        "Oracle".to_string()
    } else if lower.starts_with("liberica") {
        "Liberica".to_string()
    } else {
        // 其他：取目录名去掉版本号部分（数字、分隔符）
        name.split(|c: char| c.is_ascii_digit() || c == '-' || c == '@' || c == '.')
            .next()
            .unwrap_or(name)
            .to_string()
    }
}

/// 从 `release` 文件读取主版本号（如 `JAVA_VERSION="17.0.18"` → `17`）。
///
/// 读取失败或格式异常返回 None（视为无效 JDK，跳过）。
fn read_release_version(home: &Path) -> Option<String> {
    let content = std::fs::read_to_string(home.join("release")).ok()?;
    for line in content.lines() {
        let line = line.trim();
        if let Some(value) = line.strip_prefix("JAVA_VERSION=") {
            let v = value.trim_matches('"').to_string();
            return Some(major_version(&v));
        }
    }
    None
}

/// 从完整版本号提取主版本号（`17.0.18` → `17`）。
///
/// 特殊处理：JDK 8 及更早的版本串以 `1.` 开头（如 `1.8.0_422`），
/// 惯例取前两位（`1.8`）而非只取 `1`。
fn major_version(full: &str) -> String {
    let parts: Vec<&str> = full.split('.').collect();
    if parts.len() >= 2 && parts[0] == "1" {
        return format!("{}.{}", parts[0], parts[1]);
    }
    parts
        .first()
        .map(|s| s.to_string())
        .unwrap_or_else(|| full.to_string())
}

impl Jdk {
    /// 该 JDK 是否满足某个组件模板的 JDK 要求（按主版本匹配）。
    pub fn matches(&self, requirement: &str) -> bool {
        self.version == requirement
    }
}

/// 在本机已安装的 JDK 中，查找满足组件 JDK 要求的版本。
///
/// 返回第一个匹配（扫描结果已按版本排序）。用于"有现成的就不下载"。
pub fn find_matching(requirement: &str) -> Option<Jdk> {
    scan().into_iter().find(|j| j.matches(requirement))
}

/// 按目录名精确查找本机 JDK（安装时用户指定的具体发行商+版本）。
pub fn find_by_name(name: &str) -> Option<Jdk> {
    scan().into_iter().find(|j| j.name == name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn major_version_extracts_first_segment() {
        assert_eq!(major_version("17.0.18"), "17");
        assert_eq!(major_version("21.0.10"), "21");
        // JDK 8 及更早：取前两位 1.8
        assert_eq!(major_version("1.8.0_422"), "1.8");
        assert_eq!(major_version("1.7.0_80"), "1.7");
    }

    #[test]
    fn jdk_matches_by_major() {
        let jdk = Jdk {
            name: "test".into(),
            version: "17".into(),
            path: PathBuf::from("/x"),
            vendor: "Test".into(),
        };
        assert!(jdk.matches("17"));
        assert!(!jdk.matches("21"));
    }

    #[test]
    fn infer_vendor_known_and_unknown() {
        assert_eq!(infer_vendor("azul-17.0.18"), "Azul");
        assert_eq!(infer_vendor("openjdk@17"), "OpenJDK");
        assert_eq!(infer_vendor("corretto-21"), "Amazon Corretto");
        // 未知前缀：去掉版本号部分
        let v = infer_vendor("myjdk-11");
        assert_eq!(v, "myjdk");
    }

    #[test]
    fn scan_finds_standard_and_brew() {
        // 真实扫描本机：在 CI/无 JDK 环境可能返回空，但结构上不会 panic。
        // 找到多少取决于机器环境，这里只校验「解析出来的条目字段自洽」。
        let jdks = scan();
        // 若本机有 JDK，应都能解析出 version 与 path
        for j in &jdks {
            assert!(!j.version.is_empty());
            assert!(j.path.is_dir());
        }
    }

    #[test]
    fn env_home_layout_parses_release_file() {
        let tmp = std::env::temp_dir().join("solostack-jdk-env-home");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        std::fs::write(tmp.join("release"), "JAVA_VERSION=\"17.0.18\"\n").unwrap();

        let jdk = jdk_from_env_home(&tmp).unwrap();
        assert_eq!(jdk.name, "JAVA_HOME");
        assert_eq!(jdk.version, "17");
        assert_eq!(jdk.path, tmp);

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn find_matching_returns_some_if_present() {
        // 本机若装了 17，应能匹配；否则返回 None 也合理。
        let found = find_matching("17");
        if let Some(jdk) = found {
            assert_eq!(jdk.version, "17");
        }
    }
}
