//! XML 配置（Hadoop 的 `*-site.xml`）。
//!
//! 语义单位是 `<configuration>` 内的 `<property><name>/<value>` 键值对。
//! 编辑采用**文本级 patch**：只替换命中的 `<property>` 块，块间注释、其它块、
//! 文件头（`<?xml?>` / license 注释）与尾部一律原样保留 —— 避免「解析成 DOM
//! 再序列化」把官方模板的注释洗掉。

use std::ops::Range;
use std::path::{Path, PathBuf};

use super::{commit, read_opt, ConfigFile, Entry, Format};

pub struct XmlFile {
    path: PathBuf,
}

impl XmlFile {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        XmlFile { path: path.into() }
    }
}

impl ConfigFile for XmlFile {
    fn path(&self) -> &Path {
        &self.path
    }

    fn format(&self) -> Format {
        Format::Xml
    }

    fn read_entries(&self) -> Result<Vec<Entry>, String> {
        parse_properties(&read_opt(&self.path)?)
    }

    /// 精确读：直接定位该 `<name>` 的块取 `<value>`，不经上层解析。
    fn get(&self, key: &str) -> Result<Option<String>, String> {
        find_property(&read_opt(&self.path)?, key)
    }

    fn update_entries(&self, additions: &[Entry]) -> Result<(), String> {
        let original = read_opt(&self.path)?;
        let body = if original.trim().is_empty() {
            create_body(additions)
        } else {
            patch(&original, additions)?
        };
        commit(&self.path, self.comment_style(), &body)
    }

    fn delete_entries(&self, keys: &[&str]) -> Result<(), String> {
        let original = read_opt(&self.path)?;
        if original.trim().is_empty() {
            return Ok(());
        }
        let body = remove(&original, keys)?;
        commit(&self.path, self.comment_style(), &body)
    }
}

// ── 解析 ────────────────────────────────────────────────

/// 把 `<!-- … -->` 注释区替换成等长空白（换行保留），**字节偏移不变**。
///
/// 这样后续所有扫描与取值都只需面对「没有注释」的文本，而偏移仍可直接用于在原文上
/// 切片替换。必要性：把某个覆盖项注释掉是配置文件的常规用法，若不屏蔽注释，
/// 被注释的 `<property>` 会被当成生效配置读出来（探活按不存在的端口探测、状态永远不对），
/// `patch`/`remove` 还会去改写或删掉注释区里的内容。
///
/// 未闭合的注释按「注释到文件末尾」处理，与 XML 语义一致。
fn mask_comments(content: &str) -> String {
    let bytes = content.as_bytes();
    let mut out = bytes.to_vec();
    let mut i = 0usize;
    while i + 4 <= bytes.len() {
        if &bytes[i..i + 4] == b"<!--" {
            let end = content[i + 4..]
                .find("-->")
                .map(|rel| i + 4 + rel + 3)
                .unwrap_or(bytes.len());
            for b in &mut out[i..end] {
                if *b != b'\n' {
                    *b = b' ';
                }
            }
            i = end;
        } else {
            i += 1;
        }
    }
    String::from_utf8(out).unwrap_or_else(|_| content.to_string())
}

/// 一个 `<property>` 块：在**原文**中的字节区间 + 解析出的 name/value。
struct Block {
    range: Range<usize>,
    name: Option<String>,
    value: String,
}

/// 收集全部 `<property>` 块（按出现顺序）。
///
/// 注释区内的块**不可见**（见 `mask_comments`）；`range` 始终指向原文，供替换/删除使用。
/// 块未闭合时报错（不静默跳过，避免把损坏文件当成空配置）。
fn collect_blocks(content: &str) -> Result<Vec<Block>, String> {
    let masked = mask_comments(content);
    let mut blocks = Vec::new();
    let mut cursor = 0usize;
    while let Some(rel) = masked[cursor..].find("<property>") {
        let start = cursor + rel;
        let after = start + "<property>".len();
        let end_rel = masked[after..]
            .find("</property>")
            .ok_or("XML 中 <property> 缺少 </property>")?;
        let end = after + end_rel + "</property>".len();
        let block = &masked[start..end];
        blocks.push(Block {
            range: start..end,
            name: block_name(block),
            value: extract_tag(block, "value").unwrap_or_default(),
        });
        cursor = end;
    }
    Ok(blocks)
}

fn parse_properties(content: &str) -> Result<Vec<Entry>, String> {
    Ok(collect_blocks(content)?
        .into_iter()
        .filter_map(|b| b.name.map(|n| Entry::new(n, b.value)))
        .collect())
}

fn find_property(content: &str, key: &str) -> Result<Option<String>, String> {
    Ok(collect_blocks(content)?
        .into_iter()
        .find(|b| b.name.as_deref() == Some(key))
        .map(|b| b.value))
}

/// 取块内某标签的文本（如 `<name>` / `<value>`）。
fn extract_tag(block: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let s = block.find(&open)? + open.len();
    let e = block[s..].find(&close)? + s;
    Some(block[s..e].trim().to_string())
}

