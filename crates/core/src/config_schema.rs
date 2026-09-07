//! 组件语义化配置字段：每个组件暴露一组「语义字段」，字段的 getter/setter 负责
//! 「语义值 ↔ 底层配置」的转换（解析 URL/地址端口段、读写 `.java-home` / `hadoop-env.sh` 等）。
//!
//! 前端配置页不直接暴露 XML 键值对，而是渲染这些字段的表单。

use std::path::Path;

use crate::config::{self, ConfigProperty};

/// 一个语义化配置字段（前端展示用）。
#[derive(Debug, Clone, serde::Serialize)]
pub struct FieldMeta {
    pub id: String,
    pub label: String,
    /// 值类型：`port` / `version` / `bool`。
    pub value_type: String,
    pub value: String,
}

/// 列出组件的语义化配置字段（含当前值）。
pub fn list_fields(name: &str, version: &str) -> Result<Vec<FieldMeta>, String> {
    match name {
        "hadoop" => Ok(vec![
            FieldMeta {
                id: "namenode_web_port".into(),
                label: "HDFS WebUI 端口".into(),
                value_type: "port".into(),
                value: get_hadoop_namenode_web(name, version),
            },
            FieldMeta {
                id: "yarn_rm_web_port".into(),
                label: "YARN WebUI 端口".into(),
                value_type: "port".into(),
                value: get_hadoop_yarn_rm(name, version),
            },
            FieldMeta {
                id: "history_enabled".into(),
                label: "JobHistory".into(),
                value_type: "bool".into(),
                value: get_history_enabled(name, version),
            },
            FieldMeta {
                id: "history_web_port".into(),
                label: "JobHistory WebUI 端口".into(),
                value_type: "port".into(),
                value: get_history_web_port(name, version),
            },
            FieldMeta {
                id: "jdk_version".into(),
                label: "JDK 版本".into(),
                value_type: "version".into(),
                value: get_jdk_version(name, version),
            },
        ]),
        "kafka" => Ok(vec![
            FieldMeta {
                id: "broker_port".into(),
                label: "Broker 端口".into(),
                value_type: "port".into(),
                value: get_kafka_broker(name, version),
            },
            FieldMeta {
                id: "jdk_version".into(),
                label: "JDK 版本".into(),
                value_type: "version".into(),
                value: get_jdk_version(name, version),
            },
        ]),
        _ => Ok(vec![]),
    }
}

/// 设置字段值（按组件 + 字段 id 分派）。
pub fn set_field(name: &str, version: &str, field_id: &str, value: &str) -> Result<(), String> {
    match (name, field_id) {
        ("hadoop", "namenode_web_port") => set_hadoop_namenode_web(name, version, value),
        ("hadoop", "yarn_rm_web_port") => set_hadoop_yarn_rm(name, version, value),
        ("hadoop", "history_enabled") => set_history_enabled(name, version, value),
        ("hadoop", "history_web_port") => set_history_web_port(name, version, value),
        ("hadoop", "jdk_version") => set_jdk_version(name, version, value, true),
        ("kafka", "broker_port") => set_kafka_broker(name, version, value),
        ("kafka", "jdk_version") => set_jdk_version(name, version, value, false),
        _ => Err(format!("未知配置字段: {field_id}")),
    }
}

// ── hadoop：HDFS WebUI 端口 ─────────────────────────
fn get_hadoop_namenode_web(name: &str, version: &str) -> String {
    read_config_prop(name, version, "hdfs-site.xml", "dfs.namenode.http-address")
        .map(|v| parse_host_port(&v))
        .unwrap_or_else(|| "9870".into())
}

fn set_hadoop_namenode_web(name: &str, version: &str, value: &str) -> Result<(), String> {
    let port = parse_port_value(value)?;
    set_config_prop(name, version, "hdfs-site.xml", "dfs.namenode.http-address", &format!("localhost:{port}"))?;
    update_detect_port(name, version, 0, port)
}

// ── hadoop：YARN WebUI 端口 ─────────────────────────
fn get_hadoop_yarn_rm(name: &str, version: &str) -> String {
    read_config_prop(name, version, "yarn-site.xml", "yarn.resourcemanager.webapp.address")
        .map(|v| parse_host_port(&v))
        .unwrap_or_else(|| "8088".into())
}

