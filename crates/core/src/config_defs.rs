/// 组件配置（config json）—— 下载源与 JDK 支持的事实来源。
///
/// 每种组件一个配置文件（`config_defs/<id>.json`），随 app 内置分发，
/// 初始化时复制到 `~/.solostack/configs/` 供查看。结构：
/// ```json
/// { "source": [{ "name": "官方源", "version": { "3.5.0": "URL" } }],
///   "java_support": { "3.5.0": [17, 21] } }
/// ```
use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// 一个组件的完整配置。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentConfig {
    /// 组件 id（由文件名决定，如 hadoop / kafka）。
    #[serde(skip)]
    pub id: String,
    /// GUI 显示名（如 Hadoop）。
    #[serde(default)]
    pub display_name: String,
    /// 分类（hadoop / kafka，用于归组与 logo）。
    #[serde(default)]
    pub category: String,
    /// 该组件的下载源列表。
    pub source: Vec<SourceDef>,
    /// 版本 → 支持的 JDK 主版本列表。
    pub java_support: BTreeMap<String, Vec<u16>>,
}

/// 一个下载源。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceDef {
    pub name: String,
    /// 版本 → 完整下载地址。
    pub version: BTreeMap<String, String>,
}

/// 内置组件配置（id, json）。
pub const BUILTIN: &[(&str, &str)] = &[
    ("hadoop", include_str!("config_defs/hadoop.json")),
    ("kafka", include_str!("config_defs/kafka.json")),
];

/// 加载全部组件配置。
pub fn load_all() -> Result<Vec<ComponentConfig>, String> {
    let mut configs = Vec::new();
    for (id, json) in BUILTIN {
        let mut cfg: ComponentConfig =
            serde_json::from_str(json).map_err(|e| format!("解析组件 {id} 配置失败: {e}"))?;
        cfg.id = id.to_string();
        configs.push(cfg);
    }
    Ok(configs)
}

/// 按 id 取组件配置。
pub fn component(id: &str) -> Result<ComponentConfig, String> {
    load_all()?
        .into_iter()
        .find(|c| c.id == id)
        .ok_or_else(|| format!("未找到组件配置: {id}"))
}

/// 解析该组件某下载源下某版本的完整下载地址。
pub fn resolve_url(component_id: &str, source_name: &str, version: &str) -> Result<String, String> {
    let cfg = component(component_id)?;
    let src = cfg
        .source
        .iter()
        .find(|s| s.name == source_name)
        .ok_or_else(|| format!("组件 {component_id} 无下载源 {source_name}"))?;
    src.version
        .get(version)
        .cloned()
        .ok_or_else(|| format!("下载源 {source_name} 无 {component_id} {version} 的下载地址"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_all_has_hadoop_and_kafka() {
        let configs = load_all().unwrap();
        assert_eq!(configs.len(), 2);
        assert!(configs.iter().any(|c| c.id == "hadoop"));
        assert!(configs.iter().any(|c| c.id == "kafka"));
    }

    #[test]
    fn hadoop_has_source_and_java_support() {
        let cfg = component("hadoop").unwrap();
        assert!(!cfg.source.is_empty());
        assert!(cfg.java_support.contains_key("3.5.0"));
        assert!(cfg.java_support["3.5.0"].contains(&17));
    }

    #[test]
    fn resolve_url_hadoop_3_5_0() {
        let url = resolve_url("hadoop", "官方源", "3.5.0").unwrap();
        assert!(url.ends_with("hadoop-3.5.0-aarch64.tar.gz"));
        assert!(resolve_url("hadoop", "清华源", "3.5.0").is_ok());
        assert!(resolve_url("hadoop", "官方源", "9.9.9").is_err());
        assert!(resolve_url("nope", "官方源", "3.5.0").is_err());
    }

    #[test]
    fn all_urls_are_ascii() {
        // 防历史教训：URL 里混入非 ASCII 乱码（如 ß 混进域名）导致下载失败
        for cfg in load_all().unwrap() {
            for src in &cfg.source {
                for url in src.version.values() {
                    assert!(url.is_ascii(), "{} {} URL 含非 ASCII 字符: {url}", cfg.id, src.name);
                }
            }
        }
    }
}
