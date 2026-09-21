//! Hadoop 的 `ConfigLifecycle` / `FieldSchema` 实现：配置布局与字段读写。

use super::effective::{ensure_ports_distinct, Effective};
use super::generate::write_config;
use super::super::{Hadoop, NAME};
use super::{
    F_CORE, F_ENV, F_HDFS, F_MAPRED, F_WORKERS, F_YARN, HISTORY_DEFAULT, K_HISTORY_WEB, K_NN_HTTP,
    K_RM_WEB, P_HISTORY_ENABLED, P_HISTORY_PORT, P_NN_WEB, P_RM_WEB, RM_WEB_DEFAULT,
    NN_WEB_DEFAULT,
};
use crate::component::ports::pick_free;
use crate::component::{
    self, ConfigFieldUpdate, ConfigFieldValue, ConfigLayout, ConfigLifecycle, FieldSchema,
    InstallParam, InstallParams,
};
use crate::config::ConfigPlan;

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

    fn detect_ports(&self, environment_id: &str, version: &str) -> Vec<u16> {
        Effective::from_config(environment_id, version).ports()
    }

    fn install_params(&self, _version: &str) -> Vec<InstallParam> {
        vec![
            InstallParam::new(P_NN_WEB, NN_WEB_DEFAULT),
            InstallParam::new(P_RM_WEB, RM_WEB_DEFAULT),
            InstallParam::new(P_HISTORY_ENABLED, false),
            InstallParam::new(P_HISTORY_PORT, HISTORY_DEFAULT),
        ]
    }

    fn apply_install_config(
        &self,
        environment_id: &str,
        version: &str,
        params: &InstallParams,
    ) -> Result<(), String> {
        write_config(environment_id, version, &Effective::from_install(params)?)
    }

    fn ensure_config(&self, environment_id: &str, version: &str) -> Result<(), String> {
        component::config_io::validate_layout(environment_id, NAME, version, &self.config_layout())?;
        // 幂等补齐：读回当前生效值再合并写回（缺失的键补上，已有值不动）
        write_config(
            environment_id,
            version,
            &Effective::from_config(environment_id, version),
        )
    }

    fn java_env_file(&self) -> Option<&'static str> {
        Some(F_ENV)
    }
}

impl FieldSchema for Hadoop {
    fn field_values(&self, environment_id: &str, version: &str) -> Vec<ConfigFieldValue> {
        let eff = Effective::from_config(environment_id, version);
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

    fn plan_field_updates(
        &self,
        environment_id: &str,
        version: &str,
        updates: &[ConfigFieldUpdate],
    ) -> Result<ConfigPlan, String> {
        let current = Effective::from_config(environment_id, version);
        let mut next = current.clone();
        let mut set_namenode = false;
        let mut set_yarn = false;
        let mut history_enabled = None;
        let mut history_port = None;

        for update in updates {
            match update.id.as_str() {
                "namenode_web_port" => {
                    next.nn_web = component::fields::parse_port(&update.value)?;
                    set_namenode = true;
                }
                "yarn_rm_web_port" => {
                    next.rm_web = component::fields::parse_port(&update.value)?;
                    set_yarn = true;
                }
                "history_enabled" => {
                    history_enabled = match update.value.trim() {
                        "true" => Some(true),
                        "false" => Some(false),
                        _ => return Err("历史服务器开关取值无效".to_string()),
                    };
                }
                "history_web_port" => {
                    history_port = Some(component::fields::parse_port(&update.value)?);
                }
                _ => return Err(format!("未知配置字段: {}", update.id)),
            }
        }

        match history_enabled {
            Some(true) => {
                next.history = Some(
                    history_port
                        .or(next.history)
                        .unwrap_or_else(|| pick_free(HISTORY_DEFAULT, HISTORY_DEFAULT)),
                );
            }
            Some(false) => next.history = None,
            None if history_port.is_some() && next.history.is_some() => {
                next.history = history_port;
            }
            None => {}
        }
        ensure_ports_distinct(&next)?;

        let mut plan = ConfigPlan::new();
        if set_namenode {
            plan.set(
                component::config_io::config_path(environment_id, NAME, version, F_HDFS)?,
                K_NN_HTTP,
                format!("localhost:{}", next.nn_web),
            )?;
        }
        if set_yarn {
            plan.set(
                component::config_io::config_path(environment_id, NAME, version, F_YARN)?,
                K_RM_WEB,
                format!("localhost:{}", next.rm_web),
            )?;
        }

        let history_touched =
            history_enabled.is_some() || (history_port.is_some() && current.history.is_some());
        if history_touched {
            let path = component::config_io::config_path(environment_id, NAME, version, F_MAPRED)?;
            match next.history {
                Some(port) => {
                    plan.set(path, K_HISTORY_WEB, format!("localhost:{port}"))?;
                }
                None if current.history.is_some() => {
                    plan.remove(path, K_HISTORY_WEB)?;
                }
                None => {}
            }
        }
        Ok(plan)
    }
}
