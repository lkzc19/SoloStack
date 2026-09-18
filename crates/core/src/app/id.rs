//! 应用内部随机 ID。
//!
//! 新 ID 统一使用 8 位 NanoID。环境 ID 额外兼容旧版 UUID，避免升级后
//! 无法读取已经存在的环境目录。

use nanoid::nanoid;
use uuid::Uuid;

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

/// 环境 ID 同时接受新 NanoID 和旧 UUID。
pub fn is_environment_id(value: &str) -> bool {
    is_short_id(value) || Uuid::parse_str(value).is_ok()
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
    fn environment_id_accepts_nanoid_and_legacy_uuid() {
        assert!(is_environment_id("V1StGXR8"));
        assert!(is_environment_id("550e8400-e29b-41d4-a716-446655440000"));
        assert!(!is_environment_id("../../etc"));
        assert!(!is_environment_id("short"));
    }
}
