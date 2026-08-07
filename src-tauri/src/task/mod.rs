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

pub use model::*;
pub use steps::*;

static TM: OnceLock<Arc<TaskManager>> = OnceLock::new();

pub fn init() {
    TM.set(Arc::new(TaskManager::default())).ok();
}

pub fn global() -> &'static Arc<TaskManager> {
    TM.get().expect("TaskManager not initialized")
}

#[derive(Debug, Clone, Serialize)]
pub struct TaskProgress {
    pub task_id: String,
    pub name: String,
    pub status: String, // pending | running | done | error | cancelled
    pub progress: f64,  // 0.0 – 100.0
    pub speed_bps: f64,
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
    pub status: String, // pending | running | done | error
    pub progress: f64,  // 0–100, meaningful only when status=running
}

pub struct TaskManager {
    inner: Mutex<Inner>,
}

impl Default for TaskManager {
    fn default() -> Self {
        Self {
            inner: Mutex::new(Inner {
                tasks: HashMap::new(),
                active_paths: HashMap::new(),
                counter: 0,
            }),
        }
    }
}

struct Inner {
    tasks: HashMap<String, TaskHandle>,
    /// 被下载任务占用的路径（版本目录 / 目标文件），同路径并发互斥
    active_paths: HashMap<std::path::PathBuf, ()>,
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
            status: "pending".into(),
            progress: 0.0,
            speed_bps: 0.0,
            completed_files: 0,
            total_files: 0,
            completed_bytes: 0,
            total_bytes: 0,
            current_phase: "等待中".into(),
            steps: steps
                .iter()
                .map(|s| StepInfo {
                    name: s.to_string(),
                    status: "pending".into(),
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
                p.status = "cancelled".into();
            }
            true
        } else {
            false
        }
    }

    pub fn fail(&self, id: &str) {
        self.inner.lock().unwrap().tasks.remove(id);
    }

    pub fn list(&self) -> Vec<TaskProgress> {
        let inner = self.inner.lock().unwrap();
        inner
            .tasks
            .values()
            .filter_map(|h| h.progress.try_lock().ok().map(|p| p.clone()))
            .collect()
    }

    pub fn remove(&self, id: &str) {
        self.inner.lock().unwrap().tasks.remove(id);
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
}
