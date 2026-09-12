//! 配置字段读写：调度层。
//!
//! 后端只向调用方提供「字段当前值 + 按 key 写回」；字段的呈现(布局/文案/控件)
//! 完全由前端表单决定，后端不携带任何展示信息。

pub use crate::component::ConfigFieldValue;
use crate::component::{fields, registry};

/// 列出组件的全部配置字段当前值。
pub fn list_fields(name: &str, version: &str) -> Result<Vec<ConfigFieldValue>, String> {
    let Some(c) = registry::by_component(name) else {
        return Ok(Vec::new());
    };
    let mut values = c.field_values(version);
    // 仅《需要 Java 且能落盘》的组件追加通用 jdk_version
    if !values.is_empty() && fields::supports_jdk(c) {
        values.push(fields::jdk_field(c, version));
    }
    Ok(values)
}

/// 设置字段值；`jdk_version` 走通用 JDK 应用。
pub fn set_field(name: &str, version: &str, field_id: &str, value: &str) -> Result<(), String> {
    let Some(c) = registry::by_component(name) else {
        return Err(format!("组件 {name} 未注册，无法设置配置"));
    };
    if field_id == "jdk_version" {
        if !fields::supports_jdk(c) {
            return Err(format!("组件 {name} 没有可配置的 JDK"));
        }
        return fields::apply_jdk(c, version, value);
    }
    c.set_field(version, field_id, value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn java_components_expose_jdk_field() {
        use crate::test_util::HOME_LOCK;
        let _guard = HOME_LOCK.lock().unwrap();
        let tmp = std::env::temp_dir().join("solostack-schema-test");
        std::env::set_var("HOME", &tmp);
        let _ = std::fs::remove_dir_all(&tmp);

        let fields = list_fields("hadoop", "3.5.0").unwrap();
        assert_eq!(fields.len(), 5);
        assert_eq!(fields[0].id, "namenode_web_port");
        assert_eq!(fields[2].id, "history_enabled");
        assert_eq!(
            fields[4].id, "jdk_version",
            "hadoop 有官方 hadoop-env.sh，可配 JDK"
        );

        // kafka 无官方环境文件，但由 SoloStack 生成 solostack-env.sh → 同样可配 JDK
        let kafka = list_fields("kafka", "4.3.1").unwrap();
        assert_eq!(kafka.len(), 6, "kafka 业务字段 5 个 + 通用 jdk_version");
        assert!(
            kafka.iter().any(|f| f.id == "jdk_version"),
            "kafka 的 JAVA_HOME 落在 solostack-env.sh，配置页应能选 JDK"
        );

        // 未注册组件 → 空（不 panic、不加孤立 jdk）
        assert!(list_fields("no-such", "0.0.0").unwrap().is_empty());

        let _ = std::fs::remove_dir_all(&tmp);
    }
}
