//! 组件抽象层：把组件差异收敛为多能力 trait + 每组件模块 + 静态注册表。
//!
//! 流程层(install / service / 命令层)只依赖 `Component` trait，不再写 `match name`
//! 组件分支。新增**后端组件** = package/manifest/<component>.json + component/<component>/
//! + registry 里加一行。进入发布支持列表还必须在 `src/lib/component-adapters/` 完成前端适配。
//!
//! 命名约定：`<component>` 指组件名（hadoop / kafka）。**不要用 `id` 指代组件名** ——
//! `id` 在项目里另有用途（安装参数 id、服务 id、实例标识等）。
//!
//! 配置**直接读写解压包里的官方配置文件**（不另存副本）：组件提供配置布局与
//! 语义读写，文件格式由 `config` 模块统一处理。

mod dto;
pub(crate) mod exec;
pub(crate) mod fields;
/// 已安装组件实例的发现（扫 `components/` 目录）。
pub mod instances;
// 安装参数 DTO 属于组件契约（apply_install_config 的入参）
pub mod install_config;
pub(crate) mod ports;
pub mod registry;
/// 配置字段调度：向后端其他层暴露「列出字段值 / 批量规划并保存」。
pub mod schema;

// 内置组件实现：每个组件一个目录，目录内按「配置 / 运行」分文件。
// 外部只经 `registry` 取组件，故不公开这些模块。
pub(crate) mod hadoop;
pub(crate) mod kafka;

pub use dto::{ConfigFieldUpdate, ConfigFieldValue, ConfigLayout, WebUi};
pub use install_config::{InstallParam, InstallParams};

use std::path::PathBuf;

use crate::config::{self, ConfigFile, ConfigPlan};
use crate::platform::process::ServiceSpec;

/// 配置字段能力：向前端提供「字段当前值」，并为整组字段变更生成写入计划。
pub trait FieldSchema {
    /// 全部可配置字段的当前值（呈现与布局由前端表单决定，这里只提供数据）。
    fn field_values(&self, version: &str) -> Vec<ConfigFieldValue>;
    /// 校验整组字段并生成文件写入计划；plan 阶段不允许写盘。
    fn plan_field_updates(
        &self,
        version: &str,
        updates: &[ConfigFieldUpdate],
    ) -> Result<ConfigPlan, String>;
}

/// 配置生命周期：配置布局 / 探活端口 / 安装生成 / 启动前校验补齐 / JDK 落点。
pub trait ConfigLifecycle {
    /// 组件名，必须等于 manifest/<component>.json 的文件名。
    /// 放这里而非 Component：默认方法(ensure_config)需要调用 self.component()。
    fn component(&self) -> &'static str;
    /// 官方配置目录 + 受管文件清单。
    fn config_layout(&self) -> ConfigLayout;
    /// 探活端口：从本组件配置文件**精确读**出实际生效的端口。
    /// 单项读不到时回退该项默认值（自愈），故实现不返回空。
    fn detect_ports(&self, version: &str) -> Vec<u16>;
    /// 声明本组件的安装参数（id + 默认值），供前端预填表单。
    ///
    /// 只声明「有哪些、默认多少」；类型与范围由 `apply_install_config` 解析时校验，
    /// 表单布局与文案完全由前端决定。无安装参数的组件用默认空实现。
    fn install_params(
        &self,
        _version: &str,
    ) -> Vec<crate::component::install_config::InstallParam> {
        Vec::new()
    }
    /// 安装时把安装参数落进官方配置文件（解析校验、端口占用避让都在此完成）。
    fn apply_install_config(
        &self,
        version: &str,
        params: &crate::component::install_config::InstallParams,
    ) -> Result<(), String>;
    /// 启动 / 打开配置页前校验并补齐受管配置。默认实现 = 校验配置文件存在。
    fn ensure_config(&self, version: &str) -> Result<(), String> {
        validate_layout(self.component(), version, &self.config_layout())
    }
    /// Java 组件的 JAVA_HOME 落点（相对实例的官方环境文件，如 hadoop-env.sh）。
    /// 返回 None = 该组件没有可承载 JAVA_HOME 的官方文件，启动时实时解析本机 JDK。
    fn java_env_file(&self) -> Option<&'static str> {
        None
    }
}

