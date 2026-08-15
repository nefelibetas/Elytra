// ── 批量文件下载任务（mod / 资源包 / 数据包 / 光影包垂直切片）──

use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use parking_lot::Mutex;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tauri::AppHandle;

use crate::config::downloader::model::DownloaderConfig;
use crate::constants;
use crate::downloader::fetcher;
use crate::persist;
use crate::task::model::{DownloadFileKind, DownloadFilesTask};
use crate::task::{self, StepStatus, TaskProgress, TaskStatus, WaitingDecision};

use super::progress::{
    cancelled, set_cancelled, set_error, set_phase, step_done, update_progress,
};

/// 单文件目标路径（纯函数，供单测）：versions/{version_dir}/{kind_dir}/{filename}。
fn file_dest(
    mc: &Path,
    version_dir: &str,
    kind: DownloadFileKind,
    filename: &str,
) -> std::path::PathBuf {
    mc.join("versions")
        .join(version_dir)
        .join(kind.kind_dir())
        .join(filename)
}

/// task-waiting-user 事件 payload（前端 mirror）。
#[derive(serde::Serialize, Clone)]
struct WaitingUserPayload {
    pub task_id: String,
    pub file_name: String,
    pub error: String,
}

/// 批量文件下载任务：mod / 资源包 / 数据包 / 光影包。
/// 步骤 = 每文件一步；逐文件下载到 .elytra-part 临时文件后 rename，取消/失败清理临时文件。
/// 进度：文件计数 + 字节累计（当前文件增量），EMA 速度，复用 update_progress。
pub async fn run_files_task(
    app: AppHandle,
    cancel: &Arc<AtomicBool>,
    progress: &Arc<Mutex<TaskProgress>>,
    task: &DownloadFilesTask,
) {
    let config: DownloaderConfig = persist::read(constants::SECT_DOWNLOADER).unwrap_or_default();
    let mc = config.minecraft_dir_abs();
    let task_id = progress.lock().task_id.clone();

    let items: Vec<(std::path::PathBuf, std::path::PathBuf)> = task
        .files
        .iter()
        .map(|f| {
            let dest = file_dest(&mc, &task.version_dir_name, f.kind, &f.filename);
            let tmp = dest.with_file_name(format!("{}.elytra-part", f.filename));
            (dest, tmp)
        })
        .collect();

    // 全部路径互斥占用：任一被其他任务占用 → 整任务拒绝（可预期，不半途而废）
    let mut _locks = Vec::new();
    for (dest, _) in &items {
        match task::PathLock::try_acquire(task::global(), dest) {
            Some(l) => _locks.push(l),
            None => {
                return set_error(
                    &app,
                    progress,
                    format!("该文件正在被其他任务下载:\n{}", dest.display()),
                )
            }
        }
    }
    if cancelled(cancel) {
        return set_cancelled(&app, progress);
    }
    for (dest, _) in &items {
        if let Some(parent) = dest.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                return set_error(
                    &app,
                    progress,
                    format!("创建目录失败:\n{}\n{}", e, parent.display()),
                );
            }
        }
    }

    let done_files = Arc::new(AtomicU32::new(0));
    let acc_bytes = Arc::new(AtomicU64::new(0));
    let cur_bytes = Arc::new(AtomicU64::new(0));
    let cur_total = Arc::new(AtomicU64::new(0));
    let cur_idx = Arc::new(AtomicU32::new(0));
    let total_files = task.files.len() as u32;

    // 进度上报循环：150ms 心跳，速度按相邻采样差分 + EMA 平滑
    let emit_handle = tauri::async_runtime::spawn({
        let app = app.clone();
        let progress = progress.clone();
        let done_files = done_files.clone();
        let acc_bytes = acc_bytes.clone();
        let cur_bytes = cur_bytes.clone();
        let cur_total = cur_total.clone();
        let cur_idx = cur_idx.clone();
        async move {
            let mut tick = tokio::time::interval(Duration::from_millis(150));
            let mut last_bytes = 0u64;
            let mut last_sample = Instant::now();
            let mut speed_ema: f64 = 0.0;
            loop {
                tick.tick().await;
                let d = done_files.load(Ordering::Relaxed);
                let cb = cur_bytes.load(Ordering::Relaxed);
                let ct = cur_total.load(Ordering::Relaxed);
                let acc = acc_bytes.load(Ordering::Relaxed);
                let cur = acc + cb;
                let now = Instant::now();
                let dt = now.duration_since(last_sample).as_secs_f64();
                if dt >= 0.3 && cur >= last_bytes {
                    let instant = (cur - last_bytes) as f64 / dt;
                    speed_ema = if speed_ema <= 0.0 {
                        instant
                    } else {
                        speed_ema * 0.6 + instant * 0.4
                    };
                    last_bytes = cur;
                    last_sample = now;
                }
                task::update_and_emit(&app, &progress, |p| {
                    p.completed_files = d;
                    p.completed_bytes = cur;
                    p.total_bytes = acc + ct;
                    p.speed_bps = speed_ema;
                    if let Some(st) = p.steps.get_mut(cur_idx.load(Ordering::Relaxed) as usize) {
                        if st.status == StepStatus::Running {
                            st.progress = if ct > 0 {
                                (cb as f64 / ct as f64).min(1.0) * 100.0
                            } else {
                                0.0
                            };
                        }
                    }
                    update_progress(p);
                });
            }
        }
    });

    for (i, (dest, tmp)) in items.iter().enumerate() {
        cur_idx.store(i as u32, Ordering::Relaxed);
        let f = &task.files[i];
        set_phase(&app, progress, None, &format!("下载 {}", f.filename), i, StepStatus::Running);
        cur_bytes.store(0, Ordering::Relaxed);
        cur_total.store(0, Ordering::Relaxed);
        let cb = {
            let cur_bytes = cur_bytes.clone();
            let cur_total = cur_total.clone();
            move |downloaded: u64, total_size: u64| {
                cur_bytes.store(downloaded, Ordering::Relaxed);
                if total_size > 0 {
                    cur_total.store(total_size, Ordering::Relaxed);
                }
            }
        };
        // 单文件下载 + 失败重试循环：用户可决策"跳过/重试/取消"（WaitingForUser 暂停），
        // 不再一文件失败整任务失败——批量 mod 下载常见单文件源失效（404/镜像未同步）。
        let mut decided = WaitingDecision::Retry;
        while decided == WaitingDecision::Retry {
            let result = fetcher::fetch(&f.url, tmp, Some(&cb)).await;
            if cancelled(cancel) {
                let _ = std::fs::remove_file(tmp);
                emit_handle.abort();
                return set_cancelled(&app, progress);
            }
            match result {
                Ok(_) => decided = WaitingDecision::Skip, // 下载成功，跳出重试循环
                Err(e) => {
                    let _ = std::fs::remove_file(tmp);
                    // 暂停：发事件给前端弹窗，轮询用户决策（200ms，cancel 视为取消）
                    let slot = task::global().begin_wait(&task_id);
                    task::emit_event(
                        &app,
                        crate::events::TASK_WAITING_USER,
                        WaitingUserPayload {
                            task_id: task_id.clone(),
                            file_name: f.filename.clone(),
                            error: e.to_string(),
                        },
                    );
                    decided = loop {
                        if cancelled(cancel) {
                            task::global().end_wait(&task_id);
                            emit_handle.abort();
                            return set_cancelled(&app, progress);
                        }
                        if let Some(d) = task::global().waiting_decision(&task_id) {
                            task::global().end_wait(&task_id);
                            break d;
                        }
                        tokio::time::sleep(Duration::from_millis(200)).await;
                    };
                    drop(slot);
                }
            }
        }
        if decided == WaitingDecision::Cancel {
            emit_handle.abort();
            return set_cancelled(&app, progress);
        }
        // Skip 或成功：跳过该文件继续（用户明示跳过 → 该步视为完成）
        if let Err(e) = std::fs::rename(tmp, dest) {
            let _ = std::fs::remove_file(tmp);
            emit_handle.abort();
            return set_error(&app, progress, format!("写入 {} 失败:\n{}", f.filename, e));
        }
        acc_bytes.fetch_add(cur_bytes.load(Ordering::Relaxed), Ordering::Relaxed);
        cur_bytes.store(0, Ordering::Relaxed);
        done_files.fetch_add(1, Ordering::Relaxed);
        step_done(&app, progress, i);
        task::update_and_emit(&app, progress, |p| {
            p.current_phase = format!("已完成 {}/{}", i + 1, total_files);
        });
        if cancelled(cancel) {
            emit_handle.abort();
            return set_cancelled(&app, progress);
        }
    }
    emit_handle.abort();

    task::update_and_emit(&app, progress, |p| {
        p.status = TaskStatus::Done;
        p.progress = 100.0;
        p.completed_files = total_files;
        p.total_files = total_files;
        p.current_phase = "完成".into();
    });
    if let Some(p) = progress.try_lock() {
        task::global().remove(&p.task_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file_dest_builds_isolated_path() {
        let dest = file_dest(
            Path::new("C:/mc"),
            "26.2-fabric-0.19.3",
            DownloadFileKind::Mod,
            "fabric-api-15.0.0.jar",
        );
        assert_eq!(
            dest,
            Path::new("C:/mc/versions/26.2-fabric-0.19.3/mods/fabric-api-15.0.0.jar")
        );
    }
}
