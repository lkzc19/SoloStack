use std::path::PathBuf;

use crate::paths;

/// 一个 XML 配置属性。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct ConfigProperty {
    pub name: String,
    pub value: String,
}

/// 某组件的配置副本源：组件解压目录内相对实例的配置源目录 + 需复制到副本的文件清单。
/// （组件固有差异，按 name 分支，取代旧的 template.config）
pub struct ConfigSource {
    pub source_dir: &'static str,
    pub config_files: &'static [&'static str],
}

/// 配置副本源（代码分支，取代 template.config）。
fn config_source(name: &str) -> ConfigSource {
    match name {
        "hadoop" => ConfigSource {
            source_dir: "etc/hadoop",
            config_files: &["core-site.xml", "hdfs-site.xml", "yarn-site.xml", "workers"],
        },
        "kafka" => ConfigSource {
            source_dir: "config/kraft",
            config_files: &["server.properties"],
        },
        _ => ConfigSource { source_dir: "", config_files: &[] },
    }
}

/// 运行时必需配置文件（Hadoop 配置目录完整性，见 ensure_runtime_configs 注释）。
const RUNTIME_CONFIG_FILES: &[&str] = &["log4j.properties", "hadoop-env.sh", "capacity-scheduler.xml"];

/// 初始化组件的配置副本，并生成组件配置（hadoop 伪分布式 / kafka KRaft）。
///
/// 流程：复制源配置到副本 →（若为 hadoop）注入伪分布式属性。返回配置副本目录。
pub fn prepare_config(name: &str, version: &str) -> Result<PathBuf, String> {
    let dest = init_config(name, version)?;
    match name {
        "hadoop" => {
            // 用配置副本已确定的探活端口重写，保证与探活一致
            let ports = read_detect_ports(name, version);
            crate::configgen::write_hadoop_pseudo_config(name, version, &ports)?;
            // 日志 / pid 目录写进 hadoop-env.sh
            write_hadoop_env_log_dir(name, version)?;
            write_hadoop_env_pid_dir(name, version)?;
        }
        "kafka" => crate::configgen::write_kafka_config(name, version)?,
        _ => {}
    }
    Ok(dest)
}

/// 计算组件的探活端口（写进配置副本 `.detect-ports`）。
///
/// hadoop：NameNode WebUI / DataNode / YARN RM WebUI / NodeManager，
/// 历史服务器开启时（`history_web > 0`）追加 JobHistory WebUI 端口。
/// 用户显式给定的 WebUI 端口尊重；默认端口被占用则 +1/+2 避让。
/// kafka：broker 端口（9092）。
pub fn compute_detect_ports(name: &str, namenode_web: u16, yarn_rm: u16, history_web: u16) -> Vec<u16> {
    if name != "hadoop" {
        // kafka 等：默认单端口 9092
        return vec![9092];
    }
    let nn_web = pick_free(if namenode_web > 0 { namenode_web } else { 9870 }, 9870);
    let dn = pick_free(9864, 9864);
    let rm = pick_free(if yarn_rm > 0 { yarn_rm } else { 8088 }, 8088);
    let nm = pick_free(8042, 8042);
    let mut ports = vec![nn_web, dn, rm, nm];
    if history_web > 0 {
        ports.push(pick_free(history_web, 19888));
    }
    ports
}

/// 端口避让：自定义值（≠默认）尊重；默认值被占用则递增到空闲。
fn pick_free(preferred: u16, default: u16) -> u16 {
    if preferred != default {
        return preferred;
    }
    let mut p = preferred;
    while crate::process::port_open(p) && p < u16::MAX {
        p += 1;
    }
    p
}

/// 写探活端口到配置副本 `.detect-ports`（每行一个端口）。
pub fn write_detect_ports(name: &str, version: &str, ports: &[u16]) -> Result<(), String> {
    let etc = paths::etc_instance_dir(name, version).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&etc).map_err(|e| e.to_string())?;
    let content = ports.iter().map(|p| p.to_string()).collect::<Vec<_>>().join("\n");
    std::fs::write(etc.join(".detect-ports"), content).map_err(|e| format!("写入探活端口失败: {e}"))
}

