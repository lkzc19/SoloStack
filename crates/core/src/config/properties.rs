//! `key=value` 行式配置（Kafka 的 `server.properties` 等）。
//!
//! 语法：`key=value`，`#` 或 `!` 起始的行为注释，空行忽略。
//! 不做 Java properties 的完整语义（转义、续行、行内注释）—— 我们只读自己写入
//! 的键，读取容忍即可；写回时行级替换，其余行（含注释）逐字保留。

use std::path::{Path, PathBuf};

use super::line::{lookup, merge, remove};
use super::{lines_of, read_opt, ConfigFile, Entry, Format};

pub struct PropertiesFile {
    path: PathBuf,
}

impl PropertiesFile {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        PropertiesFile { path: path.into() }
    }
}

impl ConfigFile for PropertiesFile {
    fn path(&self) -> &Path {
        &self.path
    }

    fn format(&self) -> Format {
        Format::Properties
    }

    fn read_entries(&self) -> Result<Vec<Entry>, String> {
        let content = read_opt(&self.path)?;
        Ok(lines_of(&content)
            .into_iter()
            .filter_map(|line| parse(&line).map(|(k, raw)| Entry::new(k, value(raw))))
            .collect())
    }

    fn get(&self, key: &str) -> Result<Option<String>, String> {
        Ok(lookup(&read_opt(&self.path)?, key, parse, value))
    }

    fn render_entries(&self, additions: &[Entry], removals: &[&str]) -> Result<String, String> {
        let content = read_opt(&self.path)?;
        if content.trim().is_empty() {
            let lines: Vec<String> = additions.iter().map(|e| render(&e.key, &e.value)).collect();
            return Ok(lines.join("\n"));
        }
        let content = remove(&content, removals, key_of);
        Ok(merge(&content, additions, key_of, render))
    }
}

/// 解析一行 → (键, 原始值)；注释行 / 空行 / 无 `=` 行返回 None。
fn parse(line: &str) -> Option<(&str, &str)> {
    let t = line.trim();
    if t.is_empty() || t.starts_with('#') || t.starts_with('!') {
        return None;
    }
    let (k, v) = t.split_once('=')?;
    let k = k.trim();
    if k.is_empty() {
        return None;
    }
    Some((k, v))
}

fn key_of(line: &str) -> Option<&str> {
    parse(line).map(|(k, _)| k)
}

fn value(raw: &str) -> String {
    raw.trim().to_string()
}

fn render(key: &str, value: &str) -> String {
    format!("{key}={value}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{HEADER_BEGIN, HEADER_END};

    fn tmp(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("solostack-props-{name}.properties"))
    }

    #[test]
    fn update_preserves_comments_and_order() {
        let p = tmp("keep");
        std::fs::write(&p, "# header\na=1\n\nb=2 # note\n# tail\n").unwrap();
        let f = PropertiesFile::new(&p);
        f.set("b", "3").unwrap(); // 命中 → 行级替换
        f.set("c", "4").unwrap(); // 新键 → 追加末尾

        let content = std::fs::read_to_string(&p).unwrap();
        assert!(content.contains("# header"), "注释应保留");
        assert!(content.contains("# tail"), "注释应保留");
        assert!(content.contains("a=1"), "未命中键应保留");
        assert!(content.contains("b=3"), "命中行应被替换");
        assert!(!content.contains("b=2"), "旧值不应残留");
        assert!(content.trim_end().ends_with("c=4"), "新键应追加到末尾");

        assert_eq!(f.get("a").unwrap().as_deref(), Some("1"));
        assert_eq!(f.get("b").unwrap().as_deref(), Some("3"));
        assert_eq!(f.get("zzz").unwrap(), None);
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn delete_removes_only_matching_lines() {
        let p = tmp("del");
        std::fs::write(&p, "# c\na=1\nb=2\nc=3\n").unwrap();
        let f = PropertiesFile::new(&p);
        f.remove("b").unwrap();

        let content = std::fs::read_to_string(&p).unwrap();
        assert!(!content.contains("b=2"));
        assert!(content.contains("a=1") && content.contains("c=3") && content.contains("# c"));
        assert_eq!(f.read_entries().unwrap().len(), 2);
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn missing_file_is_created_with_header() {
        let p = tmp("create");
        let _ = std::fs::remove_file(&p);
        let f = PropertiesFile::new(&p);
        f.set("x", "y").unwrap();
        let content = std::fs::read_to_string(&p).unwrap();
        assert!(content.starts_with("# SoloStack:begin"));
        assert_eq!(content.matches(HEADER_BEGIN).count(), 1);
        assert_eq!(content.matches(HEADER_END).count(), 1);
        assert_eq!(f.get("x").unwrap().as_deref(), Some("y"));
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn repeated_writes_do_not_accumulate_header() {
        let p = tmp("idem");
        std::fs::write(&p, "a=1\n").unwrap();
        let f = PropertiesFile::new(&p);
        for i in 0..4 {
            f.set("a", &i.to_string()).unwrap();
        }
        let content = std::fs::read_to_string(&p).unwrap();
        assert_eq!(content.matches(HEADER_BEGIN).count(), 1);
        assert_eq!(content.trim_end().lines().last().unwrap(), "a=3");
        let _ = std::fs::remove_file(&p);
    }
}
