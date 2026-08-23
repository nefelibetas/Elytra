// ── task 模块门面：TaskManager 单例 + 事件推送 + 类型重导出 ──
// 职责分区：model.rs（serde 任务配置）/ steps.rs（步骤分析）/ progress.rs（进度上报）/
// repair.rs（缺失扫描与修复）/ files.rs（批量文件下载）/ runner.rs（完整下载主体）/
// jdk.rs（JDK 安装任务）。

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use parking_lot::Mutex;
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, LazyLock, OnceLock};

use serde::Serialize;
use tauri::{AppHandle, Emitter};

pub mod files;
pub mod jdk;
pub mod modpack;
pub mod model;
pub mod progress;
pub mod repair;
pub mod runner;
pub mod steps;
pub mod store;

pub use model::{
    DownloadFileItem, DownloadFileKind, DownloadFileTask, DownloadFilesTask, DownloadTask,
    FabricApi, LoaderChoice, LoaderKind, ModpackInstallSpec, StepInfo,
    StepStatus, TaskProgress, TaskStatus,
};
pub use steps::{StepKind, StepSpec, TaskSpec};

static TM: OnceLock<Arc<TaskManager>> = OnceLock::new();

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

pub fn init() {
    TM.set(Arc::new(TaskManager::default())).ok();
    // 崩溃恢复（#6）：上次进程退出时残留 running 快照 → interrupted，
    // 前端启动后经 list_interrupted_tasks 拉取并询问恢复。
    store::mark_crashed_as_interrupted();
}

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

pub fn global() -> &'static Arc<TaskManager> {
    TM.get().expect("TaskManager not initialized")
}

pub struct TaskManager {
    inner: Mutex<Inner>,
}

/// 挂起任务的用户决策（WaitingForUser：批量下载单文件失败时暂停，用户选跳过/重试/取消）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WaitingDecision {
    /// 跳过该文件，继续剩余文件
    Skip,
    /// 重试该文件
    Retry,
    /// 取消整个任务
    Cancel,
}

impl Default for TaskManager {
    fn default() -> Self {
        Self {
            inner: Mutex::new(Inner {
                tasks: HashMap::new(),
                active_paths: HashMap::new(),
                waiting: HashMap::new(),
                counter: 0,
            }),
        }
    }
}

struct Inner {
    tasks: HashMap<String, TaskHandle>,
    /// 被下载任务占用的路径（版本目录 / 目标文件），同路径并发互斥
    active_paths: HashMap<std::path::PathBuf, ()>,
    /// 挂起等待用户决策的任务（task_id → 决策槽；任务侧轮询，命令侧写入）
    waiting: HashMap<String, Arc<Mutex<Option<WaitingDecision>>>>,
    counter: u64,
}

struct TaskHandle {
    cancel: Arc<AtomicBool>,
    progress: Arc<Mutex<TaskProgress>>,
    cleanup_path: Option<std::path::PathBuf>,
}

impl TaskManager {
    /// Register a new task, return (task_id, cancel_token, progress_guard).
    pub fn create(
        &self,
        name: &str,
        steps: &[&str],
    ) -> (String, Arc<AtomicBool>, Arc<Mutex<TaskProgress>>) {
        let mut inner = self.inner.lock();
        let id = format!("t{}", inner.counter);
        inner.counter += 1;
        let cancel = Arc::new(AtomicBool::new(false));
        let progress = Arc::new(Mutex::new(TaskProgress {
            task_id: id.clone(),
            name: name.to_string(),
            status: TaskStatus::Pending,
            progress: 0.0,
            speed_bps: 0.0,
            eta_seconds: None,
            completed_files: 0,
            total_files: 0,
            completed_bytes: 0,
            total_bytes: 0,
            current_phase: "等待中".into(),
            steps: steps
                .iter()
                .map(|s| StepInfo {
                    name: s.to_string(),
                    status: StepStatus::Pending,
                    progress: 0.0,
                })
                .collect(),
            error: None,
        }));
        inner.tasks.insert(
            id.clone(),
            TaskHandle {
                cancel: cancel.clone(),
                progress: progress.clone(),
                cleanup_path: None,
            },
        );
        (id, cancel, progress)
    }

    /// 独占占用一个路径（版本目录/目标文件）；同路径已被占用返回 false。
    pub(crate) fn acquire(&self, path: &std::path::Path) -> bool {
        let mut inner = self.inner.lock();
        if inner.active_paths.contains_key(path) {
            return false;
        }
        inner.active_paths.insert(path.to_path_buf(), ());
        true
    }

    pub(crate) fn release(&self, path: &std::path::Path) {
        self.inner.lock().active_paths.remove(path);
    }

    pub fn set_cleanup_path(&self, id: &str, path: std::path::PathBuf) {
        if let Some(h) = self.inner.lock().tasks.get_mut(id) {
            h.cleanup_path = Some(path);
        }
    }