/// 读配置副本 `.detect-ports` 探活端口；文件缺失时回退组件默认。
pub fn read_detect_ports(name: &str, version: &str) -> Vec<u16> {
    let etc = match paths::etc_instance_dir(name, version) {
        Ok(d) => d,
        Err(_) => return default_ports(name),
    };
    match std::fs::read_to_string(etc.join(".detect-ports")) {
        Ok(content) => content.lines().filter_map(|l| l.trim().parse::<u16>().ok()).collect(),
        Err(_) => default_ports(name),
    }
}

/// 组件默认端口（代码分支）。
fn default_ports(name: &str) -> Vec<u16> {
    match name {
        "hadoop" => vec![9870, 9864, 8088, 8042],
        "kafka" => vec![9092],
        _ => vec![],
    }
}

/// 历史服务器是否开启（hadoop 探活端口数组长度 > 4，第 5 个是 JobHistory WebUI 端口）。
pub fn history_enabled(name: &str, version: &str) -> bool {
    read_detect_ports(name, version).len() > 4
}

/// 写 JAVA_HOME 到配置副本 `.java-home`（通用，供启动注入）。
pub fn write_java_home(name: &str, version: &str, java_home: &str) -> Result<(), String> {
    let etc = paths::etc_instance_dir(name, version).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&etc).map_err(|e| e.to_string())?;
    std::fs::write(etc.join(".java-home"), java_home).map_err(|e| format!("写入 JAVA_HOME 失败: {e}"))
}

/// 读配置副本 `.java-home`；不存在返回 None。
pub fn read_java_home(name: &str, version: &str) -> Option<String> {
    let etc = paths::etc_instance_dir(name, version).ok()?;
    let content = std::fs::read_to_string(etc.join(".java-home")).ok()?;
    let s = content.trim();
    if s.is_empty() { None } else { Some(s.to_string()) }
}

/// 在配置副本 `hadoop-env.sh` 追加 JAVA_HOME（Hadoop 原生读取，启动即用指定 JDK）。
pub fn write_hadoop_env_java_home(name: &str, version: &str, java_home: &str) -> Result<(), String> {
    let env_file = hadoop_env_file(name, version)?;
    set_env_export(&env_file, "JAVA_HOME", java_home)
}

/// 在配置副本 `hadoop-env.sh` 追加 HADOOP_LOG_DIR（日志落到 `var/log/<组件>-<版本>/`）。
pub fn write_hadoop_env_log_dir(name: &str, version: &str) -> Result<(), String> {
    let env_file = hadoop_env_file(name, version)?;
    let log_dir = paths::var_log_instance_dir(name, version).map_err(|e| e.to_string())?;
    set_env_export(&env_file, "HADOOP_LOG_DIR", &log_dir.display().to_string())
}

/// 在配置副本 `hadoop-env.sh` 追加 HADOOP_PID_DIR（pid 落到 `var/run/<组件>-<版本>/`）。
pub fn write_hadoop_env_pid_dir(name: &str, version: &str) -> Result<(), String> {
    let env_file = hadoop_env_file(name, version)?;
    let pid_dir = paths::var_run_instance_dir(name, version).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&pid_dir).map_err(|e| format!("创建 pid 目录失败: {e}"))?;
    set_env_export(&env_file, "HADOOP_PID_DIR", &pid_dir.display().to_string())
}

/// 配置副本 `hadoop-env.sh` 路径（确保父目录存在）。
fn hadoop_env_file(name: &str, version: &str) -> Result<PathBuf, String> {
    let etc = paths::etc_instance_dir(name, version).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&etc).map_err(|e| e.to_string())?;
    Ok(etc.join("hadoop-env.sh"))
}

/// 在 shell 配置里写/替换 `export KEY=value`（移除旧行避免重复）。
fn set_env_export(env_file: &std::path::Path, key: &str, value: &str) -> Result<(), String> {
    let existing = std::fs::read_to_string(env_file).unwrap_or_default();
    let prefix = format!("export {key}=");
    let filtered: Vec<&str> = existing
        .lines()
        .filter(|l| !l.trim().starts_with(&prefix))
        .collect();
    let mut content = filtered.join("\n");
    if !content.is_empty() && !content.ends_with('\n') {
        content.push('\n');
    }
    content.push_str(&format!("export {key}={value}\n"));
    std::fs::write(env_file, content).map_err(|e| format!("写入 {env_file:?} 失败: {e}"))
}

