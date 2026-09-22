//! 应用内部随机 ID。

use nanoid::nanoid;

pub const SHORT_ID_LEN: usize = 8;

/// 生成新的 8 位 NanoID。
pub fn new_id() -> String {
    nanoid!(SHORT_ID_LEN)
}

/// 是否为当前格式的 8 位 NanoID。
pub fn is_short_id(value: &str) -> bool {
    value.len() == SHORT_ID_LEN
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// 环境 ID 使用当前 8 位 NanoID 格式。
pub fn is_environment_id(value: &str) -> bool {
    is_short_id(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_id_is_eight_char_nanoid() {
        let id = new_id();
        assert_eq!(id.len(), SHORT_ID_LEN);
        assert!(is_short_id(&id));
    }

    #[test]
    fn environment_id_accepts_only_nanoid() {
        assert!(is_environment_id("V1StGXR8"));
        assert!(!is_environment_id("550e8400-e29b-41d4-a716-446655440000"));
        assert!(!is_environment_id("../../etc"));
        assert!(!is_environment_id("short"));
    }
}
