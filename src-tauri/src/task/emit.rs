//! 事件派发：事件出口抽象（[`EventSink`]）+ 顺序队列线程 + 进度锁外的 emit。

use parking_lot::Mutex;
use serde::Serialize;
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, LazyLock};

use tauri::Emitter;

use super::model::TaskProgress;
use crate::events;

/// 任务事件出口。
///
/// 生产实现是 Tauri 的 `AppHandle`（见下方 blanket impl），事件经队列线程派发；
/// 测试可注入 no-op 或记录型实现 —— 这是任务执行体（`run_task` 等）不直接依赖
/// `tauri::AppHandle` 的原因，因而可在集成测试中被驱动（`tauri::test::mock_app()`
/// 返回的 `MockRuntime` 与生产 `Wry` 不是同一类型，无法互相替代）。
pub trait EventSink: Send + Sync + 'static {
    /// 派发一条事件（实现需自行保证不阻塞调用方：生产走队列线程）。
    fn send(&self, event: &'static str, payload: serde_json::Value);
}

impl<R: tauri::Runtime> EventSink for tauri::AppHandle<R> {
    fn send(&self, event: &'static str, payload: serde_json::Value) {
        let app = self.clone();
        // 只做 channel 投递（O(1)），实际 emit 在队列线程执行
        let _ = EMIT_TX.send(Box::new(move || {
            let _ = app.emit(event, &payload);
        }));
    }
}

/// 事件派发队列（独立线程顺序发送）：`app.emit` 是同步调用，若前端主线程
/// （webview JS）未及时消费 IPC 消息会阻塞调用方——任务线程卡在 emit 上
/// 表现为"任务卡死 + 取消失效"（取消的 try_lock 已释放但任务不前进）。
/// 统一走队列线程后，任务线程/进度锁永远不碰 IPC，emit 阻塞只占本线程。
///
/// 队列投递的是**闭包**（而非 `(AppHandle, …)` 元组）：这样队列本身与具体
/// runtime 解耦，`AppHandle<R>` 的 blanket impl 无需为每种 runtime 各存一份队列。
static EMIT_TX: LazyLock<Sender<Box<dyn FnOnce() + Send>>> = LazyLock::new(|| {
    let (tx, rx) = channel::<Box<dyn FnOnce() + Send>>();
    std::thread::Builder::new()
        .name("task-progress-emit".into())
        .spawn(move || {
            while let Ok(job) = rx.recv() {
                job();
            }
        })
        .expect("spawn task-progress-emit thread");
    tx
});

/// 顺序派发一条事件（不阻塞调用方）。payload 先序列化，发送端只做 channel 投递。
pub(crate) fn emit_event<E: Serialize + Clone + Send + 'static>(
    sink: &dyn EventSink,
    event: &'static str,
    payload: E,
) {
    let Ok(v) = serde_json::to_value(payload) else {
        return;
    };
    sink.send(event, v);
}

pub(crate) fn emit(sink: &dyn EventSink, p: &TaskProgress) {
    emit_event(sink, events::TASK_PROGRESS, p.clone());
}

pub(crate) fn update_and_emit(
    sink: &dyn EventSink,
    guard: &Arc<Mutex<TaskProgress>>,
    f: impl FnOnce(&mut TaskProgress),
) {
    // 锁内更新 + clone 快照，锁外 emit：emit（tauri IPC 分发）若因 webview
    // 未及时消费而阻塞，不会持有 progress 锁——否则取消（cancel 的 try_lock）
    // 和进度读取都会卡死，表现为"取消失效"。
    //
    // 优先 try_lock：正常短临界区立即成功，不阻塞调用方。仅当锁已被占用
    // （异常：持锁方长时间未释放）才退化为阻塞式 lock —— parking_lot 会在锁
    // 释放时立即唤醒，而此前的 `std::thread::sleep(5ms)` 自旋既占死 tokio
    // worker，又平白引入最多 5ms 延迟。临界区内无 IO/await，不会长等。
    let snapshot = match guard.try_lock() {
        Some(p) => p,
        None => {
            eprintln!("[emit] 进度锁被占用，改为阻塞等待（正常短临界区不应出现）");
            guard.lock()
        }
    };
    let snapshot_clone;
    {
        let mut p = snapshot;
        f(&mut p);
        snapshot_clone = p.clone();
    }
    // 锁外 emit（emit_event 序列化/投递不占 progress 锁）
    emit(sink, &snapshot_clone);
}
