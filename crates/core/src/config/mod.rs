//! 配置文件读写统一抽象。
//!
//! 所有「键值式配置文件」通过同一族方法操作：读全部 / 精确读单个键 /
//! 合并写 / 删除键。每种格式一个薄实现，组件代码只依赖 `ConfigFile` trait，
//! 不关心文件是 XML 还是 properties。
//!
//! # 文件组织
//!
//! | 文件 | 职责 |
//! |---|---|
//! | `mod.rs` | 语义单位 `Entry`、格式枚举、`ConfigFile` trait、格式分发、原子写收口 |
//! | `header.rs` | 受管说明头（`SoloStack:begin/end` 区块）的渲染、剥离、注入 |
//! | `line.rs` | 行式格式（properties / shell-env）共享的合并、删除、精确读算法 |
//! | `xml.rs` / `properties.rs` / `shell_env.rs` | 各格式的解析与渲染 |
//!
//! # 三条贯穿性约定
//!
//! - **只做合并写**：没有「整体替换」接口。任何写入都只改受管键、保留其余内容
//!   （官方模板里的注释与其它默认值必须留住），需要移除的键显式 `remove`。
//! - **受管头部**：格式允许注释时，写入的文件头部带 SoloStack 说明区块，
//!   向用户声明「受管键会被重写、请勿手改」。
//! - **原子写**：统一走「同目录临时文件 + rename」，避免崩溃/磁盘满留下截断的配置。
//!
//! 设计说明见 `docs/Config-File-Design.md`。

use std::path::{Path, PathBuf};

mod header;
mod line;
mod properties;
mod shell_env;
mod transaction;
mod xml;

use header::{inject, strip};

pub use header::{HEADER_BEGIN, HEADER_END};
pub use properties::PropertiesFile;
pub use shell_env::ShellEnvFile;
pub use transaction::{apply_plan, ConfigPlan};
pub use xml::XmlFile;

/// 一个键值条目（不同格式的公共语义单位）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub key: String,
    pub value: String,
}

impl Entry {
    pub fn new(key: impl Into<String>, value: impl Into<String>) -> Self {
        Entry {
            key: key.into(),
            value: value.into(),
        }
    }
}

/// 注释语法（决定受管头部怎么写）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommentStyle {
    /// `<!-- ... -->`（XML）
    Xml,
    /// `# ...`（properties / shell 脚本）
    Hash,
}

/// 配置文件格式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Xml,
    Properties,
    ShellEnv,
}

impl Format {
    /// 按扩展名识别格式；不认识返回 None（宁可报错也不猜错格式）。
    pub fn from_path(path: &Path) -> Option<Format> {
        let ext = path.extension()?.to_str()?.to_ascii_lowercase();
        match ext.as_str() {
            "xml" => Some(Format::Xml),
            "properties" | "props" | "conf" | "cfg" => Some(Format::Properties),
            "sh" => Some(Format::ShellEnv),
            _ => None,
        }
    }

    /// 该格式的注释语法；None = 不能承载注释（如未来的 JSON），此时跳过受管头部。
    pub fn comment_style(self) -> Option<CommentStyle> {
        match self {
            Format::Xml => Some(CommentStyle::Xml),
            Format::Properties | Format::ShellEnv => Some(CommentStyle::Hash),
        }
    }
}

/// 键值式配置文件的统一操作族。
pub trait ConfigFile {
    fn path(&self) -> &Path;
    fn format(&self) -> Format;
    /// 该文件的注释语法；默认由格式决定。
    fn comment_style(&self) -> Option<CommentStyle> {
        self.format().comment_style()
    }

    /// 读全部键值（不含注释与受管头部；文件不存在视为空）。
    fn read_entries(&self) -> Result<Vec<Entry>, String>;

    /// 精确读单个键；键不存在返回 None。
    fn get(&self, key: &str) -> Result<Option<String>, String> {
        Ok(self
            .read_entries()?
            .into_iter()
            .find(|e| e.key == key)
            .map(|e| e.value))
    }

