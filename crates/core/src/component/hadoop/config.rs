//! Hadoop 的配置布局、生效值与字段读写。

use super::{Hadoop, NAME};
use crate::app::paths;
use crate::component::fields::{param_bool, param_port};
use crate::component::ports::pick_free;
use crate::component::{
    self, ConfigFieldValue, ConfigLayout, ConfigLifecycle, FieldSchema, InstallParam, InstallParams,
};

// 配置文件名（官方模板自带，原地修改）
pub(super) const F_CORE: &str = "core-site.xml";
pub(super) const F_HDFS: &str = "hdfs-site.xml";
pub(super) const F_YARN: &str = "yarn-site.xml";
pub(super) const F_MAPRED: &str = "mapred-site.xml";
pub(super) const F_ENV: &str = "hadoop-env.sh";
pub(super) const F_WORKERS: &str = "workers";

// 配置键
const K_FS_DEFAULT_FS: &str = "fs.defaultFS";
const K_NN_NAME_DIR: &str = "dfs.namenode.name.dir";
const K_DN_DATA_DIR: &str = "dfs.datanode.data.dir";
const K_REPLICATION: &str = "dfs.replication";
const K_NN_HTTP: &str = "dfs.namenode.http-address";
const K_DN_HTTP: &str = "dfs.datanode.http.address";
const K_SECONDARY_HTTP: &str = "dfs.namenode.secondary.http-address";
const K_RM_HOST: &str = "yarn.resourcemanager.hostname";
const K_RM_WEB: &str = "yarn.resourcemanager.webapp.address";
const K_NM_WEB: &str = "yarn.nodemanager.webapp.address";
const K_HISTORY_WEB: &str = "mapreduce.jobhistory.webapp.address";

// 安装参数 id（组件自己声明的参数表；前端提交同名键）
const P_NN_WEB: &str = "namenode_web_port";
const P_RM_WEB: &str = "yarn_rm_web_port";
const P_HISTORY_ENABLED: &str = "history_enabled";
const P_HISTORY_PORT: &str = "history_web_port";

// 默认端口（配置文件读不到该项时的回退）
const NN_WEB_DEFAULT: u16 = 9870;
const DN_HTTP_DEFAULT: u16 = 9864;
const RM_WEB_DEFAULT: u16 = 8088;
const NM_WEB_DEFAULT: u16 = 8042;
const HISTORY_DEFAULT: u16 = 19888;
/// NameNode RPC 端口（`fs.defaultFS` 用，不参与探活）。
const NAMENODE_RPC: u16 = 8020;
/// SecondaryNameNode WebUI（固定，不参与探活）。
const SECONDARY_HTTP: u16 = 50090;

/// hadoop 的生效配置：安装时由安装选项算出，启动前由配置文件精确读回。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Effective {
    pub(super) nn_web: u16,
    pub(super) dn_http: u16,
    pub(super) rm_web: u16,
    pub(super) nm_web: u16,
    /// 历史服务器 WebUI 端口；None = 关闭。
    pub(super) history: Option<u16>,
}

impl Effective {
    /// 安装时：默认端口被占用则避让，用户显式指定的端口尊重。
    ///
    /// 参数缺省（前端未提交）时全部走默认值 —— 与 `install_params` 声明的一致。
    fn from_install(params: &InstallParams) -> Result<Self, String> {
        let nn_web = param_port(params, P_NN_WEB, NN_WEB_DEFAULT)?;
        let rm_web = param_port(params, P_RM_WEB, RM_WEB_DEFAULT)?;
        let history_enabled = param_bool(params, P_HISTORY_ENABLED, false)?;
        let history_web = param_port(params, P_HISTORY_PORT, HISTORY_DEFAULT)?;
        Ok(Effective {
            nn_web: pick_free(nn_web, NN_WEB_DEFAULT),
            dn_http: pick_free(DN_HTTP_DEFAULT, DN_HTTP_DEFAULT),
            rm_web: pick_free(rm_web, RM_WEB_DEFAULT),
            nm_web: pick_free(NM_WEB_DEFAULT, NM_WEB_DEFAULT),
            history: history_enabled.then(|| pick_free(history_web, HISTORY_DEFAULT)),
        })
    }

