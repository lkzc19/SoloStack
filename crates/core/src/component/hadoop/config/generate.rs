//! Hadoop 配置生成：把生效配置合并写进各官方配置文件。

use super::effective::{ensure_ports_distinct, Effective};
use super::read::{managed_datanode_dir, managed_namenode_dir, read_port};
use super::super::NAME;
use super::{
    F_CORE, F_ENV, F_HDFS, F_MAPRED, F_WORKERS, F_YARN, K_DN_DATA_DIR, K_DN_HTTP,
    K_FS_DEFAULT_FS, K_HISTORY_WEB, K_NM_WEB, K_NN_HTTP, K_NN_NAME_DIR, K_REPLICATION, K_RM_HOST,
    K_RM_WEB, K_SECONDARY_HTTP, NAMENODE_RPC, SECONDARY_HTTP,
};
use crate::app::paths;
use crate::component;
use crate::config::ConfigPlan;

/// 把生效配置合并写进各官方配置文件（只动受管键，注释与其它配置原样保留）。
pub(super) fn write_config(
    environment_id: &str,
    version: &str,
    eff: &Effective,
) -> Result<(), String> {
    ensure_ports_distinct(eff)?;
    let data_root =
        paths::var_data_instance_dir(environment_id, NAME, version).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&data_root).map_err(|e| e.to_string())?;
    let mut plan = ConfigPlan::new();

    // core-site.xml：RPC 端口固定 8020（WebUI 端口 9870 与之不同，勿混用）
    plan.set(
        component::config_io::config_path(environment_id, NAME, version, F_CORE)?,
        K_FS_DEFAULT_FS,
        format!("hdfs://localhost:{NAMENODE_RPC}"),
    )?;

    // 数据目录：写 SoloStack 受管路径（见 managed_namenode_dir 的说明）
    plan.set(
        component::config_io::config_path(environment_id, NAME, version, F_HDFS)?,
        K_NN_NAME_DIR,
        managed_namenode_dir(environment_id, version)?
            .display()
            .to_string(),
    )?;
    plan.set(
        component::config_io::config_path(environment_id, NAME, version, F_HDFS)?,
        K_DN_DATA_DIR,
        managed_datanode_dir(environment_id, version)?
            .display()
            .to_string(),
    )?;
    plan.set(
        component::config_io::config_path(environment_id, NAME, version, F_HDFS)?,
        K_REPLICATION,
        "1",
    )?;
    plan.set(
        component::config_io::config_path(environment_id, NAME, version, F_HDFS)?,
        K_NN_HTTP,
        format!("localhost:{}", eff.nn_web),
    )?;
    plan.set(
        component::config_io::config_path(environment_id, NAME, version, F_HDFS)?,
        K_DN_HTTP,
        format!("0.0.0.0:{}", eff.dn_http),
    )?;
    plan.set(
        component::config_io::config_path(environment_id, NAME, version, F_HDFS)?,
        K_SECONDARY_HTTP,
        format!("localhost:{SECONDARY_HTTP}"),
    )?;

    plan.set(
        component::config_io::config_path(environment_id, NAME, version, F_YARN)?,
        K_RM_HOST,
        "localhost",
    )?;
    plan.set(
        component::config_io::config_path(environment_id, NAME, version, F_YARN)?,
        K_RM_WEB,
        format!("localhost:{}", eff.rm_web),
    )?;
    plan.set(
        component::config_io::config_path(environment_id, NAME, version, F_YARN)?,
        K_NM_WEB,
        format!("0.0.0.0:{}", eff.nm_web),
    )?;

    // mapred-site.xml：只在真正受管时落笔（开启写键，关闭删键），不白贴受管说明
    match eff.history {
        Some(port) => plan.set(
            component::config_io::config_path(environment_id, NAME, version, F_MAPRED)?,
            K_HISTORY_WEB,
            format!("localhost:{port}"),
        )?,
        None if read_port(environment_id, version, F_MAPRED, K_HISTORY_WEB).is_some() => {
            plan.remove(
                component::config_io::config_path(environment_id, NAME, version, F_MAPRED)?,
                K_HISTORY_WEB,
            )?;
        }
        None => {}
    }

    // hadoop-env.sh：日志与 pid 目录（JAVA_HOME 由通用 JDK 流程写入）
    plan.set(
        component::config_io::config_path(environment_id, NAME, version, F_ENV)?,
        "HADOOP_LOG_DIR",
        paths::var_log_instance_dir(environment_id, NAME, version)
            .map_err(|e| e.to_string())?
            .display()
            .to_string(),
    )?;
    plan.set(
        component::config_io::config_path(environment_id, NAME, version, F_ENV)?,
        "HADOOP_PID_DIR",
        paths::var_run_instance_dir(environment_id, NAME, version)
            .map_err(|e| e.to_string())?
            .display()
            .to_string(),
    )?;

    // `workers` 是纯主机列表：只要非空即可，官方模板自带 `localhost`，不覆盖用户改动。
    let path = component::config_io::config_path(environment_id, NAME, version, F_WORKERS)?;
    let non_empty = std::fs::read_to_string(&path)
        .map(|s| !s.trim().is_empty())
        .unwrap_or(false);
    if !non_empty {
        plan.replace_text(path, "localhost\n")?;
    }

    crate::config::apply_plan(&plan)
}