    /// 精确读单个键并裁掉首尾空白；**空串视为未设置**（返回 None）。
    ///
    /// 「空值 = 未设置」是本项目的统一约定（组件配置项默认值兜底、端口 0 等都依赖它），
    /// 故收口在这里，避免每个组件各写一遍。
    fn get_trimmed(&self, key: &str) -> Result<Option<String>, String> {
        Ok(self
            .get(key)?
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty()))
    }

    /// 合并写：只改命中键的值、追加缺失的新键，其余内容与注释原样保留。
    /// 文件不存在或为空时按传入条目创建（并注入受管头部）。
    fn update_entries(&self, additions: &[Entry]) -> Result<(), String> {
        if additions.is_empty() {
            return Ok(());
        }
        let body = self.render_entries(additions, &[])?;
        commit(self.path(), self.comment_style(), &body)
    }

    /// 删除键（不存在的键忽略）。
    fn delete_entries(&self, keys: &[&str]) -> Result<(), String> {
        if keys.is_empty() {
            return Ok(());
        }
        let body = self.render_entries(&[], keys)?;
        if body.trim().is_empty() {
            return Ok(());
        }
        commit(self.path(), self.comment_style(), &body)
    }

    /// 在内存中生成合并/删除后的完整文件内容，不写盘。
    fn render_entries(&self, additions: &[Entry], removals: &[&str]) -> Result<String, String>;

    /// 便捷：设单个键（= `update_entries`）。
    fn set(&self, key: &str, value: &str) -> Result<(), String> {
        self.update_entries(&[Entry::new(key, value)])
    }

    /// 便捷：删单个键（= `delete_entries`）。
    fn remove(&self, key: &str) -> Result<(), String> {
        self.delete_entries(&[key])
    }
}

/// 按扩展名打开配置文件（创建句柄，不读盘）。
pub fn open(path: impl Into<PathBuf>) -> Result<Box<dyn ConfigFile>, String> {
    let path = path.into();
    let format = Format::from_path(&path)
        .ok_or_else(|| format!("不支持的配置文件类型: {}", path.display()))?;
    Ok(open_as(path, format))
}

/// 显式指定格式打开（扩展名不可靠时用）。
pub fn open_as(path: impl Into<PathBuf>, format: Format) -> Box<dyn ConfigFile> {
    let path = path.into();
    match format {
        Format::Xml => Box::new(XmlFile::new(path)),
        Format::Properties => Box::new(PropertiesFile::new(path)),
        Format::ShellEnv => Box::new(ShellEnvFile::new(path)),
    }
}

/// 解析「路径型」配置项为绝对路径。
///
/// 各组件的数据目录键（`dfs.namenode.name.dir`、`log.dirs` 等）语义不尽相同，
/// 但解析规则是通用的，收口在这里：
///
/// - **多值取第一个**：Hadoop 允许逗号分隔多个目录，我们只跟第一个（主副本所在）；
/// - **去 `file://` 前缀**：Hadoop 官方允许 `file:///data/dfs/name` 这种写法；
/// - **相对路径按 `base` 解析**：组件启动时的工作目录就是实例目录，与之一致。
///
/// 返回 `None` 表示该项未设置（空值）。
pub fn resolve_path(raw: &str, base: &Path) -> Option<PathBuf> {
    let first = raw.split(',').next().unwrap_or("").trim();
    let stripped = first.strip_prefix("file://").unwrap_or(first);
    if stripped.is_empty() {
        return None;
    }
    let p = PathBuf::from(stripped);
    Some(if p.is_absolute() { p } else { base.join(p) })
}

// ── 写入收口 ────────────────────────────────────────────

/// 统一收口：剥旧头部 → 注入新头部 → 原子写。各格式实现只负责拼出 body。
pub(super) fn commit(path: &Path, style: Option<CommentStyle>, body: &str) -> Result<(), String> {
    let out = prepare_content(style, body);
    write_atomic(path, &out)
}

/// 加上受管头部并统一末尾换行。
pub(super) fn prepare_content(style: Option<CommentStyle>, body: &str) -> String {
    let mut lines = lines_of(body);
    strip(&mut lines);
    if let Some(style) = style {
        inject(&mut lines, style);
    }
    let mut out = lines.join("\n");
    if !out.is_empty() {
        out.push('\n');
    }
    out
}

/// 按行拆分：去掉末尾换行后切分，不产生幽灵空行（重复写入保持稳定）。
fn lines_of(content: &str) -> Vec<String> {
    let trimmed = content.strip_suffix('\n').unwrap_or(content);
    if trimmed.is_empty() {
        Vec::new()
    } else {
        trimmed.split('\n').map(|s| s.to_string()).collect()
    }
}

/// 原子写：同目录临时文件 + rename，并继承原文件权限。
fn write_atomic(path: &Path, content: &str) -> Result<(), String> {
    let dir = path
        .parent()
        .ok_or_else(|| format!("路径无父目录: {}", path.display()))?;
    std::fs::create_dir_all(dir).map_err(|e| format!("创建目录 {} 失败: {e}", dir.display()))?;
    let name = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("config");
    let tmp = dir.join(format!(".{name}.solostack.tmp"));
    std::fs::write(&tmp, content).map_err(|e| format!("写入 {} 失败: {e}", tmp.display()))?;
    // 继承原文件权限，避免 rename 后权限被降级为默认值
    if let Ok(meta) = std::fs::metadata(path) {
        let _ = std::fs::set_permissions(&tmp, meta.permissions());
    }
    std::fs::rename(&tmp, path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        format!("替换 {} 失败: {e}", path.display())
    })
}

