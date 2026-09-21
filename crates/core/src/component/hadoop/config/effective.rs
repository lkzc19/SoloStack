//! Hadoop 生效值：安装时由安装参数算出，启动前 / 配置页由配置文件精确读回。

use super::read::{parse_host_port, read_all};
use super::{
    DN_HTTP_DEFAULT, HISTORY_DEFAULT, K_DN_HTTP, K_HISTORY_WEB, K_NM_WEB, K_NN_HTTP, K_RM_WEB,
    NN_WEB_DEFAULT, NM_WEB_DEFAULT, P_HISTORY_ENABLED, P_HISTORY_PORT, P_NN_WEB, P_RM_WEB,
    RM_WEB_DEFAULT, F_HDFS, F_MAPRED, F_YARN,
};
use crate::component::fields::{param_bool, param_port};
use crate::component::ports::pick_free;
use crate::component::InstallParams;

/// hadoop 的生效配置。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Effective {
    pub nn_web: u16,
    pub dn_http: u16,
    pub rm_web: u16,
    pub nm_web: u16,
    /// 历史服务器 WebUI 端口；None = 关闭。
    pub history: Option<u16>,
}

impl Effective {
    /// 安装时：默认端口被占用则避让，用户显式指定的端口尊重。
    ///
    /// 参数缺省（前端未提交）时全部走默认值 —— 与 `install_params` 声明的一致。
    pub fn from_install(params: &InstallParams) -> Result<Self, String> {
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
    pub fn from_config(environment_id: &str, version: &str) -> Self {
        let hdfs = read_all(environment_id, version, F_HDFS);
        let yarn = read_all(environment_id, version, F_YARN);
        let mapred = read_all(environment_id, version, F_MAPRED);
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
    pub fn ports(&self) -> Vec<u16> {
        let mut ports = vec![self.nn_web, self.dn_http, self.rm_web, self.nm_web];
        if let Some(h) = self.history {
            ports.push(h);
        }
        ports
    }
}

/// 校验生效端口互不重复（含用户手填、避让后偶然撞上）。
///
/// 同一组件内两个角色绑同一端口必然失败，且 Hadoop 的报错含糊，故提前拦住。
pub(super) fn ensure_ports_distinct(eff: &Effective) -> Result<(), String> {
    let mut named = vec![
        ("HDFS WebUI", eff.nn_web),
        ("DataNode WebUI", eff.dn_http),
        ("YARN WebUI", eff.rm_web),
        ("NodeManager WebUI", eff.nm_web),
    ];
    if let Some(h) = eff.history {
        named.push(("JobHistory WebUI", h));
    }
    crate::component::ports::ensure_distinct(&named)
}
