//! 事件派发：事件出口抽象（[`EventSink`]）+ 派发线程 + 进度锁外的 emit。
//!
//! # 为什么需要派发线程
//!
//! `app.emit` 是同步调用：webview（JS 主线程）未及时消费 IPC 时会阻塞调用方。
//! 任务线程若卡在 emit 上，表现为「任务卡死 + 取消失效」（取消的 `try_lock` 已
//! 释放但任务不前进）。统一走独立派发线程后，任务线程 / 进度锁永远不碰 IPC。
//!
//! # 为什么进度帧要按任务合并（[`Dispatcher`]）
//!
//! 派发队列不能无界：webview 卡住时（例如正渲染上千个版本卡片）生产者仍按心跳
//! 频率投递——任务 150ms、工具箱 250ms 一帧，而 `TaskProgress` 是**完整快照**
//! （整合包安装的 `steps` 每个内容文件一步，单帧可达数十 KB）。不合并的话内存按
//! 「卡顿时长 × 频率 × 帧大小」线性上涨（整合包场景量级可达百 KB/s）。
//!
//! 合并规则：**同一 (事件, 任务) 只保留最新一帧**。
//! - 语义安全：payload 是完整快照而非增量，被合并掉的中间帧只是少刷新几次进度条；
//!   终态（done/error/cancelled）就是该任务最后一次写入，**不会被丢弃**。
//! - 键必须含**事件名**：`task-waiting-user` 与 `task-progress` 属于同一任务，
//!   若共用槽位，后到的进度帧会把「等你决策」的弹窗帧顶掉 → 任务永远等人来点。
//! - 只合并高频快照类（`task-progress` / `tooldl-progress`）；其余低频事件
//!   （登录完成 / 联机状态 / 游戏退出 / 配置恢复 / 等待用户决策）不可丢且量小，
//!   走 FIFO 直投。
//!
//! 两条通道之间无全局顺序保证（语义相互独立）；同一 (事件, 任务) 内部严格保序，
//! 后帧绝不会先于前帧送达（否则前端卡片状态会回退）。

use parking_lot::{Condvar, Mutex};
use serde::Serialize;
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, LazyLock};

use tauri::Emitter;

use super::model::TaskProgress;
use crate::events;

/// 任务事件出口。
///
/// 生产实现是 Tauri 的 `AppHandle`（见下方 blanket impl），事件经派发线程投递；
/// 测试可注入 no-op 或记录型实现 —— 这是任务执行体（`run_task` 等）不直接依赖
/// `tauri::AppHandle` 的原因，因而可在集成测试中被驱动（`tauri::test::mock_app()`
/// 返回的 `MockRuntime` 与生产 `Wry` 不是同一类型，无法互相替代）。
pub trait EventSink: Send + Sync + 'static {
    /// 派发一条事件（实现需自行保证不阻塞调用方：生产走队列线程）。
    fn send(&self, event: &'static str, payload: serde_json::Value);
}

impl<R: tauri::Runtime> EventSink for tauri::AppHandle<R> {
    fn send(&self, event: &'static str, payload: serde_json::Value) {
        // 先判定并入键，再把 payload 移进 job
        let key = coalesce_key(event, &payload);
        let app = self.clone();
        // 只做 mutex/channel 投递（O(1)），实际 emit 由派发线程执行
        let job: Job = Box::new(move || {
            let _ = app.emit(event, &payload);
        });
        match key {
            Some(key) => DISPATCH.enqueue_progress(key, job),
            None => DISPATCH.enqueue_immediate(job),
        }
    }
}

/// 待执行的一次投递。
type Job = Box<dyn FnOnce() + Send>;

/// 全局派发器（进程内唯一）：懒启动派发线程。
static DISPATCH: LazyLock<Arc<Dispatcher>> = LazyLock::new(|| {
    let dispatcher = Arc::new(Dispatcher::default());
    let worker = Arc::clone(&dispatcher);
    std::thread::Builder::new()
        .name("task-progress-emit".into())
        .spawn(move || worker.run())
        .expect("spawn task-progress-emit thread");
    dispatcher
});

/// 事件派发器：高频快照帧按 (事件, 任务) 合并，低频事件 FIFO 直投。
///
/// 状态由一个互斥量 + 条件变量保护：生产者只做 `insert`/`push_back` + `notify_one`
/// （O(1)，永不阻塞），派发线程取出后**在锁外**执行（`app.emit` 可能长时间阻塞）。
#[derive(Default)]
struct Dispatcher {
    inner: Mutex<Pending>,
    wake: Condvar,
}

/// 派发器内部状态。
#[derive(Default)]
struct Pending {
    /// `(事件名, 任务 id)` → 待发的最新一帧（同键新帧覆盖旧帧）。
    progress: HashMap<String, Job>,
    /// 低频、不可丢、需保持顺序的即时事件。
    immediate: VecDeque<Job>,
}

