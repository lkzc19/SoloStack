use serde::{Deserialize, Serialize};

/// 安装参数（收集用户配置后写入临时 `installs/<组件>-<4位Id>-install.json`，安装完成后删除）。
///
/// 只作为安装过程的数据载体；最终配置看 `etc/<组件>-<版本>/` 下的配置文件。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InstallConfig {
    pub component: String,
    pub version: String,
    pub source_id: String,
    /// 用户选择的 JDK 主版本（安装时解析出路径写入配置）。
    pub jdk_version: String,
    /// NameNode WebUI 端口（hadoop；0 = 默认 9870）。
    #[serde(default)]
    pub namenode_web: u16,
    /// YARN RM WebUI 端口（hadoop；0 = 默认 8088）。
    #[serde(default)]
    pub yarn_rm: u16,
    /// 是否开启历史服务器（hadoop）。
    #[serde(default)]
    pub history_enabled: bool,
    /// 历史服务器 WebUI 端口（hadoop；0 = 默认 19888）。
    #[serde(default)]
    pub history_web_port: u16,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serde_roundtrip() {
        let cfg = InstallConfig {
            component: "hadoop".into(),
            version: "3.5.0".into(),
            source_id: "清华源".into(),
            jdk_version: "17".into(),
            namenode_web: 9871,
            yarn_rm: 8089,
            history_enabled: true,
            history_web_port: 19888,
        };
        let json = serde_json::to_string(&cfg).unwrap();
        let back: InstallConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(back, cfg);
    }

    #[test]
    fn missing_ports_default_zero() {
        let cfg: InstallConfig = serde_json::from_str(
            r#"{"component":"kafka","version":"4.1.0","source_id":"官方源","jdk_version":"17"}"#,
        )
        .unwrap();
        assert_eq!(cfg.namenode_web, 0);
        assert_eq!(cfg.yarn_rm, 0);
    }
}