    /// 启动前 / 配置页：从配置文件精确读回，单项缺失用默认值兜底。
    ///
    /// 每个文件**只读一次**：这条路径在状态轮询里高频执行（启停等待期每秒一次），
    /// 逐键调用会退化成「每键一次读盘 + 全文件解析」。
    pub(super) fn from_config(version: &str) -> Self {
        let hdfs = read_all(version, F_HDFS);
        let yarn = read_all(version, F_YARN);
        let mapred = read_all(version, F_MAPRED);
        Effective {
            nn_web: hdfs
                .get(K_NN_HTTP)
                .and_then(|v| parse_host_port(v))
                .unwrap_or(NN_WEB_DEFAULT),
            dn_http: hdfs
                .get(K_DN_HTTP)
                .and_then(|v| parse_host_port(v))
                .unwrap_or(DN_HTTP_DEFAULT),
            rm_web: yarn
                .get(K_RM_WEB)
                .and_then(|v| parse_host_port(v))
                .unwrap_or(RM_WEB_DEFAULT),
            nm_web: yarn
                .get(K_NM_WEB)
                .and_then(|v| parse_host_port(v))
                .unwrap_or(NM_WEB_DEFAULT),
            history: mapred.get(K_HISTORY_WEB).and_then(|v| parse_host_port(v)),
        }
    }

    /// 参与探活的端口（历史服务器仅在其开启时参与）。
    pub(super) fn ports(&self) -> Vec<u16> {
        let mut ports = vec![self.nn_web, self.dn_http, self.rm_web, self.nm_web];
        if let Some(h) = self.history {
            ports.push(h);
        }
        ports
    }
}

impl ConfigLifecycle for Hadoop {
    fn component(&self) -> &'static str {
        NAME
    }

    fn config_layout(&self) -> ConfigLayout {
        ConfigLayout {
            dir: "etc/hadoop",
            files: &[F_CORE, F_HDFS, F_YARN, F_MAPRED, F_ENV, F_WORKERS],
        }
    }

    fn detect_ports(&self, version: &str) -> Vec<u16> {
        Effective::from_config(version).ports()
    }

    fn install_params(&self, _version: &str) -> Vec<InstallParam> {
        vec![
            InstallParam::new(P_NN_WEB, NN_WEB_DEFAULT),
            InstallParam::new(P_RM_WEB, RM_WEB_DEFAULT),
            InstallParam::new(P_HISTORY_ENABLED, false),
            InstallParam::new(P_HISTORY_PORT, HISTORY_DEFAULT),
        ]
    }

    fn apply_install_config(&self, version: &str, params: &InstallParams) -> Result<(), String> {
        write_config(version, &Effective::from_install(params)?)
    }

    fn ensure_config(&self, version: &str) -> Result<(), String> {
        component::validate_layout(NAME, version, &self.config_layout())?;
        // 幂等补齐：读回当前生效值再合并写回（缺失的键补上，已有值不动）
        write_config(version, &Effective::from_config(version))
    }

    fn java_env_file(&self) -> Option<&'static str> {
        Some(F_ENV)
    }
}

impl FieldSchema for Hadoop {
    fn field_values(&self, version: &str) -> Vec<ConfigFieldValue> {
        let eff = Effective::from_config(version);
        vec![
            ConfigFieldValue {
                id: "namenode_web_port".into(),
                value: eff.nn_web.to_string(),
            },
            ConfigFieldValue {
                id: "yarn_rm_web_port".into(),
                value: eff.rm_web.to_string(),
            },
            ConfigFieldValue {
                id: "history_enabled".into(),
                value: if eff.history.is_some() {
                    "true".into()
                } else {
                    "false".into()
                },
            },
            ConfigFieldValue {
                id: "history_web_port".into(),
                value: eff.history.unwrap_or(HISTORY_DEFAULT).to_string(),
            },
        ]
    }

