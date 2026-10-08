//! 工具箱下载任务注册表与调度：入队 / 暂停 / 继续 / 取消 / 移除 + 并发槽位控制。
//!
//! 调度模型：每个任务入队时 spawn 一个等待者，轮询占用并发槽位（上限读
//! [`MAX_FILES`] 缓存，保存配置即刻生效），拿到槽位后交给 [`runner::run`]；
//! 队列等待期间显示 [`ToolDlStatus::Queued`]，可被取消。

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicU8, Ordering};
use std::sync::{Arc, LazyLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use parking_lot::Mutex;

use super::model::{StopReason, ToolDlSnapshot, ToolDlStatus, ToolDlTask};
use super::runner;
use crate::config::tooldl::{ToolDlConfig, DEFAULT_MAX_FILES};
use crate::downloader::segmented;
use crate::task::EventSink;
use crate::{constants, persist, util};

/// 全部任务（保持入队顺序）。
static TASKS: LazyLock<Mutex<Vec<Arc<ToolDlTask>>>> = LazyLock::new(|| Mutex::new(Vec::new()));
/// 正在下载的任务数（并发槽位占用）。
static RUNNING: LazyLock<Mutex<u32>> = LazyLock::new(|| Mutex::new(0));
/// 任务 id 序号（同一秒内多次入队不撞号）。
static SEQ: LazyLock<Mutex<u64>> = LazyLock::new(|| Mutex::new(0));
/// 并发上限缓存：避免调度轮询反复读盘；保存配置与进程启动时同步。
static MAX_FILES: AtomicU32 = AtomicU32::new(DEFAULT_MAX_FILES);

/// 进程启动时把持久化的并发上限载入缓存。
pub fn init() {
    let config: ToolDlConfig = persist::read(constants::SECT_TOOL_DL).unwrap_or_default();
    set_max_files(config.max_files);
}

/// 更新并发上限缓存（保存配置后调用）。
pub fn set_max_files(max_files: u32) {
    MAX_FILES.store(max_files.max(1), Ordering::Relaxed);
}

/// 入队一批下载链接，返回新任务 id 列表（同批内按顺序排队）。
pub fn enqueue(
    sink: Arc<dyn EventSink>,
    urls: Vec<String>,
    save_dir: String,
    segments: u32,
) -> Result<Vec<String>, String> {
    let dir = PathBuf::from(save_dir.trim());
    if dir.as_os_str().is_empty() {
        return Err("请先选择保存目录".into());
    }
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建保存目录失败\n{e}"))?;

    let segments = segments.clamp(1, 64);
    let mut created = Vec::new();
    for raw in urls {
        let url = raw.trim().to_string();
        if url.is_empty() {
            continue;
        }
        if !(url.starts_with("http://") || url.starts_with("https://")) {
            return Err(format!("仅支持 http/https 链接\n{url}"));
        }
        let task = Arc::new(build_task(url, &dir, segments));
        created.push(task.id.clone());
        TASKS.lock().push(Arc::clone(&task));
        spawn_waiter(Arc::clone(&sink), task);
    }
    if created.is_empty() {
        return Err("请至少填入一个下载链接".into());
    }
    Ok(created)
}

/// 暂停一个正在下载的任务（排队中 / 已终态返回错误）。
pub fn pause(task_id: &str) -> Result<(), String> {
    let task = find(task_id).ok_or("任务不存在")?;
    if task.status() != ToolDlStatus::Downloading {
        return Err("任务未在下载".into());
    }
    task.request_stop(StopReason::Pause);
    Ok(())
}

/// 继续（暂停）或重试（失败）：重置停止标志与错误后重新排队（分段路径断点续传）。
pub fn resume(sink: Arc<dyn EventSink>, task_id: &str) -> Result<(), String> {
    let task = find(task_id).ok_or("任务不存在")?;
    let status = task.status();
    if status != ToolDlStatus::Paused && status != ToolDlStatus::Error {
        return Err("任务当前状态无法继续".into());
    }
    task.clear_stop();
    task.speed.store(0, Ordering::Relaxed);
    *task.error.lock() = None;
    task.set_status(ToolDlStatus::Queued);
    spawn_waiter(sink, task);
    Ok(())
}

/// 取消任务并清理中间产物（正在下载时由执行体完成清理）。
pub fn cancel(task_id: &str) -> Result<(), String> {
    let task = find(task_id).ok_or("任务不存在")?;
    let status = task.status();
    if matches!(status, ToolDlStatus::Done | ToolDlStatus::Cancelled) {
        return Err("任务已结束".into());
    }
    task.request_stop(StopReason::Cancel);
    if status == ToolDlStatus::Queued {
        // 尚未进入下载循环：立即收口（等待者稍后检测到标志再做一次幂等收口）
        cleanup_partials(&task.dest);
        task.set_status(ToolDlStatus::Cancelled);
    }
    Ok(())
}

/// 从列表移除任务（未结束的先取消，中间产物由执行体清理）。
pub fn remove(task_id: &str) -> Result<(), String> {
    let task = {
        let mut tasks = TASKS.lock();
        match tasks.iter().position(|t| t.id == task_id) {
            Some(index) => tasks.remove(index),
            None => return Err("任务不存在".into()),
        }
    };
    if !matches!(
        task.status(),
        ToolDlStatus::Done | ToolDlStatus::Error | ToolDlStatus::Cancelled
    ) {
        task.request_stop(StopReason::Cancel);
    }
    Ok(())
}

/// 清除所有已结束（完成 / 失败 / 取消）的任务记录。
pub fn clear_finished() {
    TASKS.lock().retain(|t| {
        !matches!(
            t.status(),
            ToolDlStatus::Done | ToolDlStatus::Error | ToolDlStatus::Cancelled
        )
    });
}

/// 全部任务快照（前端页面重进时恢复列表）。
pub fn list() -> Vec<ToolDlSnapshot> {
    TASKS.lock().iter().map(|t| t.snapshot()).collect()
}

/// 任务是否仍在注册表中（执行体据此跳过已移除任务的进度派发）。
pub(crate) fn contains(task_id: &str) -> bool {
    TASKS.lock().iter().any(|t| t.id == task_id)
}

/// 清理分段中间产物（`.part` / `.part.json`）。
pub(crate) fn cleanup_partials(dest: &Path) {
    let (part, meta) = segmented::partial_paths(dest);
    let _ = std::fs::remove_file(part);
    let _ = std::fs::remove_file(meta);
}

fn find(task_id: &str) -> Option<Arc<ToolDlTask>> {
    TASKS.lock().iter().find(|t| t.id == task_id).cloned()
}

/// 等待空闲并发槽位后启动执行体（Queued → Downloading）。
fn spawn_waiter(sink: Arc<dyn EventSink>, task: Arc<ToolDlTask>) {
    tokio::spawn(async move {
        loop {
            if task.is_stopped() {
                // 排队期间被取消：不占槽位，直接收口
                finish_queued(&task, &sink);
                return;
            }
            if try_acquire_slot() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(150)).await;
        }
        task.set_status(ToolDlStatus::Downloading);
        runner::run(Arc::clone(&task), Arc::clone(&sink)).await;
        release_slot();
    });
}