/// 初始化组件的配置副本：把组件实例内 `config_source` 的配置复制到 `~/.solostack/etc/<组件>-<版本>/`。
///
/// 复制的是配置清单指定的文件（而非整个目录），保持副本精简。
/// 若副本已存在则不覆盖（保留用户已修改的配置）。
/// 副本创建后始终补齐运行时必需配置（`log4j.properties` 等），保证 HADOOP_CONF_DIR 完整。
fn init_config(name: &str, version: &str) -> Result<PathBuf, String> {
    let instance = paths::instance_dir(name, version).map_err(|e| e.to_string())?;
    let dest = paths::etc_instance_dir(name, version).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dest).map_err(|e| format!("创建配置目录失败: {e}"))?;

    let source = config_source(name);
    let source_dir = instance.join(source.source_dir);
    for file in source.config_files {
        let src = source_dir.join(file);
        let dst = dest.join(file);
        if !dst.exists() {
            if !src.is_file() {
                return Err(format!("配置源文件不存在: {}", src.display()));
            }
            std::fs::copy(&src, &dst).map_err(|e| format!("复制 {} 失败: {e}", src.display()))?;
        }
    }

    // 补齐运行时必需配置（幂等：仅当副本缺失且源存在时补复制，不覆盖用户已改内容）。
    ensure_runtime_configs(name, version, &dest, &source_dir)?;

    Ok(dest)
}

/// 补齐运行时必需配置文件到配置副本（只补缺失，不覆盖）。
fn ensure_runtime_configs(
    name: &str,
    version: &str,
    dest: &std::path::Path,
    source_dir: &std::path::Path,
) -> Result<(), String> {
    let instance = paths::instance_dir(name, version).map_err(|e| e.to_string())?;
    let dir = if name == "hadoop" {
        instance.join("etc/hadoop")
    } else {
        source_dir.to_path_buf()
    };
    for file in RUNTIME_CONFIG_FILES {
        let src = dir.join(file);
        let dst = dest.join(file);
        if src.is_file() && !dst.exists() {
            let _ = std::fs::copy(&src, &dst);
        }
    }
    Ok(())
}

/// 配置文件在配置副本中的完整路径。
pub fn config_file_path(name: &str, version: &str, file: &str) -> Result<PathBuf, String> {
    Ok(paths::etc_instance_dir(name, version)
        .map_err(|e| e.to_string())?
        .join(file))
}

/// 读取配置副本中某个 XML 配置文件的所有属性（`name`/`value`）。
pub fn read_config_props(name: &str, version: &str, file: &str) -> Result<Vec<ConfigProperty>, String> {
    let path = config_file_path(name, version, file)?;
    let content = std::fs::read_to_string(&path)
        .map_err(|e| format!("读取 {} 失败: {e}", path.display()))?;
    parse_xml_properties(&content)
}

/// 覆写配置副本中某个 XML 文件的所有属性（保留文件其余内容）。
pub fn write_config_props(
    name: &str,
    version: &str,
    file: &str,
    props: &[ConfigProperty],
) -> Result<(), String> {
    let path = config_file_path(name, version, file)?;
    let content = std::fs::read_to_string(&path)
        .map_err(|e| format!("读取 {} 失败: {e}", path.display()))?;
    let rendered = render_xml_properties(&content, props)?;
    std::fs::write(&path, rendered).map_err(|e| format!("写入 {} 失败: {e}", path.display()))?;
    let _ = crate::app_log::append(crate::app_log::INFO, &format!("保存配置 {name} / {file}"));
    Ok(())
}