    fn set_field(&self, version: &str, field_id: &str, value: &str) -> Result<(), String> {
        match field_id {
            "namenode_web_port" => {
                let port = component::fields::parse_port(value)?;
                let mut eff = Effective::from_config(version);
                eff.nn_web = port;
                ensure_ports_distinct(&eff)?;
                set_xml(version, F_HDFS, K_NN_HTTP, &format!("localhost:{port}"))
            }
            "yarn_rm_web_port" => {
                let port = component::fields::parse_port(value)?;
                let mut eff = Effective::from_config(version);
                eff.rm_web = port;
                ensure_ports_distinct(&eff)?;
                set_xml(version, F_YARN, K_RM_WEB, &format!("localhost:{port}"))
            }
            "history_enabled" => match value.trim() {
                "true" => {
                    if read_port(version, F_MAPRED, K_HISTORY_WEB).is_none() {
                        let port = pick_free(HISTORY_DEFAULT, HISTORY_DEFAULT);
                        let mut eff = Effective::from_config(version);
                        eff.history = Some(port);
                        ensure_ports_distinct(&eff)?;
                        set_xml(
                            version,
                            F_MAPRED,
                            K_HISTORY_WEB,
                            &format!("localhost:{port}"),
                        )
                    } else {
                        Ok(())
                    }
                }
                "false" => remove_xml_if_present(version, F_MAPRED, K_HISTORY_WEB),
                _ => Err("历史服务器开关取值无效".to_string()),
            },
            "history_web_port" => {
                // 关闭状态下的空操作：开关关掉后「键存在即开启」，若这里照写就会把
                // 刚删除的键写回去（配置页保存会提交全部字段，顺序在 history_enabled 之后）
                if read_port(version, F_MAPRED, K_HISTORY_WEB).is_none() {
                    return Ok(());
                }
                let port = component::fields::parse_port(value)?;
                let mut eff = Effective::from_config(version);
                eff.history = Some(port);
                ensure_ports_distinct(&eff)?;
                set_xml(
                    version,
                    F_MAPRED,
                    K_HISTORY_WEB,
                    &format!("localhost:{port}"),
                )
            }
            _ => Err(format!("未知配置字段: {field_id}")),
        }
    }
}

/// 校验生效端口互不重复（含用户手填、避让后偶然撞上）。
///
/// 同一组件内两个角色绑同一端口必然失败，且 Hadoop 的报错含糊，故提前拦住。
fn ensure_ports_distinct(eff: &Effective) -> Result<(), String> {
    let mut named = vec![
        ("HDFS WebUI", eff.nn_web),
        ("DataNode WebUI", eff.dn_http),
        ("YARN WebUI", eff.rm_web),
        ("NodeManager WebUI", eff.nm_web),
    ];
    if let Some(h) = eff.history {
        named.push(("JobHistory WebUI", h));
    }
    component::ports::ensure_distinct(&named)
}

// ── 配置生成(安装 / 启动前共用)────────────────────────

