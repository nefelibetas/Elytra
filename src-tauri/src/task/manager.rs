//! TaskManager 单例：任务注册表、终态收口、挂起决策与崩溃恢复初始化。

use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock};

use super::model::{StepInfo, StepStatus, TaskProgress, TaskStatus};
use super::store;

/// 全局任务管理器（首次访问即构造，无独立初始化路径）。
static TM: LazyLock<Arc<TaskManager>> = LazyLock::new(|| Arc::new(TaskManager::default()));

/// 启动期钩子（`lib.rs` 调一次）：崩溃恢复（#6）——上次进程退出时残留
/// running 快照 → interrupted，前端启动后经 list_interrupted_tasks 拉取并询问恢复。
pub fn init() {
    store::mark_crashed_as_interrupted();
}

pub fn global() -> &'static Arc<TaskManager> {
    &TM
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

pub struct TaskManager {
    inner: Mutex<Inner>,
}

impl Default for TaskManager {
    fn default() -> Self {
        Self {
            inner: Mutex::new(Inner {
                tasks: HashMap::new(),
                waiting: HashMap::new(),
                counter: 0,
            }),
        }
    }
}

struct Inner {
    tasks: HashMap<String, TaskHandle>,
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

    pub fn set_cleanup_path(&self, id: &str, path: std::path::PathBuf) {
        if let Some(h) = self.inner.lock().tasks.get_mut(id) {
            h.cleanup_path = Some(path);
        }
    }

    /// 暂停任务：设取消标志停下载 + 取走清理路径（保留已下载目录，恢复时文件级
    /// sha1 幂等只补缺失） + 状态标 Paused + 移除内存注册。DB 快照保留（status=paused），
    /// 供前端列表/恢复（resume_task 复用崩溃恢复路径重跑快照）。
    /// 任务线程随后检测到 cancel flag 退出，其 set_cancelled 收尾因注册已移除而 no-op；
    /// 它仍会 emit 一次 cancelled 事件，前端以 paused 状态为准忽略之。
    pub fn pause(&self, id: &str) -> bool {
        let paused = {
            let mut inner = self.inner.lock();
            if let Some(h) = inner.tasks.get_mut(id) {
                h.cancel.store(true, Ordering::SeqCst);
                // 取走清理路径：暂停不清目录（下载产物保留，恢复只补缺失文件）
                let _ = h.cleanup_path.take();
                if let Some(mut p) = h.progress.try_lock() {
                    p.status = TaskStatus::Paused;
                }
                true
            } else {
                false
            }
        };
        if paused {
            // DB 标 paused（快照保留）；不持任何锁
            store::update_status(id, TaskStatus::Paused);
            // 移除内存注册（与终态收口一致）；runner 线程随 cancel flag 自然退出
            self.inner.lock().tasks.remove(id);
        }
        paused
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

#[cfg(test)]
mod tests {
    use super::*;

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
        store::save_snapshot(
            &task_id,
            "测试任务",
            &store::JobSpec::Repair {
                version_dir_name: "26.2".into(),
            },
            TaskStatus::Running,
        );
        assert!(
            store::list_interrupted().iter().any(|j| j.id == task_id),
            "running 快照应出现在中断列表"
        );
        // 模拟任务完成：status=Done 后 remove 收口
        progress.lock().status = TaskStatus::Done;
        tm.remove(&task_id);
        assert!(!tm.exists(&task_id), "remove 应移除内存注册");
        assert!(
            !store::list_interrupted().iter().any(|j| j.id == task_id),
            "完成后快照应写回终态，不得被列为中断任务"
        );
        store::remove_snapshot(&task_id);
    }
}
