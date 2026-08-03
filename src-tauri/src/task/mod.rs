use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use serde::Serialize;
use tauri::{AppHandle, Emitter};

pub mod runner;

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
    fn default() -> Self { Self { inner: Mutex::new(Inner { tasks: HashMap::new(), counter: 0 }) } }
}

struct Inner {
    tasks: HashMap<String, TaskHandle>,
    counter: u64,
}

struct TaskHandle {
    cancel: Arc<AtomicBool>,
    progress: Arc<Mutex<TaskProgress>>,
    cleanup_path: Option<std::path::PathBuf>,
}

impl TaskManager {
    /// Register a new task, return (task_id, cancel_token, progress_guard).
    pub fn create(&self, name: &str, steps: &[&str]) -> (String, Arc<AtomicBool>, Arc<Mutex<TaskProgress>>) {
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
            steps: steps.iter().map(|s| StepInfo {
                name: s.to_string(),
                status: "pending".into(),
                progress: 0.0,
            }).collect(),
            error: None,
        }));
        inner.tasks.insert(
            id.clone(),
            TaskHandle { cancel: cancel.clone(), progress: progress.clone(), cleanup_path: None },
        );
        (id, cancel, progress)
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

    pub fn list(&self) -> Vec<TaskProgress> {
        let inner = self.inner.lock().unwrap();
        inner.tasks.values().filter_map(|h| h.progress.try_lock().ok().map(|p| p.clone())).collect()
    }

    pub fn remove(&self, id: &str) {
        self.inner.lock().unwrap().tasks.remove(id);
    }
}

// ── helpers ────────────────────────────────────

/// Emit a progress event to the frontend.
pub fn emit(app: &AppHandle, p: &TaskProgress) {
    let _ = app.emit(crate::events::TASK_PROGRESS, p);
}

/// Update progress fields and emit.
pub fn update_and_emit(app: &AppHandle, guard: &Arc<Mutex<TaskProgress>>, f: impl FnOnce(&mut TaskProgress)) {
    if let Ok(mut p) = guard.lock() {
        f(&mut p);
        emit(app, &p);
    }
}