fn set_hadoop_yarn_rm(name: &str, version: &str, value: &str) -> Result<(), String> {
    let port = parse_port_value(value)?;
    set_config_prop(name, version, "yarn-site.xml", "yarn.resourcemanager.webapp.address", &format!("localhost:{port}"))?;
    update_detect_port(name, version, 2, port)
}

// ── hadoop：JobHistory（开启状态存在 .detect-ports 长度里） ──
fn get_history_enabled(name: &str, version: &str) -> String {
    if config::history_enabled(name, version) { "true".into() } else { "false".into() }
}

fn get_history_web_port(name: &str, version: &str) -> String {
    let ports = config::read_detect_ports(name, version);
    if ports.len() > 4 { ports[4].to_string() } else { "19888".into() }
}

fn set_history_enabled(name: &str, version: &str, value: &str) -> Result<(), String> {
    let mut ports = config::read_detect_ports(name, version);
    if value.trim() == "true" {
        if ports.len() <= 4 {
            ports.push(19888);
            config::write_detect_ports(name, version, &ports)?;
            crate::configgen::write_mapred_config(name, version, 19888)?;
        }
    } else if ports.len() > 4 {
        ports.truncate(4);
        config::write_detect_ports(name, version, &ports)?;
    }
    Ok(())
}

fn set_history_web_port(name: &str, version: &str, value: &str) -> Result<(), String> {
    let port = parse_port_value(value)?;
    let mut ports = config::read_detect_ports(name, version);
    if ports.len() > 4 {
        ports[4] = port;
        config::write_detect_ports(name, version, &ports)?;
        crate::configgen::write_mapred_config(name, version, port)?;
    }
    Ok(())
}

// ── kafka：Broker 端口 ───────────────────────────────
fn get_kafka_broker(name: &str, version: &str) -> String {
    let Ok(path) = config::config_file_path(name, version, "server.properties") else {
        return "9092".into();
    };
    read_property(&path, "listeners")
        .map(|v| parse_broker_port(&v))
        .unwrap_or_else(|| "9092".into())
}

fn set_kafka_broker(name: &str, version: &str, value: &str) -> Result<(), String> {
    let port = parse_port_value(value)?;
    let path = config::config_file_path(name, version, "server.properties")?;
    let listeners = read_property(&path, "listeners").unwrap_or_else(|| "PLAINTEXT://:9092".into());
    let new_listeners = replace_broker_port(&listeners, port);
    write_property(&path, "listeners", &new_listeners)?;
    update_detect_port(name, version, 0, port)
}

// ── JDK 版本（通用）──────────────────────────────────
fn get_jdk_version(name: &str, version: &str) -> String {
    let Some(home) = config::read_java_home(name, version) else {
        return String::new();
    };
    crate::jdk::scan()
        .into_iter()
        .find(|j| j.path.display().to_string() == home)
        .map(|j| j.name)
        .unwrap_or_default()
}

fn set_jdk_version(name: &str, version: &str, value: &str, is_hadoop: bool) -> Result<(), String> {
    let jdk = crate::jdk::find_by_name(value.trim())
        .or_else(|| crate::jdk::find_matching(value.trim()))
        .ok_or_else(|| format!("本机未找到 JDK {value}"))?;
    let home = jdk.path.display().to_string();
    config::write_java_home(name, version, &home)?;
    if is_hadoop {
        config::write_hadoop_env_java_home(name, version, &home)?;
    }
    Ok(())
}

// ── 底层 helper ─────────────────────────────────────
/// 读单个 XML 属性值。
fn read_config_prop(name: &str, version: &str, file: &str, prop: &str) -> Option<String> {
    let props = config::read_config_props(name, version, file).ok()?;
    props.into_iter().find(|p| p.name == prop).map(|p| p.value)
}

/// 写单个 XML 属性值（读-改-写）。
fn set_config_prop(name: &str, version: &str, file: &str, prop: &str, value: &str) -> Result<(), String> {
    let mut props = config::read_config_props(name, version, file)?;
    if let Some(p) = props.iter_mut().find(|p| p.name == prop) {
        p.value = value.to_string();
    } else {
        props.push(ConfigProperty { name: prop.to_string(), value: value.to_string() });
    }
    config::write_config_props(name, version, file, &props)
}

/// 更新探活端口 `index` 位置的值并写回 `.detect-ports`。
fn update_detect_port(name: &str, version: &str, index: usize, port: u16) -> Result<(), String> {
    let mut ports = config::read_detect_ports(name, version);
    if ports.is_empty() {
        ports = match name {
            "hadoop" => vec![9870, 9864, 8088, 8042],
            "kafka" => vec![9092],
            _ => vec![],
        };
    }
    if let Some(p) = ports.get_mut(index) {
        *p = port;
    }
    config::write_detect_ports(name, version, &ports)
}