impl Dispatcher {
    /// 投递一帧进度快照：同键已排队的旧帧被替换（丢弃的是**中间帧**，不是终态）。
    fn enqueue_progress(&self, key: String, job: Job) {
        let mut pending = self.inner.lock();
        pending.progress.insert(key, job);
        self.wake.notify_one();
    }

    /// 投递一条即时事件（保持 FIFO）。
    fn enqueue_immediate(&self, job: Job) {
        let mut pending = self.inner.lock();
        pending.immediate.push_back(job);
        self.wake.notify_one();
    }

    /// 派发线程主体：等待 → 取出 → 锁外执行。
    fn run(&self) {
        loop {
            let (immediate, progress) = self.wait_and_take();
            for job in immediate {
                job();
            }
            for job in progress {
                job();
            }
        }
    }

    /// 阻塞至有活可干，然后取走全部待发（锁内只做搬运，执行在锁外）。
    fn wait_and_take(&self) -> (Vec<Job>, Vec<Job>) {
        let mut pending = self.inner.lock();
        while pending.progress.is_empty() && pending.immediate.is_empty() {
            self.wake.wait(&mut pending);
        }
        Self::take(&mut pending)
    }

    fn take(pending: &mut Pending) -> (Vec<Job>, Vec<Job>) {
        (
            pending.immediate.drain(..).collect(),
            pending.progress.drain().map(|(_, job)| job).collect(),
        )
    }

    /// 取走当前待发（不执行）。仅供单测确定性驱动。
    #[cfg(test)]
    fn take_pending(&self) -> (Vec<Job>, Vec<Job>) {
        Self::take(&mut self.inner.lock())
    }
}

/// 合并键：`事件名\u{1}任务 id`（`\u{1}` 不会出现在事件名或 id 中）。
fn frame_key(event: &str, id: &str) -> String {
    format!("{event}\u{1}{id}")
}

/// 该事件是否可合并，可合并时返回其键。
///
/// 只认高频快照类事件，且必须能从 payload 取到任务 id —— 取不到就返回 `None`
/// （宁可多投一帧，也不能把不同东西塞进同一个槽位）。
fn coalesce_key(event: &str, payload: &serde_json::Value) -> Option<String> {
    if event == events::TASK_PROGRESS || event == events::TOOL_DL_PROGRESS {
        // 任务进度用 `task_id`，工具箱下载用 `id`（各自完整快照的标识字段）。
        let id = payload
            .get("task_id")
            .or_else(|| payload.get("id"))?
            .as_str()?;
        return Some(frame_key(event, id));
    }
    if event == events::AUTO_TASK_PROGRESS {
        // 自动任务进度按「实例 + 阶段」合并：同实例三个阶段互不顶替，
        // 且新阶段开始时旧阶段的残留帧不会复活。
        let id = payload.get("version_id")?.as_str()?;
        let phase = payload.get("phase")?.as_str()?;
        return Some(frame_key(event, &format!("{id}\u{2}{phase}")));
    }
    None
}