/// 运行生命周期：启停序列 / WebUI。
pub trait Runtime {
    /// 首次运行初始化；必须幂等，由生命周期层在 `start` 前调用。
    fn init(&self, _version: &str) -> Result<(), String> {
        Ok(())
    }
    fn start(&self, version: &str) -> Result<(), String>;
    fn stop(&self, version: &str) -> Result<(), String>;
    /// 该组件实例预期运行的服务、进程命令行特征和监听端口。
    fn service_specs(&self, _version: &str) -> Vec<ServiceSpec> {
        Vec::new()
    }
    /// WebUI 跳转地址(从配置精确读出的端口推导)，默认无。
    fn web_uis(&self, _version: &str) -> Vec<WebUi> {
        Vec::new()
    }
}

/// 组合超 trait：注册表只存 `&'static dyn Component`。
///
/// 组件状态全部在文件系统（struct 无字段），故方法全 `&self` 且天然满足对象安全。
/// `component()` 由 ConfigLifecycle 提供，经 supertrait 方法在 dyn Component 上直接可调。
pub trait Component: FieldSchema + ConfigLifecycle + Runtime + Send + Sync {
    /// GUI / CLI 展示名（默认 = id；需要更友好的名字时 override）。
    fn display_name(&self) -> &'static str {
        self.component()
    }
}

// ── 组件配置文件的定位与打开 ────────────────────────────

/// 校验布局声明的配置文件都存在（缺失即报错，不静默用空配置启动）。
pub(crate) fn validate_layout(
    component: &str,
    version: &str,
    layout: &ConfigLayout,
) -> Result<(), String> {
    for file in layout.files {
        let path = config_path(component, version, file)?;
        if !path.is_file() {
            return Err(format!(
                "组件配置缺失: {}（组件包布局与声明不符或文件被删除，可重装该组件修复）",
                path.display()
            ));
        }
    }
    Ok(())
}

/// 组件配置目录：`components/<组件>/<组件>-<版本>/<布局目录>`。
pub fn config_dir(component: &str, version: &str) -> Result<PathBuf, String> {
    let layout = registry::by_component(component)
        .map(|c| c.config_layout())
        .ok_or_else(|| format!("组件 {component} 未注册"))?;
    Ok(crate::app::paths::instance_dir(component, version)
        .map_err(|e| e.to_string())?
        .join(layout.dir))
}

/// 组件配置文件绝对路径。
pub fn config_path(component: &str, version: &str, file: &str) -> Result<PathBuf, String> {
    Ok(config_dir(component, version)?.join(file))
}

/// 打开组件配置文件（按扩展名分发格式）。
pub fn open_config(
    component: &str,
    version: &str,
    file: &str,
) -> Result<Box<dyn ConfigFile>, String> {
    config::open(config_path(component, version, file)?)
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
pub fn prepare_config(component: &str, version: &str) -> Result<(), String> {
    if let Some(c) = registry::by_component(component) {
        c.ensure_config(version)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prepare_unknown_component_is_noop() {
        // 未注册组件优雅降级：不 panic、不建目录，由上层给出「不支持的组件」
        assert!(prepare_config("no-such-component", "0.0.0").is_ok());
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

        let p = config_path("hadoop", "3.5.0", "hdfs-site.xml").unwrap();
        assert_eq!(
            p,
            tmp.join(".solostack/components/hadoop/hadoop-3.5.0/etc/hadoop/hdfs-site.xml")
        );

        let k = config_path("kafka", "4.3.1", "server.properties").unwrap();
        assert!(k.ends_with("kafka-4.3.1/config/server.properties"));
    }
}
