// ── 批量文件下载任务（mod / 资源包 / 数据包 / 光影包垂直切片）──

use parking_lot::Mutex;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tauri::AppHandle;

use crate::config::downloader::DownloaderConfig;
use crate::downloader::fetcher;
use crate::downloader::path_lock::PathLock;
use crate::game::resource::extract::extract_zip_safe;
use crate::persist;
use crate::task::model::{DownloadFileItem, DownloadFileKind, DownloadFilesTask, TaskText};
use crate::task::{self, StepStatus, TaskProgress, TaskSpec, TaskStatus, WaitingDecision};

use super::progress::{
    cancelled, set_cancelled, set_error, set_phase, spawn_heartbeat, step_done, HeartbeatStats,
};
use super::{EventSink, TaskCtx};

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
#[derive(serde::Serialize, Clone, ts_rs::TS)]
#[ts(export_to = "task/")]
pub struct WaitingUserPayload {
    pub task_id: String,
    pub file_name: String,
    pub error: String,
}

/// 单文件下载 + 重试：失败静默重试（1s/2s 退避），连续 3 次失败挂起等用户决策（跳过/重试/取消）；普通 kind 与 worlds 共用，成功调 `on_success(tmp)` 落盘，失败/跳过清理 tmp；None = 任务已收口，Some(()) = 该文件已处理（跳过时 tmp 已清）。
async fn download_with_decision(
    ctx: &TaskCtx<'_>,
    filename: &str,
    url: &str,
    tmp: &Path,
    cb: &(dyn Fn(u64, u64) + Send + Sync),
    emit_handle: &tauri::async_runtime::JoinHandle<()>,
    on_success: impl FnOnce(&Path) -> Result<(), String>,
) -> Option<()> {
    let TaskCtx {
        app,
        cancel,
        progress,
        task_id,
    } = *ctx;
    let mut decided = WaitingDecision::Retry;
    let mut silent_retries = 0u32;
    let mut downloaded = false;
    while decided == WaitingDecision::Retry {
        // 传 cancel=None 由外部轮询中断：大文件中途取消能立即中断，不必等传输结束或 30s read_timeout；client 须显式绑定（临时值跨 await 借用检查不过）。
        let client = fetcher::http();
        let result = fetcher::fetch_with_c(
            client.as_ref(),
            url,
            tmp,
            Some(cb),
            None,
            Some(cancel.as_ref()),
        )
        .await;
        if cancelled(cancel) {
            let _ = std::fs::remove_file(tmp);
            emit_handle.abort();
            set_cancelled(app, progress);
            return None;
        }
        match result {
            Ok(_) => {
                downloaded = true;
                decided = WaitingDecision::Skip;
            }
            Err(e) => {
                let _ = std::fs::remove_file(tmp);
                if silent_retries < 2 {
                    silent_retries += 1;
                    tokio::time::sleep(Duration::from_millis(1000 * silent_retries as u64)).await;
                    continue;
                }
                // 暂停：发事件给前端弹窗，轮询用户决策（200ms，cancel 视为取消）
                let slot = task::global().begin_wait(task_id);
                task::emit_event(
                    app,
                    crate::events::TASK_WAITING_USER,
                    WaitingUserPayload {
                        task_id: task_id.to_string(),
                        file_name: filename.to_string(),
                        error: e.to_string(),
                    },
                );
                decided = loop {
                    if cancelled(cancel) {
                        task::global().end_wait(task_id);
                        emit_handle.abort();
                        set_cancelled(app, progress);
                        return None;
                    }
                    if let Some(d) = task::global().waiting_decision(task_id) {
                        task::global().end_wait(task_id);
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
        set_cancelled(app, progress);
        return None;
    }
    if !downloaded {
        // 用户跳过：tmp 已清理，不能再走 on_success（rename 必失败会把整批变成 Error）；按「已处理」返回。
        return Some(());
    }
    if let Err(e) = on_success(tmp) {
        let _ = std::fs::remove_file(tmp);
        emit_handle.abort();
        set_error(app, progress, format!("写入 {} 失败:\n{}", filename, e));
        return None;
    }
    Some(())
}

/// worlds 存档安装：下载 zip → 安全解压到临时区 → 定位世界根（level.dat）→ 移到 versions/{vdir}/saves/{name}（同名冲突 _{n} 后缀）→ 清理临时区；版本隔离落盘；None = 任务已收口，Some(()) = 安装完成。
async fn install_world(
    ctx: &TaskCtx<'_>,
    mc: &Path,
    version_dir: &str,
    f: &DownloadFileItem,
    cb: &(dyn Fn(u64, u64) + Send + Sync),
    emit_handle: &tauri::async_runtime::JoinHandle<()>,
) -> Option<()> {
    let TaskCtx {
        app,
        cancel,
        progress,
        ..
    } = *ctx;
    let stage = mc
        .join("versions")
        .join(version_dir)
        .join("saves")
        .join(".elytra-worlds");
    // 锁临时区，防并发 worlds 任务互相踩踏（全程独占）
    let _lock = match PathLock::try_acquire(&stage) {
        Some(l) => l,
        None => {
            emit_handle.abort();
            set_error(
                app,
                progress,
                format!("该世界正在被其他任务下载:\n{}", stage.display()),
            );
            return None;
        }
    };
    if let Err(e) = std::fs::create_dir_all(&stage) {
        emit_handle.abort();
        set_error(
            app,
            progress,
            format!("创建目录失败:\n{}\n{}", e, stage.display()),
        );
        return None;
    }
    let tmp = stage.join(format!("{}.elytra-part", f.filename));
    let zip_path = stage.join(&f.filename);
    let unpack = stage.join("unpack");
    let zip_for_rename = zip_path.clone();
    download_with_decision(ctx, &f.filename, &f.url, &tmp, cb, emit_handle, |tmp| {
        std::fs::rename(tmp, &zip_for_rename).map_err(|e| e.to_string())
    })
    .await?;
    // 解压（zip slip 防护 + 逐条目取消检查）
    if let Err(e) = extract_zip_safe(&zip_path, &unpack, cancel) {
        let _ = std::fs::remove_dir_all(&stage);
        emit_handle.abort();
        set_error(app, progress, e);
        return None;
    }
    if cancelled(cancel) {
        let _ = std::fs::remove_dir_all(&stage);
        emit_handle.abort();
        set_cancelled(app, progress);
        return None;
    }
    let Some((world_root, name)) = locate_world_root(&unpack, &f.filename) else {
        let _ = std::fs::remove_dir_all(&stage);
        emit_handle.abort();
        set_error(
            app,
            progress,
            format!("不是有效的地图存档（缺少 level.dat）\n{}", f.filename),
        );
        return None;
    };
    let saves = mc.join("versions").join(version_dir).join("saves");
    if let Err(e) = std::fs::create_dir_all(&saves) {
        let _ = std::fs::remove_dir_all(&stage);
        emit_handle.abort();
        set_error(
            app,
            progress,
            format!("创建目录失败:\n{}\n{}", e, saves.display()),
        );
        return None;
    }
    let target = saves.join(crate::util::unique_child_dir(&saves, &name));
    if let Err(e) = std::fs::rename(&world_root, &target) {
        let _ = std::fs::remove_dir_all(&stage);
        emit_handle.abort();
        set_error(app, progress, format!("安装世界 {} 失败:\n{}", name, e));
        return None;
    }
    // 清理临时区（走 spawn_blocking；失败残留由 cleanup_file_parts 兜底）
    let _ = tauri::async_runtime::spawn_blocking(move || std::fs::remove_dir_all(&stage)).await;
    Some(())
}

/// 遍历 unpack 找 level.dat（存档根标志）：根含 → (根, zip 名去 .zip 并清洗)；单层子目录含 → (子目录, 子目录名)；均无 → None。
fn locate_world_root(
    unpack_dir: &Path,
    zip_filename: &str,
) -> Option<(std::path::PathBuf, String)> {
    if unpack_dir.join("level.dat").is_file() {
        let name = crate::util::sanitize_fs_name(
            zip_filename.strip_suffix(".zip").unwrap_or(zip_filename),
            "world",
        );
        return Some((unpack_dir.to_path_buf(), name));
    }
    if let Ok(entries) = std::fs::read_dir(unpack_dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() && p.join("level.dat").is_file() {
                let name =
                    crate::util::sanitize_fs_name(&entry.file_name().to_string_lossy(), "world");
                return Some((p, name));
            }
        }
    }
    None
}

/// 批量文件下载：每文件一步；下载到 .elytra-part 后 rename，取消/失败清理临时文件；进度 = 文件计数 + 字节累计（当前文件增量），EMA 速度。
pub async fn run_files_task(
    sink: Arc<dyn EventSink>,
    cancel: &Arc<AtomicBool>,
    progress: &Arc<Mutex<TaskProgress>>,
    task: &DownloadFilesTask,
) {
    let app: &dyn EventSink = sink.as_ref();
    // 准备：配置/路径/路径锁/建目录；失败或已取消 → 内部收口后返回（`_locks` 存活至任务结束）。
    let Some(FilesPlan {
        mc,
        items,
        locks: _locks,
        task_id,
    }) = prepare_files(app, cancel, progress, task)
    else {
        return;
    };
    let counts = FilesCounters::new();
    let total_files = task.files.len() as u32;
    // 150ms 心跳刷新状态/文件数/字节/EMA 速度/ETA（停滞 >4s 或速度 <1KB/s 时 ETA 置 None）；步骤级进度由下载回调节流刷新，心跳不处理。
    let emit_handle = spawn_heartbeat(
        Arc::clone(&sink),
        progress,
        HeartbeatStats {
            done_files: Arc::clone(&counts.done_files),
            total_files,
            bytes: Arc::clone(&counts.total_bytes),
            est_total_bytes: Arc::clone(&counts.est_bytes),
            percent_by_steps: false,
            step: None,
        },
    );
    let ctx = FilesCtx {
        sink: &sink,
        cancel,
        progress,
        task,
        task_id: &task_id,
        mc: &mc,
        items: &items,
        counts: &counts,
    };
    if run_file_steps(&ctx, &emit_handle).await.is_none() {
        return;
    }
    finish_files(app, progress, total_files);
}

/// 批量文件下载的准备结果：游戏目录 + 目标/临时路径 + 路径锁（存活至任务结束）。
struct FilesPlan {
    mc: std::path::PathBuf,
    items: Vec<(std::path::PathBuf, std::path::PathBuf)>,
    locks: Vec<PathLock>,
    task_id: String,
}

/// 批量文件任务的共享计数器：心跳读取 + 下载回调写入。
struct FilesCounters {
    done_files: Arc<AtomicU32>,
    // 心跳字节源 = 已完成累计 + 当前文件已下载（回调维护），速度平滑连续
    total_bytes: Arc<AtomicU64>,
    // 总量估算 = 已完成累计 + 当前文件声明大小（回调随时更新）
    est_bytes: Arc<AtomicU64>,
    cur_bytes: Arc<AtomicU64>,
    cur_total: Arc<AtomicU64>,
    cur_idx: Arc<AtomicU32>,
}

impl FilesCounters {
    fn new() -> Self {
        Self {
            done_files: Arc::new(AtomicU32::new(0)),
            total_bytes: Arc::new(AtomicU64::new(0)),
            est_bytes: Arc::new(AtomicU64::new(0)),
            cur_bytes: Arc::new(AtomicU64::new(0)),
            cur_total: Arc::new(AtomicU64::new(0)),
            cur_idx: Arc::new(AtomicU32::new(0)),
        }
    }
}

#[derive(Clone, Copy)]
struct FilesCtx<'a> {
    sink: &'a Arc<dyn EventSink>,
    cancel: &'a Arc<AtomicBool>,
    progress: &'a Arc<Mutex<TaskProgress>>,
    task: &'a DownloadFilesTask,
    task_id: &'a str,
    mc: &'a Path,
    items: &'a [(std::path::PathBuf, std::path::PathBuf)],
    counts: &'a FilesCounters,
}

/// 准备：读配置 → 算目标/临时路径 → 全部路径互斥占用 → 取消检查 → 建目录；None = 已收口。
fn prepare_files(
    app: &dyn EventSink,
    cancel: &Arc<AtomicBool>,
    progress: &Arc<Mutex<TaskProgress>>,
    task: &DownloadFilesTask,
) -> Option<FilesPlan> {
    let config: DownloaderConfig = persist::downloader_config();
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

    // 任一目标被占用 → 整任务拒绝（不半途而废）
    let mut locks = Vec::new();
    for (dest, _) in &items {
        match PathLock::try_acquire(dest) {
            Some(l) => locks.push(l),
            None => {
                set_error(
                    app,
                    progress,
                    format!("该文件正在被其他任务下载:\n{}", dest.display()),
                );
                return None;
            }
        }
    }
    if cancelled(cancel) {
        set_cancelled(app, progress);
        return None;
    }
    for (dest, _) in &items {
        if let Some(parent) = dest.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                set_error(
                    app,
                    progress,
                    format!("创建目录失败:\n{}\n{}", e, parent.display()),
                );
                return None;
            }
        }
    }
    Some(FilesPlan {
        mc,
        items,
        locks,
        task_id,
    })
}

