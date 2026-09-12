use std::collections::BTreeMap;

use serde::Serialize;

/// 安装参数的通用载体：`参数 id → 字符串值`。
///
/// 各组件有哪些安装参数、默认值是多少、怎么校验，**由组件自己声明**
/// （`ConfigLifecycle::install_params` + `apply_install_config`）；
/// 共享契约里不再出现任何组件专属字段，命令层也只做透传。
///
/// 值统一用字符串：与配置页 `set_config_field` 的约定一致，组件用
/// `component::fields::param_*` 解析并校验（缺省 / 空串 = 用默认值）。
pub type InstallParams = BTreeMap<String, String>;

/// 安装参数：安装过程的数据载体。
///
/// **不另存**——安装选项由组件直接写进官方配置文件（`ConfigLifecycle::apply_install_config`），
/// 之后配置文件就是唯一事实源；没有 install.json、也没有 etc/ 配置副本。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallConfig {
    pub component: String,
    pub version: String,
    pub source_id: String,
    /// 用户选择的 JDK（安装时解析出路径写入组件的环境文件）。
    pub jdk_version: String,
    /// 组件自定义安装参数（见 `InstallParams`）。
    pub params: InstallParams,
}

/// 一个安装参数的声明：id + 默认值（供前端预填）。
///
/// 只声明「有哪些参数、默认值是什么」；**类型与取值范围由组件解析时校验**
/// （如 `parse_port` 拒绝 0 与特权端口），表单的呈现方式完全由前端决定。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct InstallParam {
    pub id: String,
    pub default: String,
}

impl InstallParam {
    pub fn new(id: impl Into<String>, default: impl ToString) -> Self {
        InstallParam {
            id: id.into(),
            default: default.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn install_param_serializes_default_as_string() {
        let p = InstallParam::new("namenode_web_port", 9870);
        assert_eq!(p.id, "namenode_web_port");
        assert_eq!(p.default, "9870", "默认值统一以字符串交给前端");
    }

    #[test]
    fn params_default_to_empty_map() {
        let params = InstallParams::default();
        assert!(params.is_empty(), "缺省参数集为空 → 组件全部用默认值");
    }
}
