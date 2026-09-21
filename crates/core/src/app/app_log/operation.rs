//! 操作作用域：把一次生命周期操作的 trace_id 与环境关联到其间的所有日志。

use std::cell::RefCell;
use std::time::Instant;

use super::record::{LogContext, LogLevel};
use super::store::log_with_context;
use crate::app::id;

thread_local! {
    static CURRENT_CONTEXT: RefCell<Vec<LogContext>> = const { RefCell::new(Vec::new()) };
}

/// 一次操作的日志作用域。
pub struct Operation {
    context: LogContext,
    started: Instant,
    finished: bool,
}

impl Operation {
    pub fn begin(environment_id: &str, begin_message: &str) -> Self {
        let trace_id = id::new_id();
        let context = LogContext {
            environment_id: Some(environment_id.to_string()),
            trace_id: Some(trace_id),
        };
        CURRENT_CONTEXT.with(|stack| stack.borrow_mut().push(context.clone()));
        let _ = log_with_context(LogLevel::Info, begin_message, Some(&context));
        Self {
            context,
            started: Instant::now(),
            finished: false,
        }
    }

    pub fn context(&self) -> &LogContext {
        &self.context
    }

    /// 结束操作：成功记 INFO，失败记 ERROR。
    ///
    /// 错误**不解析文案**判断类型 —— 被取消的操作走 [`Self::finish_cancelled`]
    /// （显式记为 WARN）。靠 `error.contains("取消")` 这类判断会让日志级别随文案
    /// 悄悄变化，故不采用。
    pub fn finish<T>(mut self, result: &Result<T, String>) {
        let duration_ms = self.started.elapsed().as_millis();
        match result {
            Ok(_) => {
                let _ = log_with_context(
                    LogLevel::Info,
                    &format!("操作完成（耗时 {duration_ms} ms）"),
                    Some(&self.context),
                );
            }
            Err(error) => {
                let _ = log_with_context(
                    LogLevel::Error,
                    &format!("{error}（耗时 {duration_ms} ms）"),
                    Some(&self.context),
                );
            }
        }
        self.finished = true;
    }

    /// 记录一次结构化取消，避免同时写取消事件和 operation.failed。
    pub fn finish_cancelled(mut self, message: &str) {
        let duration_ms = self.started.elapsed().as_millis();
        let _ = log_with_context(
            LogLevel::Warn,
            &format!("{message}（耗时 {duration_ms} ms）"),
            Some(&self.context),
        );
        self.finished = true;
    }
}

impl Drop for Operation {
    fn drop(&mut self) {
        if !self.finished {
            let _ = log_with_context(LogLevel::Warn, "操作未正常结束", Some(&self.context));
        }
        CURRENT_CONTEXT.with(|stack| {
            stack.borrow_mut().pop();
        });
    }
}

/// 当前线程最内层操作上下文。
pub fn current_context() -> Option<LogContext> {
    CURRENT_CONTEXT.with(|stack| stack.borrow().last().cloned())
}
