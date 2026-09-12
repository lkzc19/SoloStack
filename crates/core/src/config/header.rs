//! 受管说明头：往 SoloStack 写过的配置文件里注入一段声明，告诉用户
//! 「这个文件由谁管、受管键会被重写、手改会怎样」。
//!
//! 区块用两条行级 ASCII 哨兵围成，因此可以 grep、可以幂等重写：
//!
//! ```text
//! # SoloStack:begin · auto-managed block, do not edit
//! # ...说明正文...
//! # SoloStack:end
//! ```
//!
//! 设计说明见 `docs/Config-File-Design.md` §5。

use super::CommentStyle;

/// 受管区块起始哨兵（行级 ASCII，可 grep）。
pub const HEADER_BEGIN: &str = "SoloStack:begin";
/// 受管区块结束哨兵。
pub const HEADER_END: &str = "SoloStack:end";

/// 头部说明正文（不含哨兵与注释前缀）。
///
/// 两条硬约束，改文案时务必守住：
/// - XML 注释内不允许出现 `--`，勿用 ASCII 双连字符；
/// - **不得出现哨兵字面量**（`SoloStack:begin` / `SoloStack:end`），否则剥离逻辑
///   会把说明文字误认成区块边界。
const HEADER_NOTICE: &[&str] = &[
    "本文件由 SoloStack 写入与管理。SoloStack 是 macOS 本地大数据组件管理器，",
    "直接读写组件实例内的配置文件，不另存副本。",
    "",
    "写入语义：受管键整体重写。SoloStack 每次在应用内修改配置，都会重写本文件中",
    "由它管理的键值。请勿手动编辑这些键值，你的改动会在下次应用内修改时被覆盖。",
    "需要长期保留的自定义配置，请记录在 SoloStack 之外。",
    "",
    "区块边界由上下两条自动管理标记围成，改动标记会导致重新注入。",
];

/// 渲染受管头部（每行一条，含哨兵）。
fn render(style: CommentStyle) -> Vec<String> {
    let mut out = Vec::with_capacity(HEADER_NOTICE.len() + 4);
    match style {
        CommentStyle::Xml => {
            out.push(format!(
                "<!-- {HEADER_BEGIN} · auto-managed block, do not edit -->"
            ));
            out.push("<!--".to_string());
            for line in HEADER_NOTICE {
                out.push(if line.is_empty() {
                    String::new()
                } else {
                    format!("  {line}")
                });
            }
            out.push("-->".to_string());
            out.push(format!("<!-- {HEADER_END} -->"));
        }
        CommentStyle::Hash => {
            out.push(format!(
                "# {HEADER_BEGIN} · auto-managed block, do not edit"
            ));
            for line in HEADER_NOTICE {
                out.push(if line.is_empty() {
                    "#".to_string()
                } else {
                    format!("# {line}")
                });
            }
            out.push(format!("# {HEADER_END}"));
        }
    }
    out
}

/// 剥离已有受管区块（原地）。
///
/// 只剥离**成对且顺序正常**的标记区间；标记残缺时仅摘掉标记行本身 —— 绝不
/// 连坐删除其它内容。这一点很关键：如 shell 环境文件的说明块在文件顶部、
/// 受管键在文件末尾，若「只有 begin 就剥到末尾」，用户删掉一个 end 标记就会
/// 连带清空整份官方脚本。
pub(super) fn strip(lines: &mut Vec<String>) {
    let begin = lines.iter().position(|l| l.contains(HEADER_BEGIN));
    let end = lines.iter().position(|l| l.contains(HEADER_END));
    match (begin, end) {
        (Some(b), Some(e)) if e > b => {
            // 成对且顺序正常：连同块后一个空行一起剥掉，避免反复写入堆积空行
            let mut to = e;
            if to + 1 < lines.len() && lines[to + 1].trim().is_empty() {
                to += 1;
            }
            lines.drain(b..=to);
        }
        // 顺序异常（end 在 begin 之前）：两个标记各自孤立，逐个摘掉
        (Some(b), Some(e)) => {
            lines.remove(b.max(e));
            lines.remove(b.min(e));
        }
        (Some(i), None) | (None, Some(i)) => {
            lines.remove(i);
        }
        (None, None) => {}
    }
}

