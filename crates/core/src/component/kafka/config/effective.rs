//! Kafka 生效值：安装时由安装参数算出，启动前 / 配置页由配置文件精确读回。

use super::read::{broker_port, read_all};
use super::super::CONTROLLER_PORT;
use super::{
    BROKER_DEFAULT, K_AUTO_CREATE, K_LISTENERS, K_MESSAGE_MAX_BYTES, K_NUM_PARTITIONS,
    K_RETENTION_HOURS, MESSAGE_MAX_DEFAULT, P_AUTO_CREATE, P_BROKER_PORT, P_MESSAGE_MAX_MB,
    P_NUM_PARTITIONS, P_RETENTION_HOURS,
};
use crate::component::fields::{param_bool, param_port, param_positive_int};
use crate::component::ports;
use crate::component::InstallParams;

/// kafka 的生效配置。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Effective {
    pub(super) broker: u16,
    pub(super) num_partitions: u64,
    pub(super) retention_hours: u64,
    pub(super) message_max_mb: u64,
    pub(super) auto_create_topics: bool,
}

impl Effective {
    /// 安装时：参数缺省（前端未提交）时全部走默认值，与 `install_params` 声明一致。
    pub(super) fn from_install(params: &InstallParams) -> Result<Self, String> {
        // 避让只作用于「默认端口被占/撞保留端口」；用户显式指定的端口原样采纳，
        // 是否合法由 ensure_broker_port_ok 明确拦下（不静默改用户填的值）。
        let broker = ports::pick_free_excluding(
            param_port(params, P_BROKER_PORT, BROKER_DEFAULT)?,
            BROKER_DEFAULT,
            &[CONTROLLER_PORT],
        );
        ensure_broker_port_ok(broker)?;
        Ok(Effective {
            broker,
            num_partitions: param_positive_int(params, P_NUM_PARTITIONS, 1, "默认分区数")?,
            retention_hours: param_positive_int(params, P_RETENTION_HOURS, 168, "消息保留时长")?,
            message_max_mb: param_positive_int(params, P_MESSAGE_MAX_MB, 1, "单条消息上限")?,
            auto_create_topics: param_bool(params, P_AUTO_CREATE, true)?,
        })
    }

    /// 启动前 / 配置页：从配置文件精确读回，单项缺失用默认值兜底。
    ///
    /// server.properties **只读一次**：该路径在状态轮询里高频执行，逐键读盘代价高。
    pub(super) fn from_config(environment_id: &str, version: &str) -> Self {
        let props = read_all(environment_id, version);
        let num = |key: &str, default: u64| {
            props
                .get(key)
                .and_then(|v| v.trim().parse::<u64>().ok())
                .unwrap_or(default)
        };
        Effective {
            broker: props
                .get(K_LISTENERS)
                .and_then(|v| broker_port(v))
                .and_then(|p| p.parse::<u16>().ok())
                .unwrap_or(BROKER_DEFAULT),
            num_partitions: num(K_NUM_PARTITIONS, 1),
            retention_hours: num(K_RETENTION_HOURS, 168),
            message_max_mb: mb_from_bytes(num(K_MESSAGE_MAX_BYTES, MESSAGE_MAX_DEFAULT)),
            auto_create_topics: props.get(K_AUTO_CREATE).map(|v| v.trim()) != Some("false"),
        }
    }
}

/// 校验 broker 端口不与内置 controller 端口相同。
///
/// 组合模式（broker+controller）会把两个监听器写进同一个 `listeners`，
/// 撞端口时 Kafka 起不来且报错含糊，故在这里指名道姓地拦下。
pub(super) fn ensure_broker_port_ok(broker: u16) -> Result<(), String> {
    ports::ensure_distinct(&[
        ("Broker 端口", broker),
        ("内置 controller 端口", CONTROLLER_PORT),
    ])
}

/// MB 档位 → 字节（1 MB 用 Kafka 官方默认值，含 12 字节消息头开销）。
pub(super) fn mb_to_bytes(mb: u64) -> u64 {
    if mb <= 1 {
        MESSAGE_MAX_DEFAULT
    } else {
        mb * 1_048_576
    }
}

/// 字节 → MB 档位（向上取整，1 MB 以下归 1）。
pub(super) fn mb_from_bytes(bytes: u64) -> u64 {
    let mb = (bytes + 524_288) / 1_048_576;
    mb.max(1)
}
