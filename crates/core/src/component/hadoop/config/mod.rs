//! Hadoop 的配置布局、生效值与字段读写。
//!
//! | 文件 | 职责 |
//! |---|---|
//! | `mod.rs` | 常量（文件名 / 键 / 参数 / 端口）+ re-export + 集成测试 |
//! | `effective.rs` | 生效值 `Effective`：安装计算与配置读回 |
//! | `schema.rs` | `ConfigLifecycle` / `FieldSchema` 实现 |
//! | `generate.rs` | 生成落盘（`write_config`） |
//! | `read.rs` | 精确读取与路径解析 |

mod effective;
mod generate;
mod read;
mod schema;

pub use effective::Effective;
pub use read::{history_enabled, namenode_dir};

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
pub(super) const NAMENODE_RPC: u16 = 8020;
/// SecondaryNameNode WebUI（固定，不参与探活）。
const SECONDARY_HTTP: u16 = 50090;

#[cfg(test)]
mod tests {
    use super::super::{Hadoop, NAME};
    use super::generate::write_config;
    use super::read::read_port;
    use super::*;
    use crate::component::{
        self, ConfigFieldUpdate, ConfigLifecycle, FieldSchema, InstallParams,
    };

    const ENV_ID: &str = "00000000-0000-4000-8000-000000000001";

    fn save_field(id: &str, value: &str) -> Result<(), String> {
        let updates = [ConfigFieldUpdate {
            id: id.to_string(),
            value: value.to_string(),
        }];
        let plan = Hadoop.plan_field_updates(ENV_ID, "3.5.0", &updates)?;
        crate::config::apply_plan(&plan)
    }

    /// 在临时 HOME 下铺一份「解压出来的官方配置」，供读写测试。
    fn setup_instance(tmp: &std::path::Path) {
        std::env::set_var("HOME", tmp);
        let dir = component::config_io::config_dir(ENV_ID, NAME, "3.5.0").unwrap();
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
            .apply_install_config(ENV_ID, "3.5.0", &InstallParams::new())
            .unwrap();
        assert!(
            !history_enabled(ENV_ID, "3.5.0"),
            "空参数集不应开启 JobHistory"
        );
        let eff = Effective::from_config(ENV_ID, "3.5.0");
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
        write_config(ENV_ID, "3.5.0", &eff).unwrap();

        // 「写进去的」必须等于「读出来的」——这正是取消 .detect-ports 的前提
        assert_eq!(Hadoop.detect_ports(ENV_ID, "3.5.0"), eff.ports());
        assert_eq!(Effective::from_config(ENV_ID, "3.5.0"), eff);

        // 官方模板文件被原地修改，未被整体替换
        let hdfs = component::config_io::config_dir(ENV_ID, NAME, "3.5.0")
            .unwrap()
            .join(F_HDFS);
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
            ENV_ID,
            "3.5.0",
            &Effective::from_install(&install_params()).unwrap(),
        )
        .unwrap();
        assert!(history_enabled(ENV_ID, "3.5.0"));

        save_field("history_enabled", "false").unwrap();
        assert!(!history_enabled(ENV_ID, "3.5.0"), "关闭后应删掉受管键");
        assert_eq!(
            Hadoop.detect_ports(ENV_ID, "3.5.0").len(),
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
            ENV_ID,
            "3.5.0",
            &Effective::from_install(&install_params()).unwrap(),
        )
        .unwrap();
        save_field("history_enabled", "false").unwrap();
        assert!(!history_enabled(ENV_ID, "3.5.0"));

        // 模拟配置页保存：紧接着提交 history_web_port（字段顺序在开关之后）
        save_field("history_web_port", "19888").unwrap();
        assert!(
            !history_enabled(ENV_ID, "3.5.0"),
            "关闭状态下提交端口不应把键写回（否则开关会被静默改回开启）"
        );

        // 开启后可以正常改端口
        save_field("history_enabled", "true").unwrap();
        save_field("history_web_port", "19899").unwrap();
        assert_eq!(
            read_port(ENV_ID, "3.5.0", F_MAPRED, K_HISTORY_WEB),
            Some(19899)
        );

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
            ENV_ID,
            "3.5.0",
            &Effective::from_install(&install_params()).unwrap(),
        )
        .unwrap();
        let nn_web = read_port(ENV_ID, "3.5.0", F_HDFS, K_NN_HTTP).unwrap();

        let err = save_field("yarn_rm_web_port", &nn_web.to_string()).unwrap_err();
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
            ENV_ID,
            "3.5.0",
            &Effective::from_install(&install_params()).unwrap(),
        )
        .unwrap();
        save_field("namenode_web_port", "9871").unwrap();
        save_field("yarn_rm_web_port", "8089").unwrap();

        let values = Hadoop.field_values(ENV_ID, "3.5.0");
        let get = |id: &str| values.iter().find(|v| v.id == id).unwrap().value.clone();
        assert_eq!(get("namenode_web_port"), "9871");
        assert_eq!(get("yarn_rm_web_port"), "8089");
        assert_eq!(
            Hadoop.detect_ports(ENV_ID, "3.5.0")[0],
            9871,
            "改端口后探活应读新值"
        );

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn batch_save_does_not_write_when_any_field_is_invalid() {
        let _guard = crate::test_util::HOME_LOCK.lock().unwrap();
        let tmp = std::env::temp_dir().join("solostack-hadoop-batch-rollback");
        let _ = std::fs::remove_dir_all(&tmp);
        setup_instance(&tmp);
        write_config(
            ENV_ID,
            "3.5.0",
            &Effective::from_install(&install_params()).unwrap(),
        )
        .unwrap();

        let hdfs = component::config_io::config_path(ENV_ID, NAME, "3.5.0", F_HDFS).unwrap();
        let yarn = component::config_io::config_path(ENV_ID, NAME, "3.5.0", F_YARN).unwrap();
        let hdfs_before = std::fs::read_to_string(&hdfs).unwrap();
        let yarn_before = std::fs::read_to_string(&yarn).unwrap();
        let updates = [
            ConfigFieldUpdate {
                id: "namenode_web_port".into(),
                value: "9871".into(),
            },
            ConfigFieldUpdate {
                id: "yarn_rm_web_port".into(),
                value: "0".into(),
            },
        ];

        let err =
            crate::component::schema::save_fields(ENV_ID, "hadoop", "3.5.0", &updates).unwrap_err();
        assert!(err.contains("端口"), "{err}");
        assert_eq!(std::fs::read_to_string(&hdfs).unwrap(), hdfs_before);
        assert_eq!(std::fs::read_to_string(&yarn).unwrap(), yarn_before);

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn ensure_config_rejects_missing_layout_file() {
        let _guard = crate::test_util::HOME_LOCK.lock().unwrap();
        let tmp = std::env::temp_dir().join("solostack-hadoop-missing");
        let _ = std::fs::remove_dir_all(&tmp);
        setup_instance(&tmp);
        std::fs::remove_file(
            component::config_io::config_dir(ENV_ID, NAME, "3.5.0")
                .unwrap()
                .join(F_WORKERS),
        )
        .unwrap();

        let err = Hadoop.ensure_config(ENV_ID, "3.5.0");
        assert!(err.is_err(), "布局声明的文件缺失应报错而非静默继续");

        let _ = std::fs::remove_dir_all(&tmp);
    }
}
