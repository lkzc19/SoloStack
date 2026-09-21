//! 日志读取：app 与组件共用的来源抽象、读取与实时跟随。
//!
//! 采用 Flink Web 看日志的做法：看文件尾部 + 跟随新增 + 就地搜索（搜索在前端
//! 对已加载内容做）。app 日志按天、组件日志按文件，差异全部收敛在 `source`，
//! 读取与跟随（`reader`）只此一份。
//!
//! | 文件 | 职责 |
//! |---|---|
//! | `model.rs` | 统一行模型 `LogLine` |
//! | `source.rs` | `LogSource`：定位、枚举、路径守卫、是否结构化 |
//! | `reader.rs` | `tail` + `LogFollower`（轮转/半行缓冲） |

mod model;
mod reader;
mod source;

pub use model::LogLine;
pub use reader::{tail, LogFollower, PollResult, MAX_TAIL_LINES};
pub use source::{list_component_files, validated_component_file, LogSource};