/// 当前文件下载回调：维护字节计数/总量估算；步骤级进度节流刷新（~150ms）。
fn file_progress_callback(
    sink: &Arc<dyn EventSink>,
    progress: &Arc<Mutex<TaskProgress>>,
    counts: &FilesCounters,
    acc_snap: u64,
) -> impl Fn(u64, u64) + Send + Sync {
    let app = Arc::clone(sink);
    let progress = Arc::clone(progress);
    let cur_bytes = Arc::clone(&counts.cur_bytes);
    let cur_total = Arc::clone(&counts.cur_total);
    let est_bytes = Arc::clone(&counts.est_bytes);
    let total_bytes = Arc::clone(&counts.total_bytes);
    let cur_idx = Arc::clone(&counts.cur_idx);
    let throttle_ms = Arc::new(AtomicU64::new(0));
    let clock = std::time::Instant::now();
    move |downloaded: u64, total_size: u64| {
        cur_bytes.store(downloaded, Ordering::Relaxed);
        total_bytes.store(acc_snap + downloaded, Ordering::Relaxed);
        if total_size > 0 {
            cur_total.store(total_size, Ordering::Relaxed);
            est_bytes.store(acc_snap + total_size, Ordering::Relaxed);
        }
        let now_ms = clock.elapsed().as_millis() as u64;
        if now_ms.saturating_sub(throttle_ms.swap(now_ms, Ordering::Relaxed)) >= 150 {
            task::update_and_emit(app.as_ref(), &progress, |p| {
                let cb_b = cur_bytes.load(Ordering::Relaxed);
                let ct = cur_total.load(Ordering::Relaxed);
                if let Some(st) = p.steps.get_mut(cur_idx.load(Ordering::Relaxed) as usize) {
                    if st.status == StepStatus::Running {
                        st.progress = if ct > 0 {
                            (cb_b as f64 / ct as f64).min(1.0) * 100.0
                        } else {
                            0.0
                        };
                    }
                }
            });
        }
    }
}