/// 把生效配置合并写进各官方配置文件（只动受管键，注释与其它配置原样保留）。
fn write_config(version: &str, eff: &Effective) -> Result<(), String> {
    ensure_ports_distinct(eff)?;
    let data_root = paths::var_data_instance_dir(NAME, version).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&data_root).map_err(|e| e.to_string())?;

    // core-site.xml：RPC 端口固定 8020（WebUI 端口 9870 与之不同，勿混用）
    set_xml(
        version,
        F_CORE,
        K_FS_DEFAULT_FS,
        &format!("hdfs://localhost:{NAMENODE_RPC}"),
    )?;

    // 数据目录：写 SoloStack 受管路径（见 managed_namenode_dir 的说明）
    set_xml(
        version,
        F_HDFS,
        K_NN_NAME_DIR,
        &managed_namenode_dir(version)?.display().to_string(),
    )?;
    set_xml(
        version,
        F_HDFS,
        K_DN_DATA_DIR,
        &managed_datanode_dir(version)?.display().to_string(),
    )?;
    set_xml(version, F_HDFS, K_REPLICATION, "1")?;
    set_xml(
        version,
        F_HDFS,
        K_NN_HTTP,
        &format!("localhost:{}", eff.nn_web),
    )?;
    set_xml(
        version,
        F_HDFS,
        K_DN_HTTP,
        &format!("0.0.0.0:{}", eff.dn_http),
    )?;
    set_xml(
        version,
        F_HDFS,
        K_SECONDARY_HTTP,
        &format!("localhost:{SECONDARY_HTTP}"),
    )?;

    set_xml(version, F_YARN, K_RM_HOST, "localhost")?;
    set_xml(
        version,
        F_YARN,
        K_RM_WEB,
        &format!("localhost:{}", eff.rm_web),
    )?;
    set_xml(
        version,
        F_YARN,
        K_NM_WEB,
        &format!("0.0.0.0:{}", eff.nm_web),
    )?;

    // mapred-site.xml：只在真正受管时落笔（开启写键，关闭删键），不白贴受管说明
    match eff.history {
        Some(port) => set_xml(
            version,
            F_MAPRED,
            K_HISTORY_WEB,
            &format!("localhost:{port}"),
        )?,
        None => remove_xml_if_present(version, F_MAPRED, K_HISTORY_WEB)?,
    }

    // hadoop-env.sh：日志与 pid 目录（JAVA_HOME 由通用 JDK 流程写入）
    let env = component::open_config(NAME, version, F_ENV)?;
    env.set(
        "HADOOP_LOG_DIR",
        &paths::var_log_instance_dir(NAME, version)
            .map_err(|e| e.to_string())?
            .display()
            .to_string(),
    )?;
    env.set(
        "HADOOP_PID_DIR",
        &paths::var_run_instance_dir(NAME, version)
            .map_err(|e| e.to_string())?
            .display()
            .to_string(),
    )?;

    ensure_workers(version)
}

/// `workers` 是纯主机列表（非键值格式，故不走 `config` 模块）：只要非空即可，
/// 官方模板自带 `localhost`，不覆盖用户的改动。
fn ensure_workers(version: &str) -> Result<(), String> {
    let path = component::config_path(NAME, version, F_WORKERS)?;
    let non_empty = std::fs::read_to_string(&path)
        .map(|s| !s.trim().is_empty())
        .unwrap_or(false);
    if non_empty {
        return Ok(());
    }
    crate::config::write_text(&path, "localhost\n")
}

// ── XML 键值读写（薄封装，格式细节归 config 模块）──────

fn set_xml(version: &str, file: &str, key: &str, value: &str) -> Result<(), String> {
    component::open_config(NAME, version, file)?.set(key, value)
}

/// 键存在才删（避免为「本来就没有键」的文件贴上受管说明）。
fn remove_xml_if_present(version: &str, file: &str, key: &str) -> Result<(), String> {
    let f = component::open_config(NAME, version, file)?;
    if f.get(key)?.is_some() {
        f.remove(key)?;
    }
    Ok(())
}

/// 精确读一个配置项的原始值（空值视为未设置）。
fn read_raw(version: &str, file: &str, key: &str) -> Option<String> {
    let f = component::open_config(NAME, version, file).ok()?;
    f.get_trimmed(key).ok()?
}

/// 一次读入某配置文件的全部键值（键 → 值）。
fn read_all(version: &str, file: &str) -> std::collections::HashMap<String, String> {
    component::open_config(NAME, version, file)
        .and_then(|f| f.read_entries())
        .map(|entries| {
            entries
                .into_iter()
                .map(|e| (e.key, e.value.trim().to_string()))
                .collect()
        })
        .unwrap_or_default()
}