/// 注入受管头部（原地，块后留一个空行）。
pub(super) fn inject(lines: &mut Vec<String>, style: CommentStyle) {
    let at = anchor_index(lines, style);
    let block = render(style);
    let n = block.len();
    for (offset, line) in block.into_iter().enumerate() {
        lines.insert(at + offset, line);
    }
    lines.insert(at + n, String::new());
}

/// 头部锚点：XML 在最后一个处理指令之后（声明必须在最前，注释不能插到它上面）；
/// 行注释格式在文件最顶部。
fn anchor_index(lines: &[String], style: CommentStyle) -> usize {
    match style {
        CommentStyle::Hash => 0,
        CommentStyle::Xml => {
            let mut idx = 0;
            for (i, line) in lines.iter().enumerate() {
                let t = line.trim_start();
                if t.is_empty() || t.starts_with("<?") {
                    idx = i + 1;
                } else {
                    break;
                }
            }
            idx
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn notice_text_has_no_double_hyphen() {
        // XML 注释内不允许出现 "--"，否则文件会解析失败
        for line in HEADER_NOTICE {
            assert!(!line.contains("--"), "头部文案含非法双连字符: {line}");
        }
    }

    #[test]
    fn notice_text_has_no_marker_literals() {
        // 说明正文若含哨兵字面量，剥离逻辑会把文字误认成边界
        for line in HEADER_NOTICE {
            assert!(
                !line.contains(HEADER_BEGIN) && !line.contains(HEADER_END),
                "头部文案不应出现哨兵字面量: {line}"
            );
        }
    }

    #[test]
    fn strip_never_eats_content_on_orphan_markers() {
        let mut l = lines(&["# SoloStack:begin", "keep=1", "keep=2"]);
        strip(&mut l);
        assert_eq!(l, ["keep=1", "keep=2"], "孤立 begin 不得连坐删除后续内容");

        let mut l = lines(&["keep=1", "# SoloStack:end"]);
        strip(&mut l);
        assert_eq!(l, ["keep=1"], "孤立 end 只摘掉标记行");

        // 顺序反转（end 在 begin 之前）同样按残缺处理
        let mut l = lines(&["# SoloStack:end", "keep=1", "# SoloStack:begin"]);
        strip(&mut l);
        assert_eq!(l, ["keep=1"], "顺序异常的标记对不得吞掉中间内容");
    }

    #[test]
    fn strip_removes_complete_block_with_trailing_blank() {
        let mut l = lines(&[
            "# SoloStack:begin",
            "# notice",
            "# SoloStack:end",
            "",
            "a=1",
        ]);
        strip(&mut l);
        assert_eq!(l, ["a=1"], "成对标记连同其后空行一起剥掉");
    }

    #[test]
    fn xml_anchor_goes_after_processing_instructions() {
        let l = lines(&[
            "<?xml version=\"1.0\"?>",
            "<?xml-stylesheet type=\"text/xsl\" href=\"configuration.xsl\"?>",
            "<!-- license -->",
            "<configuration>",
        ]);
        assert_eq!(
            anchor_index(&l, CommentStyle::Xml),
            2,
            "应插在最后一个 PI 之后"
        );

        // 无声明时插到最前
        let l = lines(&["<configuration>", "</configuration>"]);
        assert_eq!(anchor_index(&l, CommentStyle::Xml), 0);
    }

    #[test]
    fn hash_anchor_is_file_top() {
        let l = lines(&["#!/bin/bash", "echo hi"]);
        assert_eq!(anchor_index(&l, CommentStyle::Hash), 0);
    }

    #[test]
    fn inject_then_strip_roundtrips() {
        let mut l = lines(&["a=1", "b=2"]);
        inject(&mut l, CommentStyle::Hash);
        assert!(l[0].starts_with("# SoloStack:begin"));
        assert_eq!(l.iter().filter(|x| x.contains(HEADER_BEGIN)).count(), 1);
        strip(&mut l);
        assert_eq!(l, ["a=1", "b=2"], "注入再剥离应还原");
    }
}