    pub fn cancel(&self, id: &str) -> bool {
        let cancelled = {
            let inner = self.inner.lock();
            if let Some(h) = inner.tasks.get(id) {
                h.cancel.store(true, Ordering::SeqCst);
                // 目录清理移交任务侧 set_cancelled 收口：此处同步 remove_dir_all
                // 会阻塞 worker 线程（大版本目录删除数十秒）致前端"取消卡住"，
                // 且删除与仍在跑的下载竞争写文件。
                // progress 锁内只改内存状态；DB 写移到锁外——DB（sqlite）若因
                // 文件锁/连接池卡住，不会持有 progress/inner 锁阻塞任务与取消。
                if let Some(mut p) = h.progress.try_lock() {
                    p.status = TaskStatus::Cancelled;
                }
                true
            } else {
                false
            }
        };
        if cancelled {
            // 终态快照持久化（与 fail 收口一致）；不持任何锁
            store::update_status(id, TaskStatus::Cancelled);
        }
        cancelled
    }

    /// 取走任务清理路径（取消收口用）：任务侧退出、下载停止后异步整删，
    /// 避免取消时与仍在写的下载竞争。
    pub fn take_cleanup_path(&self, id: &str) -> Option<std::path::PathBuf> {
        let mut inner = self.inner.lock();
        inner.tasks.get_mut(id).and_then(|h| h.cleanup_path.take())
    }

    pub fn fail(&self, id: &str) {
        // 终态快照持久化：移除内存注册前记录 status（崩溃/重启后恢复依据）。
        // 所有终态（done/error/cancelled）统一收口此处——DB 故障静默，不影响任务。
        // 锁序统一 inner→progress（与 cancel/list 一致）；DB 写在 progress 锁
        // 释放后进行——DB（sqlite）卡住不持有任何锁阻塞任务/取消。
        let status = self
            .inner
            .lock()
            .tasks
            .get(id)
            .map(|h| h.progress.lock().status);
        if let Some(status) = status {
            store::update_status(id, status);
        }
        self.inner.lock().tasks.remove(id);
    }

    pub fn list(&self) -> Vec<TaskProgress> {
        let inner = self.inner.lock();
        inner
            .tasks
            .values()
            .map(|h| h.progress.lock().clone())
            .collect()
    }

    /// 任务是否仍注册（存在 = 运行中；终态已由 fail/remove 移除）。
    /// 前端用于校正"预注册与终态事件竞态"：查不到即任务已结束。
    pub fn exists(&self, id: &str) -> bool {
        self.inner.lock().tasks.contains_key(id)
    }

    pub fn remove(&self, id: &str) {
        // 终态快照持久化（与 fail 收口一致）：完成/取消/失败统一写回终态 status，
        // 否则完成任务快照残留 running，下次启动被 mark_crashed_as_interrupted
        // 误标为"未完成任务"（横幅误报 + 忽略按钮泛滥）。
        // 锁序 inner→progress；DB 写在 progress 锁释放后进行（sqlite 2s 超时静默）。
        let status = self
            .inner
            .lock()
            .tasks
            .get(id)
            .map(|h| h.progress.lock().status);
        if let Some(status) = status {
            store::update_status(id, status);
        }
        self.inner.lock().tasks.remove(id);
        self.inner.lock().waiting.remove(id);
    }

    /// 登记挂起等待用户决策：返回决策槽（任务侧轮询）。重复登记返回同一槽。
    pub fn begin_wait(&self, id: &str) -> Arc<Mutex<Option<WaitingDecision>>> {
        let mut inner = self.inner.lock();
        inner
            .waiting
            .entry(id.to_string())
            .or_insert_with(|| Arc::new(Mutex::new(None)))
            .clone()
    }

    /// 用户写入决策（命令侧）。任务未在挂起状态时返回 false。
    pub fn decide(&self, id: &str, decision: WaitingDecision) -> bool {
        let inner = self.inner.lock();
        match inner.waiting.get(id) {
            Some(slot) => {
                *slot.lock() = Some(decision);
                true
            }
            None => false,
        }
    }

    /// 任务侧轮询当前决策（挂起期间每 200ms 调用）；未决策返回 None。
    pub fn waiting_decision(&self, id: &str) -> Option<WaitingDecision> {
        let inner = self.inner.lock();
        inner.waiting.get(id).and_then(|s| *s.lock())
    }

    /// 清理挂起登记（决策已收到 / 任务终态）。
    pub fn end_wait(&self, id: &str) {
        self.inner.lock().waiting.remove(id);
    }
}

pub struct PathLock {
    tm: Arc<TaskManager>,
    path: std::path::PathBuf,
}

impl PathLock {
    pub fn try_acquire(tm: &Arc<TaskManager>, path: &std::path::Path) -> Option<PathLock> {
        if tm.acquire(path) {
            Some(PathLock {
                tm: tm.clone(),
                path: path.to_path_buf(),
            })
        } else {
            None
        }
    }
}

impl Drop for PathLock {
    fn drop(&mut self) {
        self.tm.release(&self.path);
    }
}

