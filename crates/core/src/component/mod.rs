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

/// 组件受管配置文件的定位与打开（路径解析、格式分发、安装参数校验）。
pub mod config_io;
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

use crate::config::ConfigPlan;
use crate::platform::process::ServiceSpec;

/// 配置字段能力：向前端提供「字段当前值」，并为整组字段变更生成写入计划。
pub trait FieldSchema {
    /// 全部可配置字段的当前值（呈现与布局由前端表单决定，这里只提供数据）。
    fn field_values(&self, environment_id: &str, version: &str) -> Vec<ConfigFieldValue>;
    /// 校验整组字段并生成文件写入计划；plan 阶段不允许写盘。
    fn plan_field_updates(
        &self,
        environment_id: &str,
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
    fn detect_ports(&self, environment_id: &str, version: &str) -> Vec<u16>;
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
        environment_id: &str,
        version: &str,
        params: &crate::component::install_config::InstallParams,
    ) -> Result<(), String>;
    /// 启动 / 打开配置页前校验并补齐受管配置。默认实现 = 校验配置文件存在。
    fn ensure_config(&self, environment_id: &str, version: &str) -> Result<(), String> {
        config_io::validate_layout(
            environment_id,
            self.component(),
            version,
            &self.config_layout(),
        )
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
    fn init(&self, _environment_id: &str, _version: &str) -> Result<(), String> {
        Ok(())
    }
    fn start(&self, environment_id: &str, version: &str) -> Result<(), String>;
    fn stop(&self, environment_id: &str, version: &str) -> Result<(), String>;
    /// 该组件实例预期运行的服务、进程命令行特征和监听端口。
    fn service_specs(&self, _environment_id: &str, _version: &str) -> Vec<ServiceSpec> {
        Vec::new()
    }
    /// WebUI 跳转地址(从配置精确读出的端口推导)，默认无。
    fn web_uis(&self, _environment_id: &str, _version: &str) -> Vec<WebUi> {
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
