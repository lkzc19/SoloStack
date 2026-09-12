//! `export KEY=VALUE` 行式配置（Hadoop 的 `hadoop-env.sh` 等 shell 环境文件）。
//!
//! 语法：`export KEY=VALUE` 或裸 `KEY=VALUE`，`#` 起始行为注释。
//! 只把「合法标识符 = 值」的行当配置，条件判断（`[ "$X" = "1" ]`）等脚本语句
//! 一律忽略 —— 这是能安全编辑 shell 脚本的关键。
//!
//! 键追加在文件**末尾**：shell 是后者覆盖前者，末尾写入才能确保生效。
//! 读取时容忍引号（`export JAVA_HOME="/x"` → `/x`）。

use std::path::{Path, PathBuf};

use super::line::{lookup, merge, remove};
use super::{commit, lines_of, read_opt, ConfigFile, Entry, Format};

pub struct ShellEnvFile {
    path: PathBuf,
}

impl ShellEnvFile {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        ShellEnvFile { path: path.into() }
    }
}

impl ConfigFile for ShellEnvFile {
    fn path(&self) -> &Path {
        &self.path
    }

    fn format(&self) -> Format {
        Format::ShellEnv
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

    fn update_entries(&self, additions: &[Entry]) -> Result<(), String> {
        let content = read_opt(&self.path)?;
        if content.trim().is_empty() {
            let lines: Vec<String> = additions.iter().map(|e| render(&e.key, &e.value)).collect();
            return commit(&self.path, self.comment_style(), &lines.join("\n"));
        }
        let body = merge(&content, additions, key_of, render);
        commit(&self.path, self.comment_style(), &body)
    }

    fn delete_entries(&self, keys: &[&str]) -> Result<(), String> {
        let content = read_opt(&self.path)?;
        if content.trim().is_empty() {
            return Ok(());
        }
        let body = remove(&content, keys, key_of);
        commit(&self.path, self.comment_style(), &body)
    }
}

/// 解析一行 → (键, 原始值)；注释行 / 脚本语句 / 非标识符键返回 None。
fn parse(line: &str) -> Option<(&str, &str)> {
    let t = line.trim();
    if t.is_empty() || t.starts_with('#') {
        return None;
    }
    let t = strip_export(t);
    let (k, v) = t.split_once('=')?;
    let k = k.trim();
    if k.is_empty() || !k.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return None;
    }
    Some((k, v))
}

fn key_of(line: &str) -> Option<&str> {
    parse(line).map(|(k, _)| k)
}

/// 去掉前导 `export`（要求后面跟空白，避免误伤 `exportFOO=1` 这类键）。
fn strip_export(line: &str) -> &str {
    match line.strip_prefix("export") {
        Some(rest) if rest.starts_with(char::is_whitespace) => rest.trim_start(),
        _ => line,
    }
}

/// 原始值 → 语义值：去首尾空白，成对引号剥掉。
fn value(raw: &str) -> String {
    let v = raw.trim();
    let quoted = v.len() >= 2
        && ((v.starts_with('"') && v.ends_with('"')) || (v.starts_with('\'') && v.ends_with('\'')));
    if quoted {
        v[1..v.len() - 1].to_string()
    } else {
        v.to_string()
    }
}

fn render(key: &str, value: &str) -> String {
    format!("export {key}={value}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{HEADER_BEGIN, HEADER_END};

    fn tmp(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("solostack-env-{name}.sh"))
    }

    const SAMPLE: &str = r#"# The java implementation to use.
# export JAVA_HOME=

# some setup
if [ "$HADOOP_HOME" = "" ]; then
  echo "no home"
fi
X=1
"#;

    #[test]
    fn script_statements_are_not_treated_as_config() {
        let p = tmp("parse");
        std::fs::write(&p, SAMPLE).unwrap();
        let f = ShellEnvFile::new(&p);
        let keys: Vec<String> = f
            .read_entries()
            .unwrap()
            .into_iter()
            .map(|e| e.key)
            .collect();
        assert_eq!(keys, vec!["X"], "条件判断不应被当成配置键，注释行也不算");
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn set_appends_export_at_end_and_keeps_script() {
        let p = tmp("append");
        std::fs::write(&p, SAMPLE).unwrap();
        let f = ShellEnvFile::new(&p);
        f.set("JAVA_HOME", "/opt/jdk-17").unwrap();

        let content = std::fs::read_to_string(&p).unwrap();
        assert!(content.contains("# export JAVA_HOME="), "原有注释行应保留");
        assert!(
            content.contains("if [ \"$HADOOP_HOME\" = \"\" ]; then"),
            "脚本语句应保留"
        );
        assert!(
            content.trim_end().ends_with("export JAVA_HOME=/opt/jdk-17"),
            "新键应追加末尾"
        );
        assert!(
            content.starts_with("# SoloStack:begin"),
            "头部说明应在文件顶部"
        );
        assert_eq!(f.get("JAVA_HOME").unwrap().as_deref(), Some("/opt/jdk-17"));
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn existing_export_is_replaced_in_place() {
        let p = tmp("replace");
        std::fs::write(&p, "# c\nexport JAVA_HOME=/old\nexport OTHER=1\n").unwrap();
        let f = ShellEnvFile::new(&p);
        f.set("JAVA_HOME", "/new").unwrap();

        let content = std::fs::read_to_string(&p).unwrap();
        assert!(content.contains("export JAVA_HOME=/new"));
        assert!(!content.contains("/old"), "旧值不应残留");
        assert!(content.contains("export OTHER=1"), "其它键应保留");
        assert!(content.contains("# c"), "注释应保留");
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn quoted_value_is_unquoted_on_read() {
        let p = tmp("quote");
        std::fs::write(&p, "export JAVA_HOME=\"/opt/jdk 21\"\n").unwrap();
        let f = ShellEnvFile::new(&p);
        assert_eq!(f.get("JAVA_HOME").unwrap().as_deref(), Some("/opt/jdk 21"));
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn repeated_writes_keep_single_header_and_value() {
        let p = tmp("idem");
        std::fs::write(&p, SAMPLE).unwrap();
        let f = ShellEnvFile::new(&p);
        for home in ["/a", "/b", "/c"] {
            f.set("JAVA_HOME", home).unwrap();
        }
        let content = std::fs::read_to_string(&p).unwrap();
        assert_eq!(content.matches(HEADER_BEGIN).count(), 1);
        assert_eq!(content.matches(HEADER_END).count(), 1);
        // 只数生效行（模板里的 `# export JAVA_HOME=` 是注释，不算）
        let active = content
            .lines()
            .filter(|l| !l.trim_start().starts_with('#') && l.contains("export JAVA_HOME="))
            .count();
        assert_eq!(active, 1, "不应堆积多份键");
        assert_eq!(f.get("JAVA_HOME").unwrap().as_deref(), Some("/c"));
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn remove_deletes_key_line_only() {
        let p = tmp("remove");
        std::fs::write(&p, "# c\nexport A=1\nexport B=2\n").unwrap();
        let f = ShellEnvFile::new(&p);
        f.remove("A").unwrap();
        let content = std::fs::read_to_string(&p).unwrap();
        assert!(!content.contains("export A=1"));
        assert!(content.contains("export B=2") && content.contains("# c"));
        let _ = std::fs::remove_file(&p);
    }
}
