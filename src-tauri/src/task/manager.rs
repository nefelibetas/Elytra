//! TaskManager 单例：任务注册表、终态收口、挂起决策与崩溃恢复初始化。

use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock};

use super::model::{StepInfo, StepStatus, TaskProgress, TaskStatus, TaskText};
use super::steps::StepSpec;
use super::store;

/// 全局任务管理器（首次访问即构造，无独立初始化路径）。
static TM: LazyLock<Arc<TaskManager>> = LazyLock::new(|| Arc::new(TaskManager::default()));

/// 启动期钩子（`lib.rs` 调一次）：残留 running 快照 → interrupted（前端拉取后询问恢复）。
pub fn init() {
    store::mark_crashed_as_interrupted();
}

pub fn global() -> &'static Arc<TaskManager> {
    &TM
}

/// 挂起任务的用户决策（批量下载单文件失败时暂停，用户选跳过/重试/取消）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WaitingDecision {
    Skip,
    Retry,
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

/// 进程级唯一前缀（启动毫秒 + pid 十六进制）：counter 从 0 起，无前缀则新 id `t{n}` 与崩溃残留的
/// install_jobs 行撞主键（INSERT OR REPLACE 覆盖），恢复时执行错 spec。
fn process_prefix() -> &'static str {
    static PREFIX: LazyLock<String> = LazyLock::new(|| {
        let ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        format!("{ms:x}{:x}", std::process::id())
    });
    PREFIX.as_str()
}

impl TaskManager {
    /// 新建任务，返回 (task_id, 取消标志, 进度锁)。
    pub fn create(
        &self,
        name: &str,
        text: &TaskText,
        steps: &[StepSpec],
    ) -> (String, Arc<AtomicBool>, Arc<Mutex<TaskProgress>>) {
        let mut inner = self.inner.lock();
        let id = format!("t{}-{}", process_prefix(), inner.counter);
        inner.counter += 1;
        let cancel = Arc::new(AtomicBool::new(false));
        let progress = Arc::new(Mutex::new(TaskProgress {
            task_id: id.clone(),
            name: name.to_string(),
            name_key: text.key.clone(),
            name_params: text.params.clone(),
            status: TaskStatus::Pending,
            progress: 0.0,
            speed_bps: 0.0,
            eta_seconds: None,
            completed_files: 0,
            total_files: 0,
            completed_bytes: 0,
            total_bytes: 0,
            current_phase: "等待中".into(),
            current_phase_key: Some("task.backend.phase.waiting".into()),
            current_phase_params: None,
            steps: steps
                .iter()
                .map(|s| StepInfo {
                    name: s.name.clone(),
                    kind: Some(s.kind),
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

    /// 暂停：置取消标志停下载 + 取走清理路径（保留已下载目录，恢复只补缺失）+ 标 Paused + 移除内存注册；DB 快照保留
    /// （paused），resume_task 复用崩溃恢复路径重跑。任务线程随 cancel flag 退出，其 set_cancelled 因注册已移除 no-op 但仍 emit cancelled，前端以 paused 为准。
    pub fn pause(&self, id: &str) -> bool {
        let paused = {
            let mut inner = self.inner.lock();
            if let Some(h) = inner.tasks.get_mut(id) {
                h.cancel.store(true, Ordering::SeqCst);
                // 取走清理路径：暂停不清目录，恢复只补缺失文件
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
            // 移除内存注册；runner 线程随 cancel flag 退出
            self.inner.lock().tasks.remove(id);
        }
        paused
    }

    pub fn cancel(&self, id: &str) -> bool {
        let cancelled = {
            let inner = self.inner.lock();
            if let Some(h) = inner.tasks.get(id) {
                h.cancel.store(true, Ordering::SeqCst);
                // 目录清理移交任务侧 set_cancelled 收口：同步 remove_dir_all 会阻塞 worker（大目录数十秒）
                // 且与仍在跑的下载竞争写文件。DB 写在 progress 锁外（DB 卡住不阻塞取消）。
                if let Some(mut p) = h.progress.try_lock() {
                    p.status = TaskStatus::Cancelled;
                }
                true
            } else {
                false
            }
        };
        if cancelled {
            // 终态快照持久化；不持任何锁
            store::update_status(id, TaskStatus::Cancelled);
        }
        cancelled
    }

    /// 取走任务清理路径（取消收口用）：任务侧退出后异步整删，避免与仍在写的下载竞争。
    pub fn take_cleanup_path(&self, id: &str) -> Option<std::path::PathBuf> {
        let mut inner = self.inner.lock();
        inner.tasks.get_mut(id).and_then(|h| h.cleanup_path.take())
    }

    pub fn fail(&self, id: &str) {
        // 终态（done/error/cancelled）唯一收口入口：移除内存注册前落 status 快照。
        // 锁序 inner→progress；DB 写在 progress 锁释放后（DB 卡住不持锁阻塞任务/取消）。
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

    /// 任务是否仍注册（存在 = 运行中；终态已由 fail/remove 移除）；前端据此校正预注册与终态事件竞态。
    pub fn exists(&self, id: &str) -> bool {
        self.inner.lock().tasks.contains_key(id)
    }

    pub fn remove(&self, id: &str) {
        // 终态快照持久化：否则完成任务残留 running，下次启动被 mark_crashed_as_interrupted 误标为中断。
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
    use crate::task::StepKind;

    fn file_steps(names: &[&str]) -> Vec<StepSpec> {
        names
            .iter()
            .map(|name| StepSpec {
                name: (*name).to_string(),
                kind: StepKind::File,
            })
            .collect()
    }

    /// 任务 id 跨进程唯一（带启动前缀），否则重启后 `t0` 撞崩溃残留行主键。
    #[test]
    fn task_ids_carry_process_prefix_and_increment() {
        let tm = TaskManager::default();
        let (id1, ..) = tm.create("a", &TaskText::default(), &file_steps(&["step"]));
        let (id2, ..) = tm.create("b", &TaskText::default(), &file_steps(&["step"]));
        assert_ne!(id1, id2);
        assert!(
            id1.starts_with(&format!("t{}-", process_prefix())),
            "任务 id 应带进程前缀: {id1}"
        );
        assert!(!process_prefix().is_empty());
    }

    /// WaitingForUser：begin_wait 后轮询无决策 → decide 命中 → end_wait 清理；未挂起 decide 返回 false。
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

    /// remove 收口必须写回终态快照：否则完成任务残留 running，下次启动被误标为中断（横幅误报）。
    #[test]
    fn remove_persists_terminal_status_to_snapshot() {
        let tm = Arc::new(TaskManager::default());
        let (task_id, _cancel, progress) =
            tm.create("测试任务", &TaskText::default(), &file_steps(&["步骤"]));
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