fn block_name(block: &str) -> Option<String> {
    extract_tag(block, "name")
}

// ── 生成 / 改写 ─────────────────────────────────────────

/// 从零创建 XML 配置（文件缺失时的兜底；保留 Hadoop 惯用的声明与样式表指令）。
fn create_body(entries: &[Entry]) -> String {
    let mut out = String::from(
        "<?xml version=\"1.0\"?>\n<?xml-stylesheet type=\"text/xsl\" href=\"configuration.xsl\"?>\n<configuration>\n",
    );
    for e in entries {
        out.push_str(&format_property(e));
    }
    out.push_str("</configuration>\n");
    out
}

/// 文本级 patch：命中键替换其 `<property>` 块；未命中的新键追加在
/// `</configuration>` 之前；其余内容（含注释，含被注释掉的旧块）逐字保留。
fn patch(original: &str, additions: &[Entry]) -> Result<String, String> {
    let mut out = String::with_capacity(original.len() + 128);
    let mut pending: Vec<&Entry> = additions.iter().collect();

    let mut cursor = 0usize;
    for block in collect_blocks(original)? {
        out.push_str(&original[cursor..block.range.start]);
        let pos = block
            .name
            .as_deref()
            .and_then(|n| pending.iter().position(|e| e.key == n));
        match pos {
            Some(pos) => {
                let e = pending.remove(pos);
                out.push_str(&format_property(e));
            }
            None => out.push_str(&original[block.range.clone()]),
        }
        cursor = block.range.end;
    }

    let rest = &original[cursor..];
    if pending.is_empty() {
        out.push_str(rest);
        return Ok(out);
    }
    let close = "</configuration>";
    let close_pos = rest
        .rfind(close)
        .ok_or("配置文件中未找到 </configuration> 节点")?;
    out.push_str(&rest[..close_pos]);
    for e in pending {
        out.push_str(&format_property(e));
    }
    out.push_str(&rest[close_pos..]);
    Ok(out)
}

/// 删除命中的 `<property>` 块，并吃掉块前残留的纯空白行。
fn remove(original: &str, keys: &[&str]) -> Result<String, String> {
    let mut out = String::with_capacity(original.len());
    let mut cursor = 0usize;

    for block in collect_blocks(original)? {
        out.push_str(&original[cursor..block.range.start]);
        if block
            .name
            .as_deref()
            .map(|n| keys.contains(&n))
            .unwrap_or(false)
        {
            // 命中：跳过整块，并回收块前只含缩进的那一段
            if let Some(nl) = out.rfind('\n') {
                if out[nl + 1..].trim().is_empty() {
                    out.truncate(nl + 1);
                }
            }
        } else {
            out.push_str(&original[block.range.clone()]);
        }
        cursor = block.range.end;
    }

    out.push_str(&original[cursor..]);
    Ok(out)
}

