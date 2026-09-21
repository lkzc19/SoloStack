//! 日志脱敏：脚本参数、环境变量与文本中的敏感值。

/// 参数脱敏：密码、Token、Secret、Key、Cookie、Authorization。
pub fn redact_args(args: &[&str]) -> Vec<String> {
    let mut redact_next = false;
    args.iter()
        .map(|arg| {
            if redact_next {
                redact_next = false;
                return "<redacted>".to_string();
            }
            if let Some((key, _)) = arg.split_once('=') {
                if sensitive_key(key.trim_start_matches('-')) {
                    return format!("{key}=<redacted>");
                }
            }
            let trimmed = arg.trim_start_matches('-');
            if sensitive_key(trimmed) {
                redact_next = true;
                return (*arg).to_string();
            }
            redact_text(arg)
        })
        .collect()
}

/// 环境变量脱敏，仅返回安全或已脱敏的键值。
pub fn redact_envs(envs: &[(&str, &str)]) -> Vec<(String, String)> {
    envs.iter()
        .map(|(key, value)| {
            (
                (*key).to_string(),
                if sensitive_key(key) {
                    "<redacted>".to_string()
                } else {
                    redact_text(value)
                },
            )
        })
        .collect()
}

fn sensitive_key(key: &str) -> bool {
    let key = key.to_ascii_lowercase();
    [
        "password",
        "passwd",
        "token",
        "secret",
        "cookie",
        "authorization",
    ]
    .iter()
    .any(|marker| key.contains(marker))
        || key == "key"
        || key.ends_with("_key")
        || key.ends_with("-key")
}

/// 把文本里 `key=value` 形式的敏感值替换为 `<redacted>`。
///
/// 供写入路径（`store`）用于脱敏整条消息，故对 crate 内可见。
pub(super) fn redact_text(value: &str) -> String {
    let mut out = value.to_string();
    for marker in [
        "password=",
        "passwd=",
        "token=",
        "secret=",
        "cookie=",
        "authorization=",
        "api_key=",
        "apikey=",
    ] {
        let mut search_from = 0;
        while let Some(relative) = out[search_from..].to_ascii_lowercase().find(marker) {
            let start = search_from + relative + marker.len();
            let end = out[start..]
                .find(|c: char| c.is_whitespace() || c == '&' || c == ';')
                .map(|offset| start + offset)
                .unwrap_or(out.len());
            out.replace_range(start..end, "<redacted>");
            search_from = start + "<redacted>".len();
        }
    }
    out
}
