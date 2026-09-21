//! Hadoop 组件实现（伪分布式）。
//!
//! 配置直接读写解压包里的官方 `etc/hadoop/` 文件：`core-site.xml` /
//! `hdfs-site.xml` / `yarn-site.xml` / `mapred-site.xml` 走 XML 键值，
//! `hadoop-env.sh` 走 shell 环境键值（JAVA_HOME 也落在这里）。
//!
//! | 文件 | 职责 |
//! |---|---|
//! | `mod.rs` | 组件类型声明 + `Component` 实现 |
//! | `config/` | 配置布局、生效值 `Effective`、生成与字段读写（`ConfigLifecycle` / `FieldSchema`） |
//! | `runtime.rs` | 启停序列、WebUI 入口、NameNode 格式化（`Runtime`） |

mod config;
mod runtime;

use super::Component;

pub struct Hadoop;

/// 组件名（等于 `package/manifest/<component>.json` 的文件名）。
pub(super) const NAME: &str = "hadoop";

impl Component for Hadoop {
    fn display_name(&self) -> &'static str {
        "Hadoop"
    }
}