/// 单个 property 块的规范化文本。
fn format_property(e: &Entry) -> String {
    format!(
        "  <property>\n    <name>{}</name>\n    <value>{}</value>\n  </property>\n",
        e.key, e.value
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{HEADER_BEGIN, HEADER_END};

    fn tmp(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("solostack-xml-{name}.xml"))
    }

    const SAMPLE: &str = r#"<?xml version="1.0"?>
<!-- license -->
<configuration>
  <!-- keep me -->
  <property>
    <name>a</name>
    <value>1</value>
  </property>
  <property>
    <name>b</name>
    <value>2</value>
  </property>
</configuration>
"#;

    /// 把覆盖项注释掉是 Hadoop 配置的常规用法：注释里的块不能算生效配置，
    /// 否则探活会按一个并不存在的端口探测（状态永远不对）。
    #[test]
    fn commented_out_property_is_not_read() {
        let p = tmp("commented");
        std::fs::write(
            &p,
            r#"<?xml version="1.0"?>
<configuration>
  <!-- 暂时停用这个覆盖项
  <property>
    <name>dfs.namenode.http-address</name>
    <value>localhost:9871</value>
  </property>
  -->
  <property>
    <name>dfs.namenode.http-address</name>
    <value>localhost:9870</value>
  </property>
</configuration>
"#,
        )
        .unwrap();
        let f = XmlFile::new(&p);
        assert_eq!(
            f.get("dfs.namenode.http-address").unwrap().as_deref(),
            Some("localhost:9870"),
            "注释块不应遮蔽真正的定义"
        );
        assert_eq!(f.read_entries().unwrap().len(), 1, "只应读到一个生效块");
        let _ = std::fs::remove_file(&p);
    }

    /// 注释区内的块既不该被改写，也不该被删除。
    #[test]
    fn commented_out_property_is_left_alone_by_write_paths() {
        let p = tmp("commented-write");
        std::fs::write(
            &p,
            r#"<configuration>
  <!-- 历史遗留 <property><name>a</name><value>1</value></property> -->
  <property>
    <name>a</name>
    <value>2</value>
  </property>
</configuration>
"#,
        )
        .unwrap();
        let f = XmlFile::new(&p);
        f.set("a", "9").unwrap();
        // 生效块被改写
        assert_eq!(f.get("a").unwrap().as_deref(), Some("9"));
        let content = std::fs::read_to_string(&p).unwrap();
        assert!(
            content
                .contains("<!-- 历史遗留 <property><name>a</name><value>1</value></property> -->"),
            "注释区必须逐字保留，不能被改写/取消注释"
        );

        // 删除也只作用于生效块
        f.remove("a").unwrap();
        let content = std::fs::read_to_string(&p).unwrap();
        assert!(!content.contains("<name>a</name>\n    <value>9</value>"));
        assert!(content.contains("历史遗留"), "删除不应动注释区");
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn update_patch_keeps_comments_and_other_props() {
        let p = tmp("patch");
        std::fs::write(&p, SAMPLE).unwrap();
        let f = XmlFile::new(&p);
        f.set("b", "3").unwrap();
        f.set("c", "4").unwrap();

        let content = std::fs::read_to_string(&p).unwrap();
        assert!(content.contains("<!-- license -->"), "头部注释应保留");
        assert!(content.contains("<!-- keep me -->"), "块间注释应保留");
        assert!(content.contains("<name>a</name>"), "其它属性应保留");
        assert!(content.contains("<value>3</value>"), "命中值应更新");
        assert!(!content.contains("<value>2</value>"));
        assert_eq!(content.matches("<configuration>").count(), 1);
        assert_eq!(content.matches("</configuration>").count(), 1);
        let close = content.find("</configuration>").unwrap();
        assert!(
            content[..close].contains("<name>c</name>"),
            "新键应在 </configuration> 之前"
        );
        assert_eq!(f.get("c").unwrap().as_deref(), Some("4"));
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn get_is_precise_and_missing_key_is_none() {
        let p = tmp("get");
        std::fs::write(&p, SAMPLE).unwrap();
        let f = XmlFile::new(&p);
        assert_eq!(f.get("a").unwrap().as_deref(), Some("1"));
        assert_eq!(f.get("zzz").unwrap(), None);
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn delete_removes_block_and_keeps_others() {
        let p = tmp("delete");
        std::fs::write(&p, SAMPLE).unwrap();
        let f = XmlFile::new(&p);
        f.remove("b").unwrap();

        let content = std::fs::read_to_string(&p).unwrap();
        assert!(!content.contains("<name>b</name>"), "命中块应删除");
        assert!(content.contains("<name>a</name>"), "其它块应保留");
        assert!(content.contains("<!-- license -->"), "注释应保留");
        assert!(!content.contains("    \n"), "不应残留纯缩进行");
        assert_eq!(f.read_entries().unwrap().len(), 1);
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn set_entry_precise_reads_ports_from_site_xml() {
        // 探活端口的精确读路径：dfs.namenode.http-address → 端口
        let p = tmp("ports");
        std::fs::write(
            &p,
            "<?xml version=\"1.0\"?>\n<configuration>\n</configuration>\n",
        )
        .unwrap();
        let f = XmlFile::new(&p);
        f.set("dfs.namenode.http-address", "localhost:9871")
            .unwrap();
        let v = f.get("dfs.namenode.http-address").unwrap().unwrap();
        assert_eq!(v.rsplit(':').next().unwrap(), "9871");
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn create_body_when_file_missing() {
        let p = tmp("create");
        let _ = std::fs::remove_file(&p);
        let f = XmlFile::new(&p);
        f.set("k", "v").unwrap();

        let content = std::fs::read_to_string(&p).unwrap();
        assert!(content.starts_with("<?xml version=\"1.0\"?>"));
        assert_eq!(content.matches("<configuration>").count(), 1);
        assert_eq!(f.get("k").unwrap().as_deref(), Some("v"));
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn repeated_writes_keep_single_header() {
        let p = tmp("header");
        std::fs::write(&p, SAMPLE).unwrap();
        let f = XmlFile::new(&p);
        for i in 0..3 {
            f.set("a", &i.to_string()).unwrap();
        }
        let content = std::fs::read_to_string(&p).unwrap();
        assert_eq!(content.matches(HEADER_BEGIN).count(), 1);
        assert_eq!(content.matches(HEADER_END).count(), 1);
        assert!(content.starts_with("<?xml version=\"1.0\"?>"));
        assert!(content.contains("<!-- license -->"));
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn broken_block_reports_error() {
        let p = tmp("broken");
        std::fs::write(&p, "<configuration>\n<property><name>a</name>\n").unwrap();
        let f = XmlFile::new(&p);
        assert!(
            f.read_entries().is_err(),
            "未闭合的 property 应报错而非静默忽略"
        );
        let _ = std::fs::remove_file(&p);
    }
}