/// 逐文件下载/落盘 + 步骤收口，结束 abort 心跳；None = 已收口，Some(()) = 全部文件完成。
async fn run_file_steps(
    ctx: &FilesCtx<'_>,
    emit_handle: &tauri::async_runtime::JoinHandle<()>,
) -> Option<()> {
    let FilesCtx {
        sink,
        cancel,
        progress,
        task,
        task_id,
        mc,
        items,
        counts,
    } = *ctx;
    let app: &dyn EventSink = sink.as_ref();
    let total_files = task.files.len() as u32;
    for (i, (dest, tmp)) in items.iter().enumerate() {
        counts.cur_idx.store(i as u32, Ordering::Relaxed);
        let f = &task.files[i];
        set_phase(
            app,
            progress,
            None,
            &format!("下载 {}", f.filename),
            i,
            StepStatus::Running,
        );
        counts.cur_bytes.store(0, Ordering::Relaxed);
        let acc_snap = counts.total_bytes.load(Ordering::Relaxed);
        let cb = file_progress_callback(sink, progress, counts, acc_snap);
        let file_ctx = TaskCtx {
            app,
            cancel,
            progress,
            task_id,
        };
        let placed = if f.kind == DownloadFileKind::World {
            install_world(&file_ctx, mc, &task.version_dir_name, f, &cb, emit_handle).await
        } else {
            download_with_decision(
                &file_ctx,
                &f.filename,
                &f.url,
                tmp,
                &cb,
                emit_handle,
                |tmp| std::fs::rename(tmp, dest).map_err(|e| e.to_string()),
            )
            .await
        };
        placed?;
        counts.total_bytes.store(
            acc_snap + counts.cur_bytes.load(Ordering::Relaxed),
            Ordering::Relaxed,
        );
        counts.cur_bytes.store(0, Ordering::Relaxed);
        counts.cur_total.store(0, Ordering::Relaxed);
        step_done(app, progress, i);
        task::update_and_emit(app, progress, |p| {
            p.current_phase = format!("已完成 {}/{}", i + 1, total_files);
            p.current_phase_key = Some("task.backend.phase.files_progress".into());
            p.current_phase_params = Some(std::collections::BTreeMap::from([
                ("done".to_string(), (i + 1).to_string()),
                ("total".to_string(), total_files.to_string()),
            ]));
        });
        if cancelled(cancel) {
            emit_handle.abort();
            set_cancelled(app, progress);
            return None;
        }
    }
    emit_handle.abort();
    Some(())
}