/// 轮询等待获取路径锁（跨任务互斥）：另一任务正占用同一目标路径时
/// 短暂等待其完成，避免多任务并发写同一文件导致 sha1 校验互相踩踏
/// （不同版本共用 libraries/assets 目录，PathLock 只锁版本目录不覆盖文件）。
pub async fn wait_path_lock(path: &std::path::Path) -> PathLock {
    let tm = global();
    loop {
        if let Some(l) = PathLock::try_acquire(tm, path) {
            return l;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
}

pub(crate) fn emit(app: &AppHandle, p: &TaskProgress) {
    emit_event(app, crate::events::TASK_PROGRESS, p.clone());
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
            eprintln!("[emit] 进度锁被持有 {}ms（等待中，正常短临界区不应出现）", waited_ms);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_lock_acquire_release() {
        let tm = Arc::new(TaskManager::default());
        let p = std::path::Path::new("C:/test/versions/26.2");
        assert!(tm.acquire(p));
        assert!(!tm.acquire(p), "同路径二次占用应失败");
        tm.release(p);
        assert!(tm.acquire(p), "释放后可再次占用");
        tm.release(p);
    }

    #[test]
    fn path_lock_guard_drops_automatically() {
        let tm = Arc::new(TaskManager::default());
        let p = std::path::Path::new("C:/test/versions/26.2");
        {
            let _g = PathLock::try_acquire(&tm, p).expect("首次占用应成功");
            assert!(!tm.acquire(p), "守卫持有期间占用应失败");
        }
        assert!(tm.acquire(p), "守卫 Drop 后占用应成功");
        tm.release(p);
    }

    #[test]
    fn task_status_serde_lowercase() {
        // 序列化结果与前端 payload 约定的字符串一致（前端 ProgressPayload.status 为 string）
        assert_eq!(
            serde_json::to_string(&TaskStatus::Pending).unwrap(),
            "\"pending\""
        );
        assert_eq!(
            serde_json::to_string(&TaskStatus::Running).unwrap(),
            "\"running\""
        );
        assert_eq!(
            serde_json::to_string(&TaskStatus::Done).unwrap(),
            "\"done\""
        );
        assert_eq!(
            serde_json::to_string(&TaskStatus::Error).unwrap(),
            "\"error\""
        );
        assert_eq!(
            serde_json::to_string(&TaskStatus::Cancelled).unwrap(),
            "\"cancelled\""
        );
        assert_eq!(
            serde_json::to_string(&StepStatus::Pending).unwrap(),
            "\"pending\""
        );
        assert_eq!(
            serde_json::to_string(&StepStatus::Running).unwrap(),
            "\"running\""
        );
        assert_eq!(
            serde_json::to_string(&StepStatus::Done).unwrap(),
            "\"done\""
        );
    }

    /// WaitingForUser：begin_wait 后轮询无决策；decide 后轮询命中；end_wait 清理；
    /// 未挂起任务 decide 返回 false。
    #[test]
    fn waiting_decision_flow() {
        let tm = Arc::new(TaskManager::default());
        let slot = tm.begin_wait("t1");
        assert!(tm.waiting_decision("t1").is_none(), "未决策时轮询应为 None");
        // 任务侧轮询（模拟 200ms 后用户决策）
        std::thread::spawn({
            let tm = tm.clone();
            move || {
                std::thread::sleep(std::time::Duration::from_millis(50));
                tm.decide("t1", WaitingDecision::Skip);
            }
        });
        // 任务侧等待循环（简化：轮询至多 1s）
        let mut got = None;
        for _ in 0..10 {
            if let Some(d) = tm.waiting_decision("t1") {
                got = Some(d);
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        assert_eq!(got, Some(WaitingDecision::Skip), "决策应被任务侧读到");
        drop(slot);
        tm.end_wait("t1");
        assert!(
            !tm.decide("t1", WaitingDecision::Cancel),
            "已清理的挂起任务 decide 应返回 false"
        );
    }

    /// remove 收口必须写回终态快照：完成任务若残留 running 快照，
    /// 下次启动被 mark_crashed_as_interrupted 误标为"未完成任务"（横幅误报）。
    #[test]
    fn remove_persists_terminal_status_to_snapshot() {
        let tm = Arc::new(TaskManager::default());
        let (task_id, _cancel, progress) = tm.create("测试任务", &["步骤"]);
        super::store::save_snapshot(
            &task_id,
            "测试任务",
            &super::store::JobSpec::Repair { version_dir_name: "26.2".into() },
            TaskStatus::Running,
        );
        assert!(
            super::store::list_interrupted().iter().any(|j| j.id == task_id),
            "running 快照应出现在中断列表"
        );
        // 模拟任务完成：status=Done 后 remove 收口
        progress.lock().status = TaskStatus::Done;
        tm.remove(&task_id);
        assert!(!tm.exists(&task_id), "remove 应移除内存注册");
        assert!(
            !super::store::list_interrupted().iter().any(|j| j.id == task_id),
            "完成后快照应写回终态，不得被列为中断任务"
        );
        super::store::remove_snapshot(&task_id);
    }
}
