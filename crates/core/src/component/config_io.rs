//! 组件配置文件的定位与打开。
//!
//! 把「组件配置在磁盘的哪个位置、按什么格式打开、安装参数是否声明过」这类
//! 机械操作，与 `component` 的能力契约（见 `mod.rs` 的 trait）分开。
//! 组件实现与生命周期层都经由本模块访问受管配置文件。

use std::path::PathBuf;

use super::{registry, ConfigLayout, InstallParams};
use crate::config::{self, ConfigFile};

/// 校验布局声明的配置文件都存在（缺失即报错，不静默用空配置启动）。
pub(crate) fn validate_layout(
    environment_id: &str,
    component: &str,
    version: &str,
    layout: &ConfigLayout,
) -> Result<(), String> {
    for file in layout.files {
        let path = config_path(environment_id, component, version, file)?;
        if !path.is_file() {
            return Err(format!(
                "组件配置缺失: {}（组件包布局与声明不符或文件被删除，请先卸载后重新安装）",
                path.display()
            ));
        }
    }
    Ok(())
}

/// 组件配置目录：`components/<组件>/<组件>-<版本>/<布局目录>`。
pub fn config_dir(environment_id: &str, component: &str, version: &str) -> Result<PathBuf, String> {
    let layout = registry::by_component(component)
        .map(|c| c.config_layout())
        .ok_or_else(|| format!("组件 {component} 未注册"))?;
    Ok(
        crate::app::paths::instance_dir(environment_id, component, version)
            .map_err(|e| e.to_string())?
            .join(layout.dir),
    )
}

/// 组件配置文件绝对路径。
pub fn config_path(
    environment_id: &str,
    component: &str,
    version: &str,
    file: &str,
) -> Result<PathBuf, String> {
    Ok(config_dir(environment_id, component, version)?.join(file))
}

/// 打开组件配置文件（按扩展名分发格式）。
pub fn open_config(
    environment_id: &str,
    component: &str,
    version: &str,
    file: &str,
) -> Result<Box<dyn ConfigFile>, String> {
    config::open(config_path(environment_id, component, version, file)?)
}

/// 校验安装参数都是该组件声明过的。
///
/// 防的是前后端参数 id 漂移：前端表单里写死的 id 若与组件声明不符，
/// 没有这道校验就会「静默忽略、用回默认值」，用户填的值消失且无提示。
pub fn validate_install_params(
    component: &str,
    version: &str,
    params: &InstallParams,
) -> Result<(), String> {
    let Some(c) = registry::by_component(component) else {
        return Err(format!("组件 {component} 未注册"));
    };
    let declared = c.install_params(version);
    let known: Vec<&str> = declared.iter().map(|p| p.id.as_str()).collect();
    for key in params.keys() {
        if !known.contains(&key.as_str()) {
            return Err(format!(
                "组件 {component} 不认识的安装参数: {key}（可用参数：{}）",
                known.join(", ")
            ));
        }
    }
    Ok(())
}

/// 启动 / 打开配置页前校验并补齐组件配置（未注册组件优雅降级为 Ok）。
pub fn prepare_config(environment_id: &str, component: &str, version: &str) -> Result<(), String> {
    if let Some(c) = registry::by_component(component) {
        c.ensure_config(environment_id, version)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prepare_unknown_component_is_noop() {
        // 未注册组件优雅降级：不 panic、不建目录，由上层给出「不支持的组件」
        assert!(prepare_config(
            "00000000-0000-4000-8000-000000000001",
            "no-such-component",
            "0.0.0"
        )
        .is_ok());
    }

    #[test]
    fn validate_install_params_rejects_unknown_ids() {
        // 声明的 id 通过
        let ok = InstallParams::from([("namenode_web_port".to_string(), "9870".to_string())]);
        assert!(validate_install_params("hadoop", "3.5.0", &ok).is_ok());
        // 没声明的 id 直接报错（而不是静默忽略、回退默认值）
        let bad = InstallParams::from([("namenode_web".to_string(), "9870".to_string())]);
        let err = validate_install_params("hadoop", "3.5.0", &bad).unwrap_err();
        assert!(err.contains("不认识的安装参数"), "{err}");
        assert!(
            err.contains("namenode_web_port"),
            "报错应列出可用参数: {err}"
        );
        // 未注册组件
        assert!(validate_install_params("no-such", "0.0.0", &InstallParams::new()).is_err());
    }

    #[test]
    fn config_path_follows_layout_dir() {
        use crate::test_util::HOME_LOCK;
        let _guard = HOME_LOCK.lock().unwrap();
        let tmp = std::env::temp_dir().join("solostack-component-path");
        std::env::set_var("HOME", &tmp);

        let environment_id = "00000000-0000-4000-8000-000000000001";
        let p = config_path(environment_id, "hadoop", "3.5.0", "hdfs-site.xml").unwrap();
        assert_eq!(
            p,
            tmp.join(".solostack/environments")
                .join(environment_id)
                .join("components/hadoop/hadoop-3.5.0/etc/hadoop/hdfs-site.xml")
        );

        let k = config_path(environment_id, "kafka", "4.3.1", "server.properties").unwrap();
        assert!(k.ends_with("kafka-4.3.1/config/server.properties"));
    }
}