/// 收尾阶段：Done 终态 + 100% 进度 + 移除任务注册（先释放 progress 锁再 remove）。
fn finish_files(app: &dyn EventSink, progress: &Arc<Mutex<TaskProgress>>, total_files: u32) {
    task::update_and_emit(app, progress, |p| {
        p.status = TaskStatus::Done;
        p.progress = 100.0;
        p.completed_files = total_files;
        p.total_files = total_files;
        p.current_phase = "完成".into();
        p.current_phase_key = Some("task.backend.phase.done".into());
        p.current_phase_params = None;
    });
    if let Some(p) = progress.try_lock() {
        let task_id = p.task_id.clone();
        drop(p); // 先释放 progress 锁再 remove（remove 内部读 progress，防死锁）
        task::global().remove(&task_id);
    }
}

pub(crate) fn spawn_files_task(app: AppHandle, task: DownloadFilesTask) -> Result<String, String> {
    // version_dir_name 是已安装版本的实际目录名（含冲突追加的 _{n}），校验目录存在即可。
    let config: DownloaderConfig = persist::downloader_config();
    let dir = config
        .minecraft_dir_abs()
        .join("versions")
        .join(&task.version_dir_name);
    if !dir.is_dir() {
        bail_key!("errors.version.missing", 
            "目标版本目录不存在（请先在版本选择页选择已安装的版本）\n版本目录: {}",
            dir.display()
        );
    }
    task.validate()?;
    super::spawn_task(
        app,
        task.name(),
        TaskText::new(
            "task.backend.name.files",
            &[("count", task.files.len().to_string())],
        ),
        task.steps(),
        task::store::JobSpec::Files(task.clone()),
        move |a, cancel, progress| {
            tauri::async_runtime::spawn(async move {
                run_files_task(a, &cancel, &progress, &task).await;
            });
        },
    )
}

