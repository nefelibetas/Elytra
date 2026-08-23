// ── JDK 安装任务（下载缺失 Java 运行时）──
// 从 runner.rs 拆出：单步任务主体，与完整下载 runner 解耦。

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;

use parking_lot::Mutex;
use tauri::AppHandle;

use crate::config::downloader::model::DownloaderConfig;
use crate::constants;
use crate::persist;
use crate::task::progress::{
    cancelled, set_cancelled, set_error, set_phase, set_total, update_progress,
};
use crate::task::{self, StepStatus, TaskProgress, TaskStatus};

/// JDK 安装任务主体：官方 runtime → Azul 回退（jdk::download::auto_install），
/// 完成后注册进 jdk 配置（discovered_jdks）。单步任务，进度照 files.rs 心跳模式：
/// 原子计数器（文件数 + 字节增量）+ 150ms 心跳（EMA 速度/ETA），终态由
/// set_error/set_cancelled/收尾 done 独占发出，心跳随 status 离开 Running 自退出。
pub async fn run_install_jdk(
    app: AppHandle,
    cancel: &Arc<AtomicBool>,
    progress: &Arc<Mutex<TaskProgress>>,
    major: u32,
) {
    let config: DownloaderConfig = persist::read(constants::SECT_DOWNLOADER).unwrap_or_default();
    let dest_dir = crate::jdk::download::runtime_dir(&config, major);

    // 同目录互斥：另一任务正安装同 major 时拒绝（可预期，不半途而废）
    let _lock = match task::PathLock::try_acquire(task::global(), &dest_dir) {
        Some(l) => l,
        None => {
            return set_error(
                &app,
                progress,
                format!("Java {major} 正在被其他任务安装:\n{}", dest_dir.display()),
            )
        }
    };

    // 取消/失败时整删半截 runtime 目录：managed 目录程序专属（可重下），
    // 半截残留会误导 auto_install 的幂等检查（bin/java.exe 已存在即误判已装）
    if let Some(p) = progress.try_lock() {
        task::global().set_cleanup_path(&p.task_id, dest_dir.clone());
    }

    set_phase(&app, progress, Some(TaskStatus::Running), &format!("下载 Java {major}"), 0, StepStatus::Running);
    if cancelled(cancel) {
        return set_cancelled(&app, progress);
    }

    // 原子计数器（下载回调写入，心跳读取）
    let files_done = Arc::new(AtomicU32::new(0));
    let files_total = Arc::new(AtomicU32::new(0));
    let acc_bytes = Arc::new(AtomicU64::new(0));
    let cur_bytes = Arc::new(AtomicU64::new(0));
    let cur_total = Arc::new(AtomicU64::new(0));

    // 进度心跳：150ms，速度按相邻采样差分 + EMA 平滑；status 离开 Running 即退出。
    // 未换用 progress::spawn_heartbeat——语义有差异：
    // 1) 字节口径为 acc_bytes + cur_bytes（累计 + 当前文件）双计数器合成，
    //    spawn_heartbeat 只接受单一 bytes 计数器；
    // 2) 额外维护 steps[0] 的当前文件字节占比（步骤级进度），
    //    spawn_heartbeat 无步骤进度逻辑；
    // 3) 不覆写 total_bytes（由 on_total 钩子经 set_total 设置），也不写 p.status，
    //    spawn_heartbeat 每拍以 est_total_bytes 覆写总量并强置 Running。
    let emit_handle = tauri::async_runtime::spawn({
        let app = app.clone();
        let progress = progress.clone();
        let files_done = files_done.clone();
        let files_total = files_total.clone();
        let acc_bytes = acc_bytes.clone();
        let cur_bytes = cur_bytes.clone();
        let cur_total = cur_total.clone();
        async move {
            let mut tick = tokio::time::interval(std::time::Duration::from_millis(150));
            let mut last_bytes = 0u64;
            let mut last_sample = std::time::Instant::now();
            let mut last_progress_at = std::time::Instant::now();
            let mut speed_ema: f64 = 0.0;
            loop {
                tick.tick().await;
                // 终态（done/error/cancelled）由任务侧收口发出，心跳随之自退出
                if progress.lock().status != TaskStatus::Running {
                    return;
                }
                let fd = files_done.load(Ordering::Relaxed);
                let ft = files_total.load(Ordering::Relaxed);
                let cb = cur_bytes.load(Ordering::Relaxed);
                let ct = cur_total.load(Ordering::Relaxed);
                let cur = acc_bytes.load(Ordering::Relaxed) + cb;
                let now = std::time::Instant::now();
                if cur > last_bytes {
                    last_progress_at = now;
                }
                let dt = now.duration_since(last_sample).as_secs_f64();
                if dt >= 0.3 && cur > last_bytes {
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
                    p.speed_bps = speed_ema;
                    let remaining = p.total_bytes.saturating_sub(cur);
                    p.eta_seconds = if speed_ema >= 1024.0
                        && remaining > 0
                        && now.duration_since(last_progress_at) <= std::time::Duration::from_secs(4)
                    {
                        Some((remaining as f64 / speed_ema).ceil() as u64)
                    } else {
                        None
                    };
                    p.completed_files = fd;
                    p.total_files = ft;
                    p.completed_bytes = cur;
                    // 当前文件字节占比（单步任务的步骤进度）
                    if let Some(st) = p.steps.get_mut(0) {
                        if st.status == StepStatus::Running {
                            st.progress = if ct > 0 {
                                (cb as f64 / ct as f64).min(1.0) * 100.0
                            } else if ft > 0 {
                                (fd as f64 / ft as f64).min(1.0) * 100.0
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

    // 下载 + 安装（官方 runtime → Azul 回退；取消/失败由 auto_install 返回）
    let hook = crate::jdk::download::InstallProgress {
        cancel: Some(cancel),
        on_total: Some(&{
            let app = app.clone();
            let progress = progress.clone();
            let files_total = files_total.clone();
            move |files: u32, bytes: u64| {
                files_total.store(files, Ordering::Relaxed);
                set_total(&app, &progress, files, bytes);
            }
        }),
        on_bytes: Some(&{
            let acc = acc_bytes.clone();
            move |delta: u64| {
                acc.fetch_add(delta, Ordering::Relaxed);
            }
        }),
        on_file_done: Some(&{
            let fd = files_done.clone();
            move || {
                fd.fetch_add(1, Ordering::Relaxed);
            }
        }),
    };
    let result = crate::jdk::download::auto_install(major, &config, &hook).await;

    if cancelled(cancel) {
        emit_handle.abort();
        return set_cancelled(&app, progress);
    }
    match result {
        Ok(info) => {
            // 注册进 jdk 配置（discovered_jdks 去重），前端完成监听自动选中
            crate::jdk::register_scanned(vec![info]);
            emit_handle.abort();
            task::update_and_emit(&app, progress, |p| {
                p.status = TaskStatus::Done;
                p.progress = 100.0;
                p.completed_files = p.total_files;
                p.current_phase = "完成".into();
            });
            if let Some(p) = progress.try_lock() {
                let task_id = p.task_id.clone();
                drop(p);
                task::global().fail(&task_id);
            }
        }
        Err(e) => {
            emit_handle.abort();
            // 半截安装后台整删（managed runtime 可重下；同步删会阻塞 worker）
            let d = dest_dir.clone();
            tauri::async_runtime::spawn_blocking(move || {
                let _ = std::fs::remove_dir_all(&d);
            });
            set_error(&app, progress, e);
        }
    }
}