/// 解析 XML 中的 `<property><name>..</name><value>..</value></property>` 列表。
fn parse_xml_properties(xml: &str) -> Result<Vec<ConfigProperty>, String> {
    let mut props = Vec::new();
    let mut rest = xml;
    while let Some(prop_start) = rest.find("<property>") {
        rest = &rest[prop_start + "<property>".len()..];
        let Some(prop_end) = rest.find("</property>") else {
            break;
        };
        let block = &rest[..prop_end];
        rest = &rest[prop_end..];

        let Some(name) = extract_tag(block, "name") else {
            continue;
        };
        let value = extract_tag(block, "value").unwrap_or_default();
        props.push(ConfigProperty { name, value });
    }
    Ok(props)
}

/// 提取 `<tag>content</tag>` 的 content，标签未出现返回 None。
fn extract_tag(block: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let s = block.find(&open)? + open.len();
    let e = block[s..].find(&close)? + s;
    Some(block[s..e].trim().to_string())
}

/// 渲染 XML：把 `<configuration>` 节点内的属性替换为给定列表，保留其余内容。
fn render_xml_properties(original: &str, props: &[ConfigProperty]) -> Result<String, String> {
    let open = "<configuration>";
    let close = "</configuration>";
    let start = original
        .find(open)
        .ok_or("配置文件中未找到 <configuration>")?;
    let end = original
        .rfind(close)
        .ok_or("配置文件中未找到 </configuration>")?;

    let head = &original[..start];
    let tail = &original[end..];

    let mut props_xml = String::from("<configuration>\n");
    for p in props {
        props_xml.push_str(&format!(
            "  <property>\n    <name>{}</name>\n    <value>{}</value>\n  </property>\n",
            p.name, p.value
        ));
    }
    props_xml.push_str("</configuration>");

    Ok(format!("{head}{props_xml}{tail}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_xml_properties_extracts_all() {
        let xml = r#"<?xml version="1.0"?>
<configuration>
  <property>
    <name>fs.defaultFS</name>
    <value>hdfs://localhost:9870</value>
  </property>
  <property>
    <name>dfs.replication</name>
    <value>1</value>
  </property>
</configuration>"#;
        let props = parse_xml_properties(xml).unwrap();
        assert_eq!(props.len(), 2);
        assert_eq!(props[0].name, "fs.defaultFS");
        assert_eq!(props[0].value, "hdfs://localhost:9870");
        assert_eq!(props[1].name, "dfs.replication");
        assert_eq!(props[1].value, "1");
    }

    #[test]
    fn render_xml_properties_preserves_license() {
        let original = r#"<?xml version="1.0"?>
<!-- Licensed under the Apache License -->
<configuration>
  <property>
    <name>old</name>
    <value>1</value>
  </property>
</configuration>"#;
        let props = vec![ConfigProperty { name: "new".into(), value: "2".into() }];
        let out = render_xml_properties(original, &props).unwrap();
        assert!(out.starts_with("<?xml version=\"1.0\"?>\n<!-- Licensed"));
        assert!(out.contains("<name>new</name>"));
        assert!(!out.contains("<name>old</name>"));
    }

    #[test]
    fn compute_detect_ports_hadoop_default() {
        let ports = compute_detect_ports("hadoop", 0, 0, 0);
        assert_eq!(ports.len(), 4);
    }

    #[test]
    fn compute_detect_ports_history_appends() {
        let ports = compute_detect_ports("hadoop", 0, 0, 19888);
        assert_eq!(ports.len(), 5);
    }

    #[test]
    fn compute_detect_ports_non_hadoop_default() {
        let ports = compute_detect_ports("kafka", 0, 0, 0);
        assert_eq!(ports, vec![9092]);
    }

    #[test]
    fn set_env_export_replaces() {
        use crate::test_util::HOME_LOCK;
        let _guard = HOME_LOCK.lock().unwrap();
        let tmp = std::env::temp_dir().join("solostack-env-test");
        std::fs::create_dir_all(&tmp).unwrap();
        let f = tmp.join("hadoop-env.sh");
        std::fs::write(&f, "export JAVA_HOME=/old\n").unwrap();
        set_env_export(&f, "JAVA_HOME", "/new").unwrap();
        let c = std::fs::read_to_string(&f).unwrap();
        assert!(!c.contains("/old"));
        assert!(c.contains("export JAVA_HOME=/new"));
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