/// 读文件；不存在返回空串（按空配置处理）。
pub(super) fn read_opt(path: &Path) -> Result<String, String> {
    if !path.exists() {
        return Ok(String::new());
    }
    std::fs::read_to_string(path).map_err(|e| format!("读取 {} 失败: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("solostack-cfg-{name}"))
    }

    #[test]
    fn format_from_path_recognizes_extensions() {
        assert_eq!(
            Format::from_path(Path::new("/x/hdfs-site.xml")),
            Some(Format::Xml)
        );
        assert_eq!(
            Format::from_path(Path::new("/x/server.properties")),
            Some(Format::Properties)
        );
        assert_eq!(
            Format::from_path(Path::new("/x/hadoop-env.sh")),
            Some(Format::ShellEnv)
        );
        assert_eq!(Format::from_path(Path::new("/x/data.json")), None);
    }

    #[test]
    fn open_rejects_unknown_extension() {
        assert!(open(tmp_path("x.unknown")).is_err());
    }

    #[test]
    fn header_is_injected_and_idempotent() {
        let p = tmp_path("header.properties");
        let _ = std::fs::remove_file(&p);
        let f = open(&p).unwrap();
        for i in 0..3 {
            f.set("a", &i.to_string()).unwrap();
        }
        let content = std::fs::read_to_string(&p).unwrap();
        // 反复写入不堆积
        assert_eq!(
            content.matches(HEADER_BEGIN).count(),
            1,
            "begin 哨兵应只有一个"
        );
        assert_eq!(content.matches(HEADER_END).count(), 1, "end 哨兵应只有一个");
        assert!(
            content.starts_with("# SoloStack:begin"),
            "Hash 格式头部应在文件顶部"
        );
        assert!(
            f.get("a").unwrap().as_deref() == Some("2"),
            "受管键应写入且可精确读回"
        );
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn header_preserves_existing_license_block() {
        let p = tmp_path("license.xml");
        let _ = std::fs::remove_file(&p);
        std::fs::write(
            &p,
            "<?xml version=\"1.0\"?>\n<!--\n  Licensed under the Apache License\n-->\n<configuration>\n</configuration>\n",
        )
        .unwrap();
        let f = open(&p).unwrap();
        f.set("fs.defaultFS", "hdfs://localhost:8020").unwrap();

        let content = std::fs::read_to_string(&p).unwrap();
        assert!(
            content.starts_with("<?xml version=\"1.0\"?>"),
            "XML 声明必须仍在最前"
        );
        assert!(
            content.contains("Licensed under the Apache License"),
            "原有 license 注释应保留"
        );
        assert_eq!(content.matches(HEADER_BEGIN).count(), 1);
        assert_eq!(content.matches("<configuration>").count(), 1);
        assert!(f.get("fs.defaultFS").unwrap().is_some());
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn resolve_path_handles_file_uri_multi_value_and_relative() {
        let base = Path::new("/inst");
        // 绝对路径原样
        assert_eq!(
            resolve_path("/data/name", base),
            Some(PathBuf::from("/data/name"))
        );
        // file:// 前缀（Hadoop 官方写法）
        assert_eq!(
            resolve_path("file:///data/dfs/name", base),
            Some(PathBuf::from("/data/dfs/name"))
        );
        // 多目录取第一个
        assert_eq!(
            resolve_path("/a/name,/b/name", base),
            Some(PathBuf::from("/a/name"))
        );
        // 相对路径按实例目录解析（与启动 cwd 一致）
        assert_eq!(
            resolve_path("relative-name", base),
            Some(PathBuf::from("/inst/relative-name"))
        );
        // 空值 = 未设置
        assert_eq!(resolve_path("", base), None);
        assert_eq!(resolve_path("  ", base), None);
    }

    #[test]
    fn get_trimmed_treats_blank_as_unset() {
        let p = tmp_path("trimmed.properties");
        std::fs::write(&p, "a= 1 \nb=\nc=3\n").unwrap();
        let f = open(&p).unwrap();
        assert_eq!(f.get_trimmed("a").unwrap().as_deref(), Some("1"));
        assert_eq!(f.get_trimmed("b").unwrap(), None, "空值应视为未设置");
        assert_eq!(f.get_trimmed("zzz").unwrap(), None);
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn write_is_atomic_and_leaves_no_tmp() {
        let p = tmp_path("atomic.properties");
        let _ = std::fs::remove_file(&p);
        let f = open(&p).unwrap();
        f.set("k", "v").unwrap();
        assert!(p.is_file());
        let tmp = p.parent().unwrap().join(format!(
            ".{}.solostack.tmp",
            p.file_name().unwrap().to_string_lossy()
        ));
        assert!(!tmp.exists(), "不应残留临时文件");
        let _ = std::fs::remove_file(&p);
    }
}
