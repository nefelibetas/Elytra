// ── JDK 安装任务（下载缺失 Java 运行时）── 单步任务主体。

use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;

use parking_lot::Mutex;

use crate::config::downloader::DownloaderConfig;
use crate::downloader::path_lock::PathLock;
use crate::jdk::download::{auto_install, runtime_dir, InstallProgress};
use crate::jdk::model::JdkInfo;
use crate::persist;
use crate::task::progress::{
    cancelled, set_cancelled, set_error, set_phase, set_total, update_progress,
};
use crate::task::{self, EventSink, StepStatus, TaskProgress, TaskStatus};

/// JDK 安装主体：按 jdk_source_mode 三态回退链（auto_install：官方 → Adoptium[TUNA 镜像] → Azul），完成后注册进 jdk 扫描缓存。
/// 单步任务，进度照 files.rs 心跳模式：原子计数器 + 150ms 心跳（EMA 速度/ETA）；终态由 set_error/set_cancelled/收尾 done 独占发出，心跳随 status 离开 Running 自退出。
pub async fn run_install_jdk(
    sink: Arc<dyn EventSink>,
    cancel: &Arc<AtomicBool>,
    progress: &Arc<Mutex<TaskProgress>>,
    major: u32,
) {
    let config: DownloaderConfig = persist::downloader_config();
    let dest_dir = runtime_dir(&config, major);

    // 准备阶段：目录互斥 + 清理登记 + Running 相位（锁冲突/已取消时自行收口）
    let Some(_lock) = begin_jdk_install(sink.as_ref(), progress, cancel, &dest_dir, major) else {
        return;
    };

    // 原子计数器（回调写入，心跳读取）
    let counts = JdkCounters::new();

    let emit_handle = spawn_jdk_heartbeat(sink.clone(), progress.clone(), &counts);

    // 下载 + 安装（按 jdk_source_mode 回退链；取消/失败由 auto_install 返回）
    let hook = InstallProgress {
        cancel: Some(cancel),
        on_attempt: Some(&{
            let app = sink.clone();
            let progress = progress.clone();
            let counts = counts.clone();
            move || {
                // 换源重试：总量口径变了，先把上一轮的计数与分母清掉
                counts.reset();
                set_total(app.as_ref(), &progress, 0, 0);
            }
        }),
        on_total: Some(&{
            let app = sink.clone();
            let progress = progress.clone();
            let files_total = counts.files_total.clone();
            move |files: u32, bytes: u64| {
                files_total.store(files, Ordering::Relaxed);
                set_total(app.as_ref(), &progress, files, bytes);
            }
        }),
        on_bytes: Some(&{
            let acc = counts.acc_bytes.clone();
            let pkg_total = counts.pkg_total.clone();
            move |delta: u64, total: u64| {
                acc.fetch_add(delta, Ordering::Relaxed);
                if total > 0 {
                    pkg_total.store(total, Ordering::Relaxed);
                }
            }
        }),
        on_file_done: Some(&{
            let fd = counts.files_done.clone();
            move || {
                fd.fetch_add(1, Ordering::Relaxed);
            }
        }),
    };
    let result = auto_install(major, &config, &hook).await;

    settle_jdk_install(
        sink.as_ref(),
        progress,
        cancel,
        &dest_dir,
        emit_handle,
        result,
    );
}

/// 同目录互斥 + 半截目录清理登记 + Running 相位；锁冲突或已取消时自行收口返回 None，返回的锁由调用方持有至任务结束。
fn begin_jdk_install(
    app: &dyn EventSink,
    progress: &Arc<Mutex<TaskProgress>>,
    cancel: &Arc<AtomicBool>,
    dest_dir: &Path,
    major: u32,
) -> Option<PathLock> {
    // 同目录互斥：另一任务正安装同 major 时拒绝
    let lock = match PathLock::try_acquire(dest_dir) {
        Some(l) => l,
        None => {
            set_error(
                app,
                progress,
                format!("Java {major} 正在被其他任务安装:\n{}", dest_dir.display()),
            );
            return None;
        }
    };

    // 取消/失败时整删半截 runtime 目录：半截残留会误导 auto_install 幂等检查（bin/java.exe 存在即误判已装）。先取 task_id
    // 释放 progress 守卫再调 set_cleanup_path（后者取 inner 锁），否则构成 progress→inner 反向锁序（与 fail/list 的 inner→progress 成 ABBA）。
    let cleanup_task_id = progress.try_lock().map(|p| p.task_id.clone());
    if let Some(task_id) = cleanup_task_id {
        task::global().set_cleanup_path(&task_id, dest_dir.to_path_buf());
    }

    set_phase(
        app,
        progress,
        Some(TaskStatus::Running),
        &format!("下载 Java {major}"),
        0,
        StepStatus::Running,
    );
    if cancelled(cancel) {
        set_cancelled(app, progress);
        return None;
    }
    Some(lock)
}