/// 尝试占一个并发槽位（上限取当前缓存，改动即时生效）。
fn try_acquire_slot() -> bool {
    let mut running = RUNNING.lock();
    if *running >= MAX_FILES.load(Ordering::Relaxed) {
        return false;
    }
    *running += 1;
    true
}

fn release_slot() {
    let mut running = RUNNING.lock();
    *running = running.saturating_sub(1);
}

/// 排队等待期间被处理的收口（从未进入下载循环）。
fn finish_queued(task: &Arc<ToolDlTask>, sink: &Arc<dyn EventSink>) {
    if task.stop_reason() == StopReason::Cancel {
        cleanup_partials(&task.dest);
        task.set_status(ToolDlStatus::Cancelled);
    } else {
        task.set_status(ToolDlStatus::Paused);
    }
    runner::emit_snapshot(sink, task);
}

fn build_task(url: String, dir: &Path, segments: u32) -> ToolDlTask {
    let file_name = file_name_from_url(&url);
    let dest = unique_dest(dir, &file_name);
    ToolDlTask {
        id: next_id(),
        url,
        file_name,
        dest,
        save_dir: dir.to_path_buf(),
        segments,
        created_at: now_secs(),
        downloaded: AtomicU64::new(0),
        total: AtomicU64::new(0),
        speed: AtomicU64::new(0),
        ranged: AtomicBool::new(false),
        segments_used: AtomicU32::new(segments),
        status: Mutex::new(ToolDlStatus::Queued),
        error: Mutex::new(None),
        stop: AtomicBool::new(false),
        stop_reason: AtomicU8::new(StopReason::None.as_u8()),
    }
}

/// URL → 落盘文件名：去掉 query/fragment，再过 Windows 保留名与字符清洗。
fn file_name_from_url(url: &str) -> String {
    let path = url.split(['?', '#']).next().unwrap_or(url);
    let raw = util::url_file_name(path).unwrap_or("download.bin");
    util::sanitize_fs_name(raw, "download.bin")
}

/// 目标路径去重：同名文件已存在时追加 `_1`、`_2`…，兜底追加内容哈希。
fn unique_dest(dir: &Path, file_name: &str) -> PathBuf {
    let candidate = dir.join(file_name);
    if !candidate.exists() {
        return candidate;
    }
    let path = Path::new(file_name);
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("download");
    let ext = path.extension().and_then(|s| s.to_str());
    for n in 1..1000 {
        let name = match ext {
            Some(ext) => format!("{stem}_{n}.{ext}"),
            None => format!("{stem}_{n}"),
        };
        let candidate = dir.join(&name);
        if !candidate.exists() {
            return candidate;
        }
    }
    dir.join(format!("{stem}_{}", util::fnv1a64(file_name)))
}

fn next_id() -> String {
    let mut seq = SEQ.lock();
    *seq += 1;
    format!("tdl-{}-{}", now_secs(), *seq)
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