/// 派发一条事件（不阻塞调用方）。payload 先序列化，发送端只做投递。
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
    // 锁内更新 + clone 快照，锁外 emit：emit 若因 webview 未及时消费而阻塞，
    // 不会持有 progress 锁——否则取消（cancel 的 try_lock）和进度读取都会卡死，
    // 表现为"取消失效"。
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::Arc;

    type Log = Arc<Mutex<Vec<&'static str>>>;

    fn rec(log: &Log, tag: &'static str) -> Job {
        let log = Arc::clone(log);
        Box::new(move || log.lock().push(tag))
    }

    fn key(event: &str, id: &str) -> String {
        frame_key(event, id)
    }

    /// 只有高频快照类事件可合并，且必须带标识字段。
    #[test]
    fn coalesce_key_covers_only_high_rate_snapshots() {
        assert_eq!(
            coalesce_key(
                events::TASK_PROGRESS,
                &json!({"task_id": "t1", "status": "running"})
            )
            .as_deref(),
            Some(key(events::TASK_PROGRESS, "t1").as_str())
        );
        assert_eq!(
            coalesce_key(events::TOOL_DL_PROGRESS, &json!({"id": "d1"})).as_deref(),
            Some(key(events::TOOL_DL_PROGRESS, "d1").as_str())
        );

        // 等待用户决策与进度帧同属一个任务，但**不可共槽**：否则弹窗帧被顶掉，
        // 任务会一直等人点、前端却从没弹过窗。
        assert!(coalesce_key(events::TASK_WAITING_USER, &json!({"task_id": "t1"})).is_none());
        // 其余低频事件直投。
        assert!(coalesce_key(events::ONLINE_STATE, &json!({"status": "idle"})).is_none());
        assert!(coalesce_key(events::MS_LOGIN_COMPLETE, &json!({"ok": true})).is_none());
        // 快照类但取不到 id → 不合并（宁可多投，不能顶错槽）。
        assert!(coalesce_key(events::TASK_PROGRESS, &json!({"status": "running"})).is_none());

        // 自动任务进度按（实例 + 阶段）合并：同实例不同阶段必须分槽，
        // 否则 pre_launch 的残留帧会顶掉 post_exit 的进度。
        assert_eq!(
            coalesce_key(
                events::AUTO_TASK_PROGRESS,
                &json!({"version_id": "v1", "phase": "pre_launch"})
            )
            .as_deref(),
            Some(key(events::AUTO_TASK_PROGRESS, "v1\u{2}pre_launch").as_str())
        );
        assert_ne!(
            coalesce_key(
                events::AUTO_TASK_PROGRESS,
                &json!({"version_id": "v1", "phase": "pre_launch"})
            ),
            coalesce_key(
                events::AUTO_TASK_PROGRESS,
                &json!({"version_id": "v1", "phase": "post_exit"})
            ),
            "同实例的不同阶段必须分槽"
        );
        assert_ne!(
            coalesce_key(
                events::AUTO_TASK_PROGRESS,
                &json!({"version_id": "v1", "phase": "pre_launch"})
            ),
            coalesce_key(
                events::AUTO_TASK_PROGRESS,
                &json!({"version_id": "v2", "phase": "pre_launch"})
            ),
            "不同实例必须分槽"
        );
        assert!(coalesce_key(events::AUTO_TASK_PROGRESS, &json!({"action": "record_mods"})).is_none());
        // 阶段结束汇总是低频终态，直投不合并。
        assert!(coalesce_key(events::AUTO_TASKS_FINISHED, &json!({"version_id": "v1"})).is_none());
    }

    /// 同一任务的连帧只留最新一帧（内存有界的根据）。
    #[test]
    fn same_task_keeps_only_latest_frame() {
        let dispatcher = Dispatcher::default();
        let log: Log = Arc::new(Mutex::new(Vec::new()));
        for tag in ["v1", "v2", "v3"] {
            dispatcher.enqueue_progress(key(events::TASK_PROGRESS, "t1"), rec(&log, tag));
        }

        let (immediate, progress) = dispatcher.take_pending();
        assert!(immediate.is_empty());
        assert_eq!(progress.len(), 1, "同一任务只应保留最新一帧");
        for job in progress {
            job();
        }
        assert_eq!(&*log.lock(), &["v3"], "保留的必须是最新帧");
    }

    /// 不同任务 / 不同类型互不顶替。
    #[test]
    fn distinct_tasks_and_event_kinds_are_kept_apart() {
        let dispatcher = Dispatcher::default();
        let log: Log = Arc::new(Mutex::new(Vec::new()));
        dispatcher.enqueue_progress(key(events::TASK_PROGRESS, "t1"), rec(&log, "task-t1"));
        dispatcher.enqueue_progress(key(events::TASK_PROGRESS, "t2"), rec(&log, "task-t2"));
        dispatcher.enqueue_progress(key(events::TOOL_DL_PROGRESS, "t1"), rec(&log, "dl-t1"));
        dispatcher.enqueue_immediate(rec(&log, "waiting-user"));

        let (immediate, progress) = dispatcher.take_pending();
        assert_eq!(immediate.len(), 1);
        assert_eq!(progress.len(), 3, "三个不同槽位的帧都应保留");
        for job in immediate.into_iter().chain(progress) {
            job();
        }
        let mut got = log.lock().clone();
        got.sort_unstable();
        assert_eq!(got, ["dl-t1", "task-t1", "task-t2", "waiting-user"]);
    }

    /// 即时事件保持 FIFO。
    #[test]
    fn immediate_events_keep_fifo_order() {
        let dispatcher = Dispatcher::default();
        let log: Log = Arc::new(Mutex::new(Vec::new()));
        for tag in ["a", "b", "c"] {
            dispatcher.enqueue_immediate(rec(&log, tag));
        }

        let (immediate, progress) = dispatcher.take_pending();
        assert!(progress.is_empty());
        for job in immediate {
            job();
        }
        assert_eq!(&*log.lock(), &["a", "b", "c"]);
    }

    /// 已取出的帧与随后生产的新帧之间**不得乱序**（否则前端卡片状态会回退）。
    #[test]
    fn frames_of_one_task_stay_monotonic() {
        let dispatcher = Dispatcher::default();
        let log: Log = Arc::new(Mutex::new(Vec::new()));
        dispatcher.enqueue_progress(key(events::TASK_PROGRESS, "t1"), rec(&log, "old"));
        let (_, first) = dispatcher.take_pending();
        dispatcher.enqueue_progress(key(events::TASK_PROGRESS, "t1"), rec(&log, "new"));
        let (_, second) = dispatcher.take_pending();

        for job in first.into_iter().chain(second) {
            job();
        }
        assert_eq!(&*log.lock(), &["old", "new"]);
    }
}
