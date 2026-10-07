//! 任务执行上下文：任务辅助函数共用的一组句柄。

use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use parking_lot::Mutex;

use super::emit::EventSink;
use super::model::TaskProgress;

/// 任务执行上下文（事件出口 + 取消令牌 + 进度锁 + 任务 id）。
/// 集中传递，避免每个辅助函数重复展开这四个参数。
#[derive(Clone, Copy)]
pub(crate) struct TaskCtx<'a> {
    pub app: &'a dyn EventSink,
    pub cancel: &'a Arc<AtomicBool>,
    pub progress: &'a Arc<Mutex<TaskProgress>>,
    pub task_id: &'a str,
}