/// 原子计数器束（下载回调写入，心跳读取）；`Clone` 供回调闭包共用同一份计数。
#[derive(Clone)]
struct JdkCounters {
    files_done: Arc<AtomicU32>,
    files_total: Arc<AtomicU32>,
    /// 本次源尝试已下载字节（进度分子与速度）
    acc_bytes: Arc<AtomicU64>,
    /// 整包安装的传输总长（响应 Content-Length；清单没给总量时的进度分母兜底）
    pkg_total: Arc<AtomicU64>,
}

impl JdkCounters {
    fn new() -> Self {
        Self {
            files_done: Arc::new(AtomicU32::new(0)),
            files_total: Arc::new(AtomicU32::new(0)),
            acc_bytes: Arc::new(AtomicU64::new(0)),
            pkg_total: Arc::new(AtomicU64::new(0)),
        }
    }

    fn reset(&self) {
        self.files_done.store(0, Ordering::Relaxed);
        self.files_total.store(0, Ordering::Relaxed);
        self.acc_bytes.store(0, Ordering::Relaxed);
        self.pkg_total.store(0, Ordering::Relaxed);
    }
}

/// 进度心跳（150ms，相邻采样差分 + EMA 平滑；status 离开 Running 即退出）。与 progress::spawn_heartbeat 语义不同故未复用：
/// 计数来自本模块的双计数器（清单文件数/字节 + 整包传输总长），百分比与步骤进度统一交给 `update_progress`（同口径）。
fn spawn_jdk_heartbeat(
    sink: Arc<dyn EventSink>,
    progress: Arc<Mutex<TaskProgress>>,
    counts: &JdkCounters,
) -> tauri::async_runtime::JoinHandle<()> {
    let files_done = counts.files_done.clone();
    let files_total = counts.files_total.clone();
    let acc_bytes = counts.acc_bytes.clone();
    let pkg_total = counts.pkg_total.clone();
    tauri::async_runtime::spawn(async move {
        let mut tick = tokio::time::interval(std::time::Duration::from_millis(150));
        let mut sampler = task::progress::SpeedSampler::new();
        loop {
            tick.tick().await;
            // 终态（done/error/cancelled）由任务侧收口发出，心跳随之自退出
            if progress.lock().status != TaskStatus::Running {
                return;
            }
            let fd = files_done.load(Ordering::Relaxed);
            let ft = files_total.load(Ordering::Relaxed);
            let cur = acc_bytes.load(Ordering::Relaxed);
            let pkg = pkg_total.load(Ordering::Relaxed);
            let manifest_total = progress.lock().total_bytes;
            let eta_total = if manifest_total > 0 {
                manifest_total
            } else {
                pkg
            };
            let (speed_bps, eta) = sampler.sample(cur, eta_total);
            task::update_and_emit(sink.as_ref(), &progress, |p| {
                p.speed_bps = speed_bps;
                p.eta_seconds = eta;
                p.completed_files = fd;
                p.total_files = ft;
                p.completed_bytes = cur;
                // 整包安装（Adoptium/Azul）没有清单总量：用传输总长兜底，否则总/步骤进度都会停在 0
                if p.total_bytes == 0 && ft == 0 && pkg > 0 {
                    p.total_bytes = pkg;
                }
                update_progress(p);
            });
        }
    })
}

/// 收尾：取消优先；成功注册进 jdk 配置并置 done，失败后台整删半截目录并置 error。
fn settle_jdk_install(
    app: &dyn EventSink,
    progress: &Arc<Mutex<TaskProgress>>,
    cancel: &Arc<AtomicBool>,
    dest_dir: &Path,
    emit_handle: tauri::async_runtime::JoinHandle<()>,
    result: Result<JdkInfo, String>,
) {
    if cancelled(cancel) {
        emit_handle.abort();
        set_cancelled(app, progress);
        return;
    }
    match result {
        Ok(info) => {
            // 注册进 jdk 扫描缓存（去重），前端完成监听自动选中
            crate::jdk::register_scanned(vec![info]);
            emit_handle.abort();
            task::update_and_emit(app, progress, |p| {
                p.status = TaskStatus::Done;
                p.progress = 100.0;
                p.completed_files = p.total_files;
                if let Some(st) = p.steps.first_mut() {
                    st.status = StepStatus::Done;
                    st.progress = 100.0;
                }
                p.current_phase = "完成".into();
                p.current_phase_key = Some("task.backend.phase.done".into());
                p.current_phase_params = None;
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
            let d = dest_dir.to_path_buf();
            tauri::async_runtime::spawn_blocking(move || {
                let _ = std::fs::remove_dir_all(&d);
            });
            set_error(app, progress, e);
        }
    }
}