/// 从 `host:port` 形式的配置项精确读出端口。
fn read_port(version: &str, file: &str, key: &str) -> Option<u16> {
    parse_host_port(&read_raw(version, file, key)?)
}

/// 解析 `localhost:9870` / `0.0.0.0:9864` 这类「主机:端口」里的端口。
fn parse_host_port(s: &str) -> Option<u16> {
    s.rsplit(':').next()?.trim().parse::<u16>().ok()
}

/// SoloStack 受管的 NameNode 元数据目录（**写进配置的值**）。
///
/// 数据目录键由 SoloStack 拥有：它们决定组件数据落在哪，而卸载（`keep_data`）、
/// `is_within_root` 等机制都建立在 `var/data/<组件>/<版本>/` 之下。因此我们
/// 始终写入受管路径，而不是把官方模板里的占位值（如 Kafka 的 `/tmp/...`）
/// 或历史值原样回声回去。
pub(super) fn managed_namenode_dir(version: &str) -> Result<std::path::PathBuf, String> {
    Ok(paths::var_data_instance_dir(NAME, version)
        .map_err(|e| e.to_string())?
        .join("name"))
}

/// SoloStack 受管的 DataNode 数据目录（**写进配置的值**）。
pub(super) fn managed_datanode_dir(version: &str) -> Result<std::path::PathBuf, String> {
    Ok(paths::var_data_instance_dir(NAME, version)
        .map_err(|e| e.to_string())?
        .join("data"))
}

/// NameNode 元数据目录**实际所在位置**：从配置精确读 `dfs.namenode.name.dir`。
///
/// 用于「是否已格式化」的判断，必须与 Hadoop 实际使用的位置一致：
/// 若按受管默认路径判断，手改过该键（或用 `file://` / 相对路径写法）时会错位，
/// 于是每次启动都跑一次 `namenode -format -force`（`-force` 会重格式化、抹掉
/// 命名空间数据）。相对路径按实例目录解析，与组件启动时的工作目录一致。
pub(super) fn namenode_dir(version: &str) -> Result<std::path::PathBuf, String> {
    Ok(configured_dir(
        version,
        K_NN_NAME_DIR,
        managed_namenode_dir(version)?,
    ))
}

/// 读路径型配置项并解析为绝对路径；未设置时用 `fallback`（受管默认路径）。
fn configured_dir(version: &str, key: &str, fallback: std::path::PathBuf) -> std::path::PathBuf {
    let Some(raw) = read_raw(version, F_HDFS, key) else {
        return fallback;
    };
    let base = paths::instance_dir(NAME, version).unwrap_or_else(|_| fallback.clone());
    crate::config::resolve_path(&raw, &base).unwrap_or(fallback)
}

