//! 行式格式（properties / shell-env）的共享算法。
//!
//! 两种格式的差异只有「怎么解析一行、怎么渲染一行」，合并 / 删除 / 精确读的
//! 逻辑完全一致，故收敛在这里；各格式把自己的语法函数（`fn(&str) -> …`）传进来。
//!
//! 所有算法都遵循同一条保真原则：**只碰命中键所在的行，其余行逐字保留**。

use super::{lines_of, Entry};

/// 行式合并：命中键替换该行，未命中的新键追加到末尾，其余行逐字保留。
pub(super) fn merge(
    content: &str,
    additions: &[Entry],
    parse_key: fn(&str) -> Option<&str>,
    render: fn(&str, &str) -> String,
) -> String {
    let mut pending: Vec<&Entry> = additions.iter().collect();
    let mut lines: Vec<String> = Vec::new();
    for line in lines_of(content) {
        let hit = parse_key(&line).and_then(|k| pending.iter().position(|e| e.key == k));
        match hit {
            Some(pos) => {
                let e = pending.remove(pos);
                lines.push(render(&e.key, &e.value));
            }
            None => lines.push(line),
        }
    }
    for e in pending {
        lines.push(render(&e.key, &e.value));
    }
    lines.join("\n")
}

/// 行式删除：丢掉键命中的行，其余行逐字保留。
pub(super) fn remove(content: &str, keys: &[&str], parse_key: fn(&str) -> Option<&str>) -> String {
    lines_of(content)
        .into_iter()
        .filter(|line| parse_key(line).map(|k| !keys.contains(&k)).unwrap_or(true))
        .collect::<Vec<_>>()
        .join("\n")
}

/// 精确读：取第一个键匹配的行的值。
pub(super) fn lookup(
    content: &str,
    key: &str,
    parse: fn(&str) -> Option<(&str, &str)>,
    value: fn(&str) -> String,
) -> Option<String> {
    for line in lines_of(content) {
        if let Some((k, raw)) = parse(&line) {
            if k == key {
                return Some(value(raw));
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 测试用极简语法：`k=v`，`#` 注释，值原样。
    fn parse_key(line: &str) -> Option<&str> {
        let t = line.trim();
        if t.is_empty() || t.starts_with('#') {
            return None;
        }
        t.split_once('=').map(|(k, _)| k.trim())
    }

    fn parse(line: &str) -> Option<(&str, &str)> {
        let t = line.trim();
        if t.is_empty() || t.starts_with('#') {
            return None;
        }
        t.split_once('=')
    }

    fn render(k: &str, v: &str) -> String {
        format!("{k}={v}")
    }

    fn value(raw: &str) -> String {
        raw.trim().to_string()
    }

    #[test]
    fn merge_replaces_hit_and_appends_new() {
        let out = merge(
            "# c\na=1\nb=2\n",
            &[Entry::new("b", "20"), Entry::new("z", "9")],
            parse_key,
            render,
        );
        assert_eq!(out, "# c\na=1\nb=20\nz=9");
    }

    #[test]
    fn merge_keeps_unrelated_lines_byte_for_byte() {
        let original = "# header\n\n  a = 1  # note\nb=2\n# tail";
        let out = merge(original, &[Entry::new("b", "3")], parse_key, render);
        assert!(out.contains("  a = 1  # note"), "未命中行应逐字保留");
        assert!(out.contains("# header") && out.contains("# tail"));
        assert!(out.contains("b=3") && !out.contains("b=2"));
    }

    #[test]
    fn merge_replaces_only_first_duplicate() {
        // 重复键：只替换首个，第二个原样留下（读取取首个，行为可预测）
        let out = merge("k=1\nk=2\n", &[Entry::new("k", "9")], parse_key, render);
        assert_eq!(out, "k=9\nk=2");
    }

    #[test]
    fn remove_drops_matching_lines_only() {
        let out = remove("# c\na=1\nb=2\n", &["b"], parse_key);
        assert_eq!(out, "# c\na=1");
    }

    #[test]
    fn lookup_reads_first_match_and_none_when_absent() {
        let content = "# c\na=1\nb=2\n";
        assert_eq!(lookup(content, "b", parse, value).as_deref(), Some("2"));
        assert_eq!(lookup(content, "zzz", parse, value), None);
    }
}
