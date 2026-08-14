// ── task 模块门面：TaskManager 单例 + 事件推送 + 类型重导出 ──
// 职责分区：model.rs（serde 任务配置）/ steps.rs（步骤分析）/ progress.rs（进度上报）/
// repair.rs（缺失扫描与修复）/ files.rs（批量文件下载）/ runner.rs（完整下载主体）。

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use serde::Serialize;
use tauri::{AppHandle, Emitter};

pub mod files;
pub mod model;
pub mod progress;
pub mod repair;
pub mod runner;
pub mod steps;
pub mod store;

pub use model::*;
pub use steps::*;

static TM: OnceLock<Arc<TaskManager>> = OnceLock::new();

pub fn init() {
    TM.set(Arc::new(TaskManager::default())).ok();
    // 崩溃恢复（#6）：上次进程退出时残留 running 快照 → interrupted，
    // 前端启动后经 list_interrupted_tasks 拉取并询问恢复。
    store::mark_crashed_as_interrupted();
}

pub fn global() -> &'static Arc<TaskManager> {
    TM.get().expect("TaskManager not initialized")
}

#[derive(Debug, Clone, Serialize)]
pub struct TaskProgress {
    pub task_id: String,
    pub name: String,
    pub status: TaskStatus, // pending | running | done | error | cancelled（serde lowercase）
    pub progress: f64,      // 0.0 – 100.0
    pub speed_bps: f64,
    /// 预计剩余秒数（剩余字节 / 当前速度）；停滞（>4s 无字节推进）或速度过低时 None。
    pub eta_seconds: Option<u64>,
    pub completed_files: u32,
    pub total_files: u32,
    pub completed_bytes: u64,
    pub total_bytes: u64,
    pub current_phase: String,
    pub steps: Vec<StepInfo>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct StepInfo {
    pub name: String,
    pub status: StepStatus, // pending | running | done
    pub progress: f64,      // 0–100, meaningful only when status=running
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
        let mut inner = self.inner.lock().unwrap();
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
    pub fn acquire(&self, path: &std::path::Path) -> bool {
        let mut inner = self.inner.lock().unwrap();
        if inner.active_paths.contains_key(path) {
            return false;
        }
        inner.active_paths.insert(path.to_path_buf(), ());
        true
    }

    pub fn release(&self, path: &std::path::Path) {
        self.inner.lock().unwrap().active_paths.remove(path);
    }

    pub fn set_cleanup_path(&self, id: &str, path: std::path::PathBuf) {
        if let Some(h) = self.inner.lock().unwrap().tasks.get_mut(id) {
            h.cleanup_path = Some(path);
        }
    }

    pub fn cancel(&self, id: &str) -> bool {
        let inner = self.inner.lock().unwrap();
        if let Some(h) = inner.tasks.get(id) {
            h.cancel.store(true, Ordering::SeqCst);
            if let Some(ref p) = h.cleanup_path {
                let _ = std::fs::remove_dir_all(p);
            }
            if let Ok(mut p) = h.progress.try_lock() {
                p.status = TaskStatus::Cancelled;
                // 终态快照持久化（与 fail 收口一致）
                store::update_status(id, TaskStatus::Cancelled);
            }
            true
        } else {
            false
        }
    }

    pub fn fail(&self, id: &str) {
        // 终态快照持久化：移除内存注册前记录 status（崩溃/重启后恢复依据）。
        // 所有终态（done/error/cancelled）统一收口此处——DB 故障静默，不影响任务。
        if let Some(h) = self.inner.lock().unwrap().tasks.get(id) {
            let status = h.progress.lock().unwrap().status;
            store::update_status(id, status);
        }
        self.inner.lock().unwrap().tasks.remove(id);
    }

    pub fn list(&self) -> Vec<TaskProgress> {
        let inner = self.inner.lock().unwrap();
        inner
            .tasks
            .values()
            .map(|h| h.progress.lock().unwrap().clone())
            .collect()
    }

    /// 任务是否仍注册（存在 = 运行中；终态已由 fail/remove 移除）。
    /// 前端用于校正"预注册与终态事件竞态"：查不到即任务已结束。
    pub fn exists(&self, id: &str) -> bool {
        self.inner.lock().unwrap().tasks.contains_key(id)
    }

    pub fn remove(&self, id: &str) {
        self.inner.lock().unwrap().tasks.remove(id);
        self.inner.lock().unwrap().waiting.remove(id);
    }

    /// 登记挂起等待用户决策：返回决策槽（任务侧轮询）。重复登记返回同一槽。
    pub fn begin_wait(&self, id: &str) -> Arc<Mutex<Option<WaitingDecision>>> {
        let mut inner = self.inner.lock().unwrap();
        inner
            .waiting
            .entry(id.to_string())
            .or_insert_with(|| Arc::new(Mutex::new(None)))
            .clone()
    }

    /// 用户写入决策（命令侧）。任务未在挂起状态时返回 false。
    pub fn decide(&self, id: &str, decision: WaitingDecision) -> bool {
        let inner = self.inner.lock().unwrap();
        match inner.waiting.get(id) {
            Some(slot) => {
                *slot.lock().unwrap() = Some(decision);
                true
            }
            None => false,
        }
    }

    /// 任务侧轮询当前决策（挂起期间每 200ms 调用）；未决策返回 None。
    pub fn waiting_decision(&self, id: &str) -> Option<WaitingDecision> {
        let inner = self.inner.lock().unwrap();
        inner.waiting.get(id).and_then(|s| *s.lock().unwrap())
    }

    /// 清理挂起登记（决策已收到 / 任务终态）。
    pub fn end_wait(&self, id: &str) {
        self.inner.lock().unwrap().waiting.remove(id);
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

pub fn emit(app: &AppHandle, p: &TaskProgress) {
    let _ = app.emit(crate::events::TASK_PROGRESS, p);
}

pub fn update_and_emit(
    app: &AppHandle,
    guard: &Arc<Mutex<TaskProgress>>,
    f: impl FnOnce(&mut TaskProgress),
) {
    if let Ok(mut p) = guard.lock() {
        f(&mut p);
        emit(app, &p);
    }
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
}
