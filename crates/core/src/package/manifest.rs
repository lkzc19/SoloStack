/// 组件清单（manifest json）—— 下载源与运行环境（JDK）的事实来源。
///
/// 每种组件一份清单文件（`manifest/<component>.json`），随 app 内置分发。
/// 结构：
/// ```json
/// { "source": [{ "name": "官方源", "version": {
///   "3.5.0": { "url": "URL", "sha256": "..." }
/// } }],
///   "java_support": { "3.5.0": [17, 21] } }
/// ```
use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// 一个组件的安装清单：它从哪些源下载、需要什么运行环境。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    /// 组件名（由文件名决定，如 hadoop / kafka）。
    #[serde(skip)]
    pub component: String,
    /// 该组件的下载源列表。
    pub source: Vec<SourceDef>,
    /// 版本 → 支持的 JDK 主版本列表。缺失 / 为空表示该组件不需要 Java。
    #[serde(default)]
    pub java_support: BTreeMap<String, Vec<u16>>,
}

/// 该组件是否需要 Java 运行时（安装时是否需要选 JDK、启动是否注入 JAVA_HOME）。
pub fn needs_java(component_id: &str) -> bool {
    by_component(component_id)
        .map(|m| !m.java_support.is_empty())
        .unwrap_or(false)
}

/// 一个下载源。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceDef {
    pub name: String,
    /// 版本 → 安装包地址与固定摘要。
    pub version: BTreeMap<String, ArtifactDef>,
}

/// 一个可下载安装包。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactDef {
    pub url: String,
    pub sha256: String,
}

/// 内置组件清单（组件名, json）。
pub const BUILTIN: &[(&str, &str)] = &[
    ("hadoop", include_str!("manifest/hadoop.json")),
    ("kafka", include_str!("manifest/kafka.json")),
];

/// 加载全部组件清单。
pub fn load_all() -> Result<Vec<Manifest>, String> {
    let mut manifests = Vec::new();
    for (component, json) in BUILTIN {
        let mut m: Manifest = serde_json::from_str(json)
            .map_err(|e| format!("解析组件 {component} 清单失败: {e}"))?;
        m.component = component.to_string();
        manifests.push(m);
    }
    Ok(manifests)
}

/// 按组件名取组件清单。
pub fn by_component(component: &str) -> Result<Manifest, String> {
    load_all()?
        .into_iter()
        .find(|m| m.component == component)
        .ok_or_else(|| format!("未找到组件清单: {component}"))
}

/// 解析该组件某下载源下某版本的安装包定义。
pub fn resolve_artifact(
    component_id: &str,
    source_name: &str,
    version: &str,
) -> Result<ArtifactDef, String> {
    let m = by_component(component_id)?;
    let src = m
        .source
        .iter()
        .find(|s| s.name == source_name)
        .ok_or_else(|| format!("组件 {component_id} 无下载源 {source_name}"))?;
    src.version
        .get(version)
        .cloned()
        .ok_or_else(|| format!("下载源 {source_name} 无 {component_id} {version} 的下载地址"))
}

/// 便捷解析完整下载地址。
pub fn resolve_url(component_id: &str, source_name: &str, version: &str) -> Result<String, String> {
    Ok(resolve_artifact(component_id, source_name, version)?.url)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_all_has_hadoop_and_kafka() {
        let manifests = load_all().unwrap();
        assert_eq!(manifests.len(), 2);
        assert!(manifests.iter().any(|m| m.component == "hadoop"));
        assert!(manifests.iter().any(|m| m.component == "kafka"));
    }

    #[test]
    fn hadoop_has_source_and_java_support() {
        let m = by_component("hadoop").unwrap();
        assert!(!m.source.is_empty());
        assert!(m.java_support.contains_key("3.5.0"));
        assert!(m.java_support["3.5.0"].contains(&17));
    }

    #[test]
    fn resolve_url_hadoop_3_5_0() {
        let url = resolve_url("hadoop", "官方源", "3.5.0").unwrap();
        assert!(url.ends_with("hadoop-3.5.0-aarch64.tar.gz"));
        let artifact = resolve_artifact("hadoop", "官方源", "3.5.0").unwrap();
        assert_eq!(artifact.sha256.len(), 64);
        assert!(resolve_url("hadoop", "清华源", "3.5.0").is_ok());
        assert!(resolve_url("hadoop", "官方源", "9.9.9").is_err());
        assert!(resolve_url("nope", "官方源", "3.5.0").is_err());
    }

    /// 结构不变量：每个源必须列出**同一批版本**，且每个版本都必须声明 java_support。
    ///
    /// 前者保证「选任何源都能装到下拉框里的版本」（不然会出现选了某个源却没有 URL）；
    /// 后者保证 Java 组件装完能选到 JDK（否则安装页 JDK 下拉全灰、装不下去）。
    #[test]
    fn sources_and_java_support_are_consistent() {
        for m in load_all().unwrap() {
            let first: Vec<String> = m.source[0].version.keys().cloned().collect();
            for src in &m.source {
                let versions: Vec<String> = src.version.keys().cloned().collect();
                assert_eq!(
                    versions, first,
                    "{} 的 {} 版本集与其他源不一致",
                    m.component, src.name
                );
            }
            for v in &first {
                assert!(
                    m.java_support.contains_key(v),
                    "{} 的 {} 缺少 java_support 声明",
                    m.component,
                    v
                );
            }
        }
    }

    #[test]
    fn all_urls_are_ascii() {
        // 防历史教训：URL 里混入非 ASCII 乱码（如 ß 混进域名）导致下载失败
        for m in load_all().unwrap() {
            for src in &m.source {
                for artifact in src.version.values() {
                    assert!(
                        artifact.url.is_ascii(),
                        "{} {} URL 含非 ASCII 字符: {}",
                        m.component,
                        src.name,
                        artifact.url
                    );
                }
            }
        }
    }

    #[test]
    fn artifact_sha256_is_valid_and_shared_by_sources() {
        for m in load_all().unwrap() {
            let first = &m.source[0];
            for (version, artifact) in &first.version {
                assert_eq!(artifact.sha256.len(), 64, "{version} SHA256 长度错误");
                assert!(
                    artifact
                        .sha256
                        .chars()
                        .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()),
                    "{version} SHA256 必须是小写十六进制"
                );
                for src in &m.source[1..] {
                    assert_eq!(
                        src.version[version].sha256, artifact.sha256,
                        "{} 的 {version} 在不同下载源摘要不一致",
                        m.component
                    );
                }
            }
        }
    }
}