/// 删除任务的全部 .elytra-part 残留（恢复社区资源下载前调用）；worlds 清理 `saves/.elytra-worlds/` 临时区（含半截 zip/unpack）。
pub(crate) fn cleanup_file_parts(task: &DownloadFilesTask) {
    let config: DownloaderConfig = persist::downloader_config();
    let mc = config.minecraft_dir_abs();
    for f in &task.files {
        if f.kind == DownloadFileKind::World {
            let stage = mc
                .join("versions")
                .join(&task.version_dir_name)
                .join("saves")
                .join(".elytra-worlds");
            let _ = std::fs::remove_dir_all(&stage);
            continue;
        }
        let dest = mc
            .join("versions")
            .join(&task.version_dir_name)
            .join(f.kind.kind_dir())
            .join(&f.filename);
        let tmp = dest.with_file_name(format!("{}.elytra-part", f.filename));
        let _ = std::fs::remove_file(tmp);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file_dest_builds_isolated_path() {
        let dest = file_dest(
            Path::new("mc"),
            "26.2-fabric-0.19.3",
            DownloadFileKind::Mod,
            "fabric-api-15.0.0.jar",
        );
        assert_eq!(
            dest,
            Path::new("mc/versions/26.2-fabric-0.19.3/mods/fabric-api-15.0.0.jar")
        );
        let dest = file_dest(Path::new("mc"), "26.2", DownloadFileKind::World, "map.zip");
        assert_eq!(dest, Path::new("mc/versions/26.2/saves/map.zip"));
    }

    #[test]
    fn locate_world_root_detects_level_dat() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("u1")).unwrap();
        std::fs::write(tmp.path().join("u1/level.dat"), b"x").unwrap();
        let (root, name) = locate_world_root(&tmp.path().join("u1"), "oneblock.zip").unwrap();
        assert_eq!(root, tmp.path().join("u1"), "根即世界根");
        assert_eq!(name, "oneblock", "zip 文件名去 .zip 作为世界名");

        std::fs::create_dir_all(tmp.path().join("u2/My_World")).unwrap();
        std::fs::write(tmp.path().join("u2/My_World/level.dat"), b"x").unwrap();
        let (root, name) = locate_world_root(&tmp.path().join("u2"), "map.zip").unwrap();
        assert_eq!(root, tmp.path().join("u2/My_World"), "子目录即世界根");
        assert_eq!(
            name, "My_World",
            "子目录名作为世界名（清洗逻辑经 util::sanitize_fs_name，util 单测覆盖）"
        );

        std::fs::create_dir_all(tmp.path().join("u3/nested")).unwrap();
        std::fs::write(tmp.path().join("u3/readme.txt"), b"x").unwrap();
        assert!(locate_world_root(&tmp.path().join("u3"), "map.zip").is_none());
    }

    #[test]
    fn save_dir_conflict_resolution_suffixes() {
        let tmp = tempfile::tempdir().unwrap();
        let saves = tmp.path().join("saves");
        std::fs::create_dir_all(&saves).unwrap();
        assert_eq!(
            saves.join(crate::util::unique_child_dir(&saves, "bar")),
            saves.join("bar"),
            "无冲突原名"
        );
        std::fs::create_dir_all(saves.join("foo")).unwrap();
        std::fs::create_dir_all(saves.join("foo_1")).unwrap();
        assert_eq!(
            saves.join(crate::util::unique_child_dir(&saves, "foo")),
            saves.join("foo_2"),
            "冲突递增到空位"
        );
    }

    #[test]
    fn world_name_sanitizes_via_util() {
        let clean = |s: &str| crate::util::sanitize_fs_name(s, "world");
        assert_eq!(clean("My:World?"), "My_World_");
        assert_eq!(clean("oneblock"), "oneblock", "合法名原样");
        assert_eq!(clean(""), "world", "空名回退");
        assert_eq!(clean("world."), "world", "Windows 目录名不能以点结尾");
        assert_eq!(clean("  "), "world", "全空白回退");
        assert_eq!(clean("CON"), "world", "Windows 保留名回退（新增语义）");
    }
}
