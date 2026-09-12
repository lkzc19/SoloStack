//! 组件通用字段助手：jdk_version 读写 + 数值解析。
//!
//! `jdk_version` 字段只对有「官方环境文件」的 Java 组件提供（如 hadoop 的
//! hadoop-env.sh）：读 = 从该文件精确读 JAVA_HOME 反查本机 JDK 目录名；
//! 写 = 解析 JDK 路径写回该文件。由调度层统一追加，不放进各组件的 field_values。

use super::dto::ConfigFieldValue;
use super::install_config::InstallParams;
use super::Component;
use crate::platform::jdk;

/// 组件《是否支持配置 JDK》= 需要 Java 且有官方环境文件可落盘。
pub fn supports_jdk(comp: &dyn Component) -> bool {
    crate::package::manifest::needs_java(comp.component()) && comp.java_env_file().is_some()
}

/// 构造通用 `jdk_version` 的当前值。
pub fn jdk_field(comp: &dyn Component, version: &str) -> ConfigFieldValue {
    ConfigFieldValue {
        id: "jdk_version".into(),
        value: jdk_value(comp, version),
    }
}

/// 读当前 JDK 目录名：从组件环境文件的 JAVA_HOME 反查本机 JDK 扫描结果。
fn jdk_value(comp: &dyn Component, version: &str) -> String {
    let Some(home) = read_java_home(comp, version) else {
        return String::new();
    };
    jdk::scan()
        .into_iter()
        .find(|j| j.path.display().to_string() == home)
        .map(|j| j.name)
        .unwrap_or_default()
}

/// 精确读组件环境文件里的 JAVA_HOME（组件无可落盘环境文件时返回 None）。
pub(crate) fn read_java_home(comp: &dyn Component, version: &str) -> Option<String> {
    let file = comp.java_env_file()?;
    let f = super::open_config(comp.component(), version, file).ok()?;
    let home = f.get("JAVA_HOME").ok().flatten()?;
    let home = home.trim().to_string();
    if home.is_empty() {
        None
    } else {
        Some(home)
    }
}

/// 应用 JDK：解析路径 → 写进组件官方环境文件的 JAVA_HOME。
pub fn apply_jdk(comp: &dyn Component, version: &str, value: &str) -> Result<(), String> {
    let file = comp.java_env_file().ok_or_else(|| {
        format!(
            "组件 {} 没有可承载 JAVA_HOME 的官方配置文件",
            comp.component()
        )
    })?;
    let home = super::exec::resolve_requested_jdk(value)?;
    let f = super::open_config(comp.component(), version, file)?;
    f.set("JAVA_HOME", &home)
}

// ── 安装参数解析（缺省 / 空串 = 用默认值）────────────────

/// 取一个安装参数；缺省或空串视为未提供。
pub fn param<'a>(params: &'a InstallParams, id: &str) -> Option<&'a str> {
    params.get(id).map(|v| v.trim()).filter(|v| !v.is_empty())
}

/// 取端口参数：未提供用 `default`，提供了则按端口规则校验（拒 0 与特权端口）。
pub fn param_port(params: &InstallParams, id: &str, default: u16) -> Result<u16, String> {
    match param(params, id) {
        Some(v) => parse_port(v),
        None => Ok(default),
    }
}

/// 取正整数参数：未提供用 `default`。
pub fn param_positive_int(
    params: &InstallParams,
    id: &str,
    default: u64,
    what: &str,
) -> Result<u64, String> {
    match param(params, id) {
        Some(v) => parse_positive_int(v, what),
        None => Ok(default),
    }
}

/// 取布尔参数：未提供用 `default`。
pub fn param_bool(params: &InstallParams, id: &str, default: bool) -> Result<bool, String> {
    match param(params, id) {
        Some("true") => Ok(true),
        Some("false") => Ok(false),
        Some(v) => Err(format!("{id} 取值无效: {v}（应为 true / false）")),
        None => Ok(default),
    }
}

/// 校验端口值：必须是 1024–65535 的十进制整数。
///
/// - 拒绝 **0**：`ports::or_default` 用 0 表示「未指定」，若把 0 写进配置，
///   读回时会被当成有效端口（`Some(0)`），探活永远连不上、状态永远 Stopped；
/// - 拒绝 **1–1023**：组件以普通用户身份启动，绑定特权端口必然失败。
pub fn parse_port(value: &str) -> Result<u16, String> {
    let port = value
        .trim()
        .parse::<u16>()
        .map_err(|_| format!("端口无效: {value}"))?;
    if port < 1024 {
        return Err(format!(
            "端口需在 1024–65535 之间（不能使用特权端口）: {port}"
        ));
    }
    Ok(port)
}

/// 校验正整数(分区数 / 保留时长等),必须 > 0。
pub fn parse_positive_int(value: &str, what: &str) -> Result<u64, String> {
    let n = value
        .trim()
        .parse::<u64>()
        .map_err(|_| format!("{what}无效: {value}"))?;
    if n == 0 {
        return Err(format!("{what}必须大于 0"));
    }
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_port_rejects_zero_and_privileged_ports() {
        // 0 会让「未指定」与「端口 0」混淆，读回时探活永远失败
        assert!(parse_port("0").is_err());
        // 组件以普通用户运行，绑不上特权端口
        assert!(parse_port("80").is_err());
        assert!(parse_port("1023").is_err());
        assert_eq!(parse_port(" 1024 ").unwrap(), 1024);
        assert_eq!(parse_port("65535").unwrap(), 65535);
        // 超出 u16 / 非数字
        assert!(parse_port("65536").is_err());
        assert!(parse_port("abc").is_err());
    }

    #[test]
    fn install_params_fall_back_to_defaults() {
        let mut params = super::InstallParams::new();
        // 未提供 / 空串 → 默认值
        assert_eq!(param_port(&params, "p", 9870).unwrap(), 9870);
        params.insert("p".into(), "   ".into());
        assert_eq!(param_port(&params, "p", 9870).unwrap(), 9870);
        // 提供了则校验
        params.insert("p".into(), "9871".into());
        assert_eq!(param_port(&params, "p", 9870).unwrap(), 9871);
        params.insert("p".into(), "0".into());
        assert!(param_port(&params, "p", 9870).is_err());

        assert!(param_bool(&params, "b", true).unwrap());
        params.insert("b".into(), "false".into());
        assert!(!param_bool(&params, "b", true).unwrap());
        params.insert("b".into(), "yes".into());
        assert!(param_bool(&params, "b", true).is_err());

        assert_eq!(param_positive_int(&params, "n", 168, "时长").unwrap(), 168);
        params.insert("n".into(), "24".into());
        assert_eq!(param_positive_int(&params, "n", 168, "时长").unwrap(), 24);
    }

    #[test]
    fn parse_positive_int_requires_positive() {
        assert!(parse_positive_int("0", "分区数").is_err());
        assert_eq!(parse_positive_int(" 7 ", "分区数").unwrap(), 7);
        assert!(parse_positive_int("-1", "分区数").is_err());
    }
}
