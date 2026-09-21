//! Kafka 的 `ConfigLifecycle` / `FieldSchema` 实现：配置布局与字段读写。

use super::effective::{ensure_broker_port_ok, mb_to_bytes, Effective};
use super::generate::{advertised_listeners, listeners, write_config};
use super::read::{read_broker, read_prop, replace_broker_port};
use super::super::{Kafka, NAME};
use super::{
    BROKER_DEFAULT, F_ENV, F_PROPS, K_ADVERTISED, K_AUTO_CREATE, K_LISTENERS, K_MESSAGE_MAX_BYTES,
    K_NUM_PARTITIONS, K_RETENTION_HOURS, P_AUTO_CREATE, P_BROKER_PORT, P_MESSAGE_MAX_MB,
    P_NUM_PARTITIONS, P_RETENTION_HOURS,
};
use crate::component::{
    self, ConfigFieldUpdate, ConfigFieldValue, ConfigLayout, ConfigLifecycle, FieldSchema,
    InstallParam, InstallParams,
};
use crate::config::ConfigPlan;

impl ConfigLifecycle for Kafka {
    fn component(&self) -> &'static str {
        NAME
    }

    fn config_layout(&self) -> ConfigLayout {
        ConfigLayout {
            // Kafka 4.x 起 KRaft 是唯一模式，官方已把配置铺平在 config/ 下
            dir: "config",
            files: &[F_PROPS],
        }
    }

    fn detect_ports(&self, environment_id: &str, version: &str) -> Vec<u16> {
        vec![read_broker(environment_id, version).unwrap_or(BROKER_DEFAULT)]
    }

    fn install_params(&self, _version: &str) -> Vec<InstallParam> {
        vec![
            InstallParam::new(P_BROKER_PORT, BROKER_DEFAULT),
            InstallParam::new(P_NUM_PARTITIONS, 1),
            InstallParam::new(P_RETENTION_HOURS, 168),
            InstallParam::new(P_MESSAGE_MAX_MB, 1),
            InstallParam::new(P_AUTO_CREATE, true),
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

impl FieldSchema for Kafka {
    fn field_values(&self, environment_id: &str, version: &str) -> Vec<ConfigFieldValue> {
        let eff = Effective::from_config(environment_id, version);
        vec![
            ConfigFieldValue {
                id: "broker_port".into(),
                value: eff.broker.to_string(),
            },
            ConfigFieldValue {
                id: "num_partitions".into(),
                value: eff.num_partitions.to_string(),
            },
            ConfigFieldValue {
                id: "retention_hours".into(),
                value: eff.retention_hours.to_string(),
            },
            ConfigFieldValue {
                id: "message_max_mb".into(),
                value: eff.message_max_mb.to_string(),
            },
            ConfigFieldValue {
                id: "auto_create_topics".into(),
                value: if eff.auto_create_topics {
                    "true".into()
                } else {
                    "false".into()
                },
            },
        ]
    }

    fn plan_field_updates(
        &self,
        environment_id: &str,
        version: &str,
        updates: &[ConfigFieldUpdate],
    ) -> Result<ConfigPlan, String> {
        let mut next = Effective::from_config(environment_id, version);
        let mut broker_port = None;
        let mut num_partitions = None;
        let mut retention_hours = None;
        let mut message_max_mb = None;
        let mut auto_create_topics = None;

        for update in updates {
            match update.id.as_str() {
                "broker_port" => {
                    let port = component::fields::parse_port(&update.value)?;
                    ensure_broker_port_ok(port)?;
                    broker_port = Some(port);
                    next.broker = port;
                }
                "num_partitions" => {
                    let value = component::fields::parse_positive_int(&update.value, "默认分区数")?;
                    if value > 100_000 {
                        return Err("默认分区数过大".to_string());
                    }
                    num_partitions = Some(value);
                    next.num_partitions = value;
                }
                "retention_hours" => {
                    let value =
                        component::fields::parse_positive_int(&update.value, "消息保留时长")?;
                    retention_hours = Some(value);
                    next.retention_hours = value;
                }
                "message_max_mb" => {
                    let value =
                        component::fields::parse_positive_int(&update.value, "单条消息上限")?;
                    message_max_mb = Some(value);
                    next.message_max_mb = value;
                }
                "auto_create_topics" => {
                    let value = match update.value.trim() {
                        "true" => true,
                        "false" => false,
                        _ => return Err("自动创建 Topic 取值无效".to_string()),
                    };
                    auto_create_topics = Some(value);
                    next.auto_create_topics = value;
                }
                _ => return Err(format!("未知配置字段: {}", update.id)),
            }
        }
        ensure_broker_port_ok(next.broker)?;

        let path = component::config_io::config_path(environment_id, NAME, version, F_PROPS)?;
        let mut plan = ConfigPlan::new();
        if let Some(port) = broker_port {
            let current_listeners = read_prop(
                environment_id,
                version,
                K_LISTENERS,
                &listeners(BROKER_DEFAULT),
            );
            let current_advertised = read_prop(
                environment_id,
                version,
                K_ADVERTISED,
                &advertised_listeners(BROKER_DEFAULT),
            );
            plan.set(
                path.clone(),
                K_LISTENERS,
                replace_broker_port(&current_listeners, port),
            )?;
            plan.set(
                path.clone(),
                K_ADVERTISED,
                replace_broker_port(&current_advertised, port),
            )?;
        }
        if let Some(value) = num_partitions {
            plan.set(path.clone(), K_NUM_PARTITIONS, value.to_string())?;
        }
        if let Some(value) = retention_hours {
            plan.set(path.clone(), K_RETENTION_HOURS, value.to_string())?;
        }
        if let Some(value) = message_max_mb {
            plan.set(
                path.clone(),
                K_MESSAGE_MAX_BYTES,
                mb_to_bytes(value).to_string(),
            )?;
        }
        if let Some(value) = auto_create_topics {
            plan.set(path, K_AUTO_CREATE, value.to_string())?;
        }
        Ok(plan)
    }
}