/// 历史服务器是否开启：以 `mapred-site.xml` 里有没有该键为准（精确读）。
pub(super) fn history_enabled(version: &str) -> bool {
    read_port(version, F_MAPRED, K_HISTORY_WEB).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 在临时 HOME 下铺一份「解压出来的官方配置」，供读写测试。
    fn setup_instance(tmp: &std::path::Path) {
        std::env::set_var("HOME", tmp);
        let dir = component::config_dir(NAME, "3.5.0").unwrap();
        std::fs::create_dir_all(&dir).unwrap();
        for file in Hadoop.config_layout().files {
            let content = match *file {
                F_WORKERS => "localhost\n".to_string(),
                F_ENV => "# The java implementation to use.\n# export JAVA_HOME=\n".to_string(),
                _ => "<?xml version=\"1.0\"?>\n<configuration>\n</configuration>\n".to_string(),
            };
            std::fs::write(dir.join(file), content).unwrap();
        }
    }

    /// 安装参数：只给 history_enabled=true，其余走默认值（等价于前端只改了开关）。
    fn install_params() -> InstallParams {
        InstallParams::from([(P_HISTORY_ENABLED.to_string(), "true".to_string())])
    }

    /// 契约：`install_params` 声明的默认值必须与「空参数集安装」实际采用的一致
    /// （前端预填的就是声明值，两边漂移会让用户看到的值与实际生效值不同）。
    #[test]
    fn declared_install_params_match_applied_defaults() {
        let _guard = crate::test_util::HOME_LOCK.lock().unwrap();
        let tmp = std::env::temp_dir().join("solostack-hadoop-install-params");
        let _ = std::fs::remove_dir_all(&tmp);
        setup_instance(&tmp);

        let declared: std::collections::HashMap<String, String> = Hadoop
            .install_params("3.5.0")
            .into_iter()
            .map(|p| (p.id, p.default))
            .collect();
        assert_eq!(declared[P_NN_WEB], NN_WEB_DEFAULT.to_string());
        assert_eq!(declared[P_RM_WEB], RM_WEB_DEFAULT.to_string());
        assert_eq!(declared[P_HISTORY_ENABLED], "false");
        assert_eq!(declared[P_HISTORY_PORT], HISTORY_DEFAULT.to_string());

        // 前端什么都不填时：全部走默认值，历史服务器关闭
        Hadoop
            .apply_install_config("3.5.0", &InstallParams::new())
            .unwrap();
        assert!(!history_enabled("3.5.0"), "空参数集不应开启 JobHistory");
        let eff = Effective::from_config("3.5.0");
        assert!(eff.nn_web >= NN_WEB_DEFAULT, "端口可能因占用避让而前移");
        assert!(eff.rm_web >= RM_WEB_DEFAULT);

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn written_ports_are_read_back_exactly() {
        let _guard = crate::test_util::HOME_LOCK.lock().unwrap();
        let tmp = std::env::temp_dir().join("solostack-hadoop-roundtrip");
        let _ = std::fs::remove_dir_all(&tmp);
        setup_instance(&tmp);

        let eff = Effective::from_install(&install_params()).unwrap();
        write_config("3.5.0", &eff).unwrap();

        // 「写进去的」必须等于「读出来的」——这正是取消 .detect-ports 的前提
        assert_eq!(Hadoop.detect_ports("3.5.0"), eff.ports());
        assert_eq!(Effective::from_config("3.5.0"), eff);

        // 官方模板文件被原地修改，未被整体替换
        let hdfs = component::config_dir(NAME, "3.5.0").unwrap().join(F_HDFS);
        let content = std::fs::read_to_string(&hdfs).unwrap();
        assert!(
            content.starts_with("<?xml version=\"1.0\"?>"),
            "XML 声明应保留"
        );
        assert_eq!(content.matches("<configuration>").count(), 1);

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn disabling_history_removes_managed_key() {
        let _guard = crate::test_util::HOME_LOCK.lock().unwrap();
        let tmp = std::env::temp_dir().join("solostack-hadoop-history");
        let _ = std::fs::remove_dir_all(&tmp);
        setup_instance(&tmp);

        write_config(
            "3.5.0",
            &Effective::from_install(&install_params()).unwrap(),
        )
        .unwrap();
        assert!(history_enabled("3.5.0"));

        Hadoop
            .set_field("3.5.0", "history_enabled", "false")
            .unwrap();
        assert!(!history_enabled("3.5.0"), "关闭后应删掉受管键");
        assert_eq!(
            Hadoop.detect_ports("3.5.0").len(),
            4,
            "历史端口不再参与探活"
        );

        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// 回归：JobHistory 关闭后，配置页保存会把 history_web_port 也一并提交，
    /// 此时必须**空操作**，否则刚删掉的键会被写回、开关自动弹回开启。
    #[test]
    fn history_port_edit_is_noop_while_disabled() {
        let _guard = crate::test_util::HOME_LOCK.lock().unwrap();
        let tmp = std::env::temp_dir().join("solostack-hadoop-history-port");
        let _ = std::fs::remove_dir_all(&tmp);
        setup_instance(&tmp);

        write_config(
            "3.5.0",
            &Effective::from_install(&install_params()).unwrap(),
        )
        .unwrap();
        Hadoop
            .set_field("3.5.0", "history_enabled", "false")
            .unwrap();
        assert!(!history_enabled("3.5.0"));

        // 模拟配置页保存：紧接着提交 history_web_port（字段顺序在开关之后）
        Hadoop
            .set_field("3.5.0", "history_web_port", "19888")
            .unwrap();
        assert!(
            !history_enabled("3.5.0"),
            "关闭状态下提交端口不应把键写回（否则开关会被静默改回开启）"
        );

        // 开启后可以正常改端口
        Hadoop
            .set_field("3.5.0", "history_enabled", "true")
            .unwrap();
        Hadoop
            .set_field("3.5.0", "history_web_port", "19899")
            .unwrap();
        assert_eq!(read_port("3.5.0", F_MAPRED, K_HISTORY_WEB), Some(19899));

        let _ = std::fs::remove_dir_all(&tmp);
    }

    /// 同组件内两个角色不能绑同一个端口（否则第二个监听器绑不上，报错含糊）。
    #[test]
    fn duplicate_ports_are_rejected() {
        let _guard = crate::test_util::HOME_LOCK.lock().unwrap();
        let tmp = std::env::temp_dir().join("solostack-hadoop-dup-port");
        let _ = std::fs::remove_dir_all(&tmp);
        setup_instance(&tmp);

        write_config(
            "3.5.0",
            &Effective::from_install(&install_params()).unwrap(),
        )
        .unwrap();
        let nn_web = read_port("3.5.0", F_HDFS, K_NN_HTTP).unwrap();

        let err = Hadoop
            .set_field("3.5.0", "yarn_rm_web_port", &nn_web.to_string())
            .unwrap_err();
        assert!(err.contains("端口冲突"), "实际报错: {err}");

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn field_roundtrip_reads_config_not_sidecar() {
        let _guard = crate::test_util::HOME_LOCK.lock().unwrap();
        let tmp = std::env::temp_dir().join("solostack-hadoop-fields");
        let _ = std::fs::remove_dir_all(&tmp);
        setup_instance(&tmp);

        write_config(
            "3.5.0",
            &Effective::from_install(&install_params()).unwrap(),
        )
        .unwrap();
        Hadoop
            .set_field("3.5.0", "namenode_web_port", "9871")
            .unwrap();
        Hadoop
            .set_field("3.5.0", "yarn_rm_web_port", "8089")
            .unwrap();

        let values = Hadoop.field_values("3.5.0");
        let get = |id: &str| values.iter().find(|v| v.id == id).unwrap().value.clone();
        assert_eq!(get("namenode_web_port"), "9871");
        assert_eq!(get("yarn_rm_web_port"), "8089");
        assert_eq!(
            Hadoop.detect_ports("3.5.0")[0],
            9871,
            "改端口后探活应读新值"
        );

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn ensure_config_rejects_missing_layout_file() {
        let _guard = crate::test_util::HOME_LOCK.lock().unwrap();
        let tmp = std::env::temp_dir().join("solostack-hadoop-missing");
        let _ = std::fs::remove_dir_all(&tmp);
        setup_instance(&tmp);
        std::fs::remove_file(
            component::config_dir(NAME, "3.5.0")
                .unwrap()
                .join(F_WORKERS),
        )
        .unwrap();

        let err = Hadoop.ensure_config("3.5.0");
        assert!(err.is_err(), "布局声明的文件缺失应报错而非静默继续");

        let _ = std::fs::remove_dir_all(&tmp);
    }
}
