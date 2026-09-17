//! 配置字段读写：调度层。
//!
//! 后端只向调用方提供「字段当前值 + 按 key 写回」；字段的呈现(布局/文案/控件)
//! 完全由前端表单决定，后端不携带任何展示信息。

use crate::component::exec;
use crate::component::{fields, registry};
pub use crate::component::{ConfigFieldUpdate, ConfigFieldValue};
use crate::config::{self, ConfigPlan};

/// 列出组件的全部配置字段当前值。
pub fn list_fields(
    environment_id: &str,
    name: &str,
    version: &str,
) -> Result<Vec<ConfigFieldValue>, String> {
    let Some(c) = registry::by_component(name) else {
        return Ok(Vec::new());
    };
    let mut values = c.field_values(environment_id, version);
    // 仅《需要 Java 且能落盘》的组件追加通用 jdk_version
    if !values.is_empty() && fields::supports_jdk(c) {
        values.push(fields::jdk_field(environment_id, c, version));
    }
    Ok(values)
}

/// 一次保存整组字段；JDK 与组件业务字段统一合并后事务落盘。
pub fn save_fields(
    environment_id: &str,
    name: &str,
    version: &str,
    updates: &[ConfigFieldUpdate],
) -> Result<(), String> {
    let operation = crate::app::app_log::Operation::begin(
        "save-config",
        name,
        version,
        &format!("保存 {name} v{version} 配置"),
    );
    let result = save_fields_inner(environment_id, name, version, updates);
    operation.finish(&result);
    result
}

fn save_fields_inner(
    environment_id: &str,
    name: &str,
    version: &str,
    updates: &[ConfigFieldUpdate],
) -> Result<(), String> {
    let Some(c) = registry::by_component(name) else {
        return Err(format!("组件 {name} 未注册，无法设置配置"));
    };

    let mut seen = std::collections::HashSet::new();
    for update in updates {
        if !seen.insert(update.id.as_str()) {
            return Err(format!("配置字段重复提交: {}", update.id));
        }
    }

    let mut plan = ConfigPlan::new();
    let business: Vec<ConfigFieldUpdate> = updates
        .iter()
        .filter(|update| update.id != "jdk_version")
        .cloned()
        .collect();
    if !business.is_empty() {
        plan.merge(c.plan_field_updates(environment_id, version, &business)?)?;
    }

    if let Some(update) = updates.iter().find(|update| update.id == "jdk_version") {
        if !fields::supports_jdk(c) {
            return Err(format!("组件 {name} 没有可配置的 JDK"));
        }
        let file = c
            .java_env_file()
            .ok_or_else(|| format!("组件 {name} 没有可承载 JAVA_HOME 的环境文件"))?;
        let home = exec::resolve_requested_jdk(&update.value)?;
        plan.set(
            component_path(environment_id, name, version, file)?,
            "JAVA_HOME",
            home,
        )?;
    }

    config::apply_plan(&plan).map_err(|error| {
        let message = format!("保存 {name} v{version} 配置失败: {error}");
        let _ = crate::app::app_log::error("config.save.failed", &message);
        message
    })
}

fn component_path(
    environment_id: &str,
    component: &str,
    version: &str,
    file: &str,
) -> Result<std::path::PathBuf, String> {
    crate::component::config_path(environment_id, component, version, file)
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

        let environment_id = "00000000-0000-4000-8000-000000000001";
        let fields = list_fields(environment_id, "hadoop", "3.5.0").unwrap();
        assert_eq!(fields.len(), 5);
        assert_eq!(fields[0].id, "namenode_web_port");
        assert_eq!(fields[2].id, "history_enabled");
        assert_eq!(
            fields[4].id, "jdk_version",
            "hadoop 有官方 hadoop-env.sh，可配 JDK"
        );

        // kafka 无官方环境文件，但由 SoloStack 生成 solostack-env.sh → 同样可配 JDK
        let kafka = list_fields(environment_id, "kafka", "4.3.1").unwrap();
        assert_eq!(kafka.len(), 6, "kafka 业务字段 5 个 + 通用 jdk_version");
        assert!(
            kafka.iter().any(|f| f.id == "jdk_version"),
            "kafka 的 JAVA_HOME 落在 solostack-env.sh，配置页应能选 JDK"
        );

        // 未注册组件 → 空（不 panic、不加孤立 jdk）
        assert!(list_fields(environment_id, "no-such", "0.0.0")
            .unwrap()
            .is_empty());

        let _ = std::fs::remove_dir_all(&tmp);
    }
}
