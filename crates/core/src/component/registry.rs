//! 组件静态注册表。

use super::Component;

/// 内置组件注册表。
///
/// 新增组件 = `package/manifest/<component>.json` + `component/<component>/{mod,config,runtime}.rs`
/// + 下面数组里追加一行（registry 是外部取组件的唯一入口）。
pub static REGISTRY: [&'static dyn Component; 2] = [&super::hadoop::Hadoop, &super::kafka::Kafka];

/// 全部注册组件。
pub fn all() -> &'static [&'static dyn Component] {
    &REGISTRY
}

/// 按组件名取组件实现（未注册返回 None，调用方须优雅降级）。
pub fn by_component(component: &str) -> Option<&'static dyn Component> {
    REGISTRY
        .iter()
        .copied()
        .find(|c| c.component() == component)
}

/// 组件展示名；未注册回退为组件名。
pub fn display_name(component: &str) -> String {
    by_component(component)
        .map(|c| c.display_name().to_string())
        .unwrap_or_else(|| component.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::package::manifest;

    /// manifest json 与代码注册表必须一一对应：加组件两处同进同出。
    #[test]
    fn registry_matches_manifests() {
        let json_names: Vec<String> = manifest::load_all()
            .unwrap()
            .into_iter()
            .map(|c| c.component)
            .collect();
        let code_names: Vec<String> = all().iter().map(|c| c.component().to_string()).collect();

        for name in &json_names {
            assert!(
                code_names.contains(name),
                "manifest 有 {name} 但未注册到 registry"
            );
        }
        for name in &code_names {
            assert!(
                json_names.contains(name),
                "registry 有 {name} 但缺少 manifest json"
            );
        }
        assert_eq!(json_names.len(), code_names.len());
    }
}
