//! 事件派发：顺序队列线程 + 进度锁外的 emit。

use parking_lot::Mutex;
use serde::Serialize;
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, LazyLock};

use tauri::{AppHandle, Emitter};

use super::model::TaskProgress;
use crate::events;

/// 事件派发队列（独立线程顺序发送）：`app.emit` 是同步调用，若前端主线程
/// （webview JS）未及时消费 IPC 消息会阻塞调用方——任务线程卡在 emit 上
/// 表现为"任务卡死 + 取消失效"（取消的 try_lock 已释放但任务不前进）。
/// 统一走队列线程后，任务线程/进度锁永远不碰 IPC，emit 阻塞只占本线程。
static EMIT_TX: LazyLock<Sender<(AppHandle, &'static str, serde_json::Value)>> =
    LazyLock::new(|| {
        let (tx, rx) = channel::<(AppHandle, &'static str, serde_json::Value)>();
        std::thread::Builder::new()
            .name("task-progress-emit".into())
            .spawn(move || {
                while let Ok((app, event, payload)) = rx.recv() {
                    let _ = app.emit(event, &payload);
                }
            })
            .expect("spawn task-progress-emit thread");
        tx
    });

/// 顺序派发一条事件（队列线程发送，不阻塞调用方）。payload 先序列化，
/// 发送端只做 channel 投递（O(1)），webview 消费慢只积压在队列线程侧。
pub(crate) fn emit_event<E: Serialize + Clone + Send + 'static>(
    app: &AppHandle,
    event: &'static str,
    payload: E,
) {
    let Ok(v) = serde_json::to_value(payload) else {
        return;
    };
    let _ = EMIT_TX.send((app.clone(), event, v));
}

pub(crate) fn emit(app: &AppHandle, p: &TaskProgress) {
    emit_event(app, events::TASK_PROGRESS, p.clone());
}

pub(crate) fn update_and_emit(
    app: &AppHandle,
    guard: &Arc<Mutex<TaskProgress>>,
    f: impl FnOnce(&mut TaskProgress),
) {
    // 锁内更新 + clone 快照，锁外 emit：emit（tauri IPC 分发）若因 webview
    // 未及时消费而阻塞，不会持有 progress 锁——否则取消（cancel 的 try_lock）
    // 和进度读取都会卡死，表现为"取消失效"。
    // try_lock 循环：正常短临界区立即成功；异常（锁被长期持有/死锁）打印
    // 告警后继续等待——不 panic 不放弃，且日志能定位持锁方。
    let mut waited_ms = 0u32;
    let snapshot = loop {
        if let Some(p) = guard.try_lock() {
            break p;
        }
        waited_ms += 5;
        if waited_ms == 5 || waited_ms.is_multiple_of(500) {
            eprintln!(
                "[emit] 进度锁被持有 {}ms（等待中，正常短临界区不应出现）",
                waited_ms
            );
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    };
    let snapshot_clone;
    {
        let mut p = snapshot;
        f(&mut p);
        snapshot_clone = p.clone();
    }
    // 锁外 emit（emit_event 序列化/投递不占 progress 锁）
    emit(app, &snapshot_clone);
}