/// 解析 `localhost:9870` / `0.0.0.0:9092` 这类「主机:端口」里的端口。
fn parse_host_port(s: &str) -> String {
    s.rsplit(':').next().unwrap_or("").trim().to_string()
}

/// 解析 kafka `listeners` 里的第一个监听端口。
fn parse_broker_port(listeners: &str) -> String {
    let Some(idx) = listeners.find("://:") else {
        return String::new();
    };
    let rest = &listeners[idx + "://:".len()..];
    rest.chars().take_while(|c| c.is_ascii_digit()).collect()
}

/// 替换 kafka `listeners` 里第一个监听端口。
fn replace_broker_port(listeners: &str, port: u16) -> String {
    let Some(idx) = listeners.find("://:") else {
        return listeners.to_string();
    };
    let prefix_end = idx + "://:".len();
    let rest = &listeners[prefix_end..];
    let digit_len = rest.chars().take_while(|c| c.is_ascii_digit()).count();
    let mut out = String::new();
    out.push_str(&listeners[..prefix_end]);
    out.push_str(&port.to_string());
    out.push_str(&rest[digit_len..]);
    out
}

/// 校验端口值。
fn parse_port_value(value: &str) -> Result<u16, String> {
    value.trim().parse::<u16>().map_err(|_| format!("端口无效: {value}"))
}

/// 读 properties 文件某个 key 的值（按行，`key=value`）。
fn read_property(path: &Path, key: &str) -> Option<String> {
    let content = std::fs::read_to_string(path).ok()?;
    for line in content.lines() {
        let line = line.trim();
        if let Some((k, v)) = line.split_once('=') {
            if k.trim() == key {
                return Some(v.trim().to_string());
            }
        }
    }
    None
}

/// 写 properties 文件某个 key（保留注释与其他行）。
fn write_property(path: &Path, key: &str, value: &str) -> Result<(), String> {
    let content = std::fs::read_to_string(path).map_err(|e| format!("读取 {} 失败: {e}", path.display()))?;
    let mut out = String::new();
    let mut found = false;
    for line in content.lines() {
        let trimmed = line.trim();
        if let Some((k, _)) = trimmed.split_once('=') {
            if k.trim() == key {
                out.push_str(&format!("{key}={value}\n"));
                found = true;
                continue;
            }
        }
        out.push_str(line);
        out.push('\n');
    }
    if !found {
        out.push_str(&format!("{key}={value}\n"));
    }
    std::fs::write(path, out).map_err(|e| format!("写入 {} 失败: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_host_port_extracts_port() {
        assert_eq!(parse_host_port("localhost:9870"), "9870");
        assert_eq!(parse_host_port("0.0.0.0:9092"), "9092");
    }

    #[test]
    fn parse_broker_port_extracts_first_listener() {
        assert_eq!(parse_broker_port("PLAINTEXT://:9092,CONTROLLER://:9093"), "9092");
        assert_eq!(parse_broker_port("PLAINTEXT://:10000"), "10000");
    }

    #[test]
    fn replace_broker_port_replaces_first_only() {
        assert_eq!(
            replace_broker_port("PLAINTEXT://:9092,CONTROLLER://:9093", 10000),
            "PLAINTEXT://:10000,CONTROLLER://:9093"
        );
    }

    #[test]
    fn list_fields_hadoop_has_five() {
        use crate::test_util::HOME_LOCK;
        let _guard = HOME_LOCK.lock().unwrap();
        let tmp = std::env::temp_dir().join("solostack-schema-test");
        std::env::set_var("HOME", &tmp);
        let _ = std::fs::remove_dir_all(&tmp);

        let fields = list_fields("hadoop", "3.5.0").unwrap();
        assert_eq!(fields.len(), 5);
        assert_eq!(fields[0].id, "namenode_web_port");
        assert_eq!(fields[1].id, "yarn_rm_web_port");
        assert_eq!(fields[2].id, "history_enabled");
        assert_eq!(fields[3].id, "history_web_port");
        assert_eq!(fields[4].id, "jdk_version");
        assert_eq!(fields[2].value, "false");

        let _ = std::fs::remove_dir_all(&tmp);
    }
}
