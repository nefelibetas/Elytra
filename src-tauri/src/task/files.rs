// ── 批量文件下载任务（mod / 资源包 / 数据包 / 光影包垂直切片）──

use parking_lot::Mutex;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use tauri::AppHandle;

use crate::config::downloader::model::DownloaderConfig;
use crate::constants;
use crate::downloader::fetcher;
use crate::persist;
use crate::task::model::{DownloadFileItem, DownloadFileKind, DownloadFilesTask};
use crate::task::{self, StepStatus, TaskProgress, TaskStatus, WaitingDecision};

use super::progress::{cancelled, set_cancelled, set_error, set_phase, step_done, update_progress};

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

/// 单文件下载 + 失败重试循环：用户可决策"跳过/重试/取消"（WaitingForUser 暂停），
/// 不再一文件失败整任务失败——批量下载常见单文件源失效（404/镜像未同步）。
/// 首次失败静默自动重试（1s/2s 退避）：重启/切代理后的瞬时网络抖动自动消化，不弹窗打扰；
/// 连续 3 次失败才挂起等用户决策。普通 kind 与 worlds 共用（行为一致）。
/// 下载成功调用 `on_success(tmp)` 落盘/安装；失败清理 tmp。
/// 返回 None = 任务已结束（内部已 set_error/set_cancelled）；Some(()) = 该文件完成
/// （用户明示跳过时 tmp 已被清理，on_success 落盘会失败——与既有流程报错语义一致）。
#[allow(clippy::too_many_arguments)] // 上下文参数（app/cancel/progress/task_id）对齐 search_community 先例
async fn download_with_decision(
    app: &AppHandle,
    cancel: &Arc<AtomicBool>,
    progress: &Arc<Mutex<TaskProgress>>,
    task_id: &str,
    filename: &str,
    url: &str,
    tmp: &Path,
    cb: &(dyn Fn(u64, u64) + Send + Sync),
    emit_handle: &tauri::async_runtime::JoinHandle<()>,
    on_success: impl FnOnce(&Path) -> Result<(), String>,
) -> Option<()> {
    let mut decided = WaitingDecision::Retry;
    let mut silent_retries = 0u32;
    while decided == WaitingDecision::Retry {
        let result = fetcher::fetch(url, tmp, Some(cb)).await;
        if cancelled(cancel) {
            let _ = std::fs::remove_file(tmp);
            emit_handle.abort();
            set_cancelled(app, progress);
            return None;
        }
        match result {
            Ok(_) => decided = WaitingDecision::Skip, // 下载成功，跳出重试循环
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
    // Skip 或成功：用户明示跳过时 tmp 已被清理，落盘会失败（与既有流程一致的报错语义）
    if let Err(e) = on_success(tmp) {
        let _ = std::fs::remove_file(tmp);
        emit_handle.abort();
        set_error(app, progress, format!("写入 {} 失败:\n{}", filename, e));
        return None;
    }
    Some(())
}

/// worlds 地图存档安装：下载 zip → 安全解压到临时区 → 定位世界根（level.dat）→
/// 移动至 versions/{vdir}/saves/{name}（同名冲突 _{n} 后缀）→ 清理临时区。
/// 版本隔离落盘：MC 启动时版本目录即游戏目录，saves 直接可读。
/// 返回 None = 任务已结束（内部已 set_error/set_cancelled）；Some(()) = 安装完成。
#[allow(clippy::too_many_arguments)] // 上下文参数（app/cancel/progress/task_id）对齐 search_community 先例
async fn install_world(
    app: &AppHandle,
    cancel: &Arc<AtomicBool>,
    progress: &Arc<Mutex<TaskProgress>>,
    task_id: &str,
    mc: &Path,
    version_dir: &str,
    f: &DownloadFileItem,
    cb: &(dyn Fn(u64, u64) + Send + Sync),
    emit_handle: &tauri::async_runtime::JoinHandle<()>,
) -> Option<()> {
    let stage = mc
        .join("versions")
        .join(version_dir)
        .join("saves")
        .join(".elytra-worlds");
    // 锁临时区目录：防并发 worlds 任务互相踩踏（下载/解压/移动全程独占）
    let _lock = match task::PathLock::try_acquire(task::global(), &stage) {
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
    // 下载（含静默重试/WaitingForUser，与普通 kind 一致），成功后 rename 到临时区 zip
    let zip_for_rename = zip_path.clone();
    download_with_decision(
        app,
        cancel,
        progress,
        task_id,
        &f.filename,
        &f.url,
        &tmp,
        cb,
        emit_handle,
        |tmp| std::fs::rename(tmp, &zip_for_rename).map_err(|e| e.to_string()),
    )
    .await?;
    // 解压（zip slip 防护 + 逐条目取消检查）
    if let Err(e) = crate::downloader::extract::extract_zip_safe(&zip_path, &unpack, cancel) {
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
    // 定位世界根（level.dat）
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
    let target = resolve_save_dir(&saves, &name);
    if let Err(e) = std::fs::rename(&world_root, &target) {
        let _ = std::fs::remove_dir_all(&stage);
        emit_handle.abort();
        set_error(app, progress, format!("安装世界 {} 失败:\n{}", name, e));
        return None;
    }
    // 清理临时区（重活走 spawn_blocking；失败残留由 cleanup_file_parts 兜底）
    let _ = tauri::async_runtime::spawn_blocking(move || std::fs::remove_dir_all(&stage)).await;
    Some(())
}

/// 遍历 unpack 目录找 level.dat（地图存档根标志）：根含 → (根, zip 文件名去 .zip 并清洗)；
/// 单层子目录含 → (子目录, 子目录名)；均无 → None。
fn locate_world_root(
    unpack_dir: &Path,
    zip_filename: &str,
) -> Option<(std::path::PathBuf, String)> {
    if unpack_dir.join("level.dat").is_file() {
        let name = sanitize_world_name(zip_filename.strip_suffix(".zip").unwrap_or(zip_filename));
        return Some((unpack_dir.to_path_buf(), name));
    }
    if let Ok(entries) = std::fs::read_dir(unpack_dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() && p.join("level.dat").is_file() {
                let name = sanitize_world_name(&entry.file_name().to_string_lossy());
                return Some((p, name));
            }
        }
    }
    None
}

/// 清洗世界目录名：非法文件名字符（`<>:"/\|?*` 与控制字符）替换为 `_`；
/// 空结果（或 Windows 上以点结尾/全空白）回退用 `world`。
fn sanitize_world_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if c.is_control() || matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*') {
                '_'
            } else {
                c
            }
        })
        .collect();
    let cleaned = cleaned.trim_end_matches('.').trim().to_string();
    if cleaned.is_empty() {
        "world".to_string()
    } else {
        cleaned
    }
}

/// 冲突解决：目标存在则 `name_{n}`（n 从 1 递增，对齐版本目录 `{id}_{n}` 语义）。
fn resolve_save_dir(saves_dir: &Path, name: &str) -> std::path::PathBuf {
    let candidate = saves_dir.join(name);
    if !candidate.exists() {
        return candidate;
    }
    let mut n = 1;
    loop {
        let next = saves_dir.join(format!("{name}_{n}"));
        if !next.exists() {
            return next;
        }
        n += 1;
    }
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

    // 进度上报循环：150ms 心跳，速度按相邻采样差分 + EMA 平滑（对齐 EmitLoop：
    // 有实际字节推进才刷新停滞时钟；cur 相等时保持上次速度不衰减归零）；
    // ETA 同步计算（停滞 >4s 或速度 <1KB/s 置 None，前端显示 —）。
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
            let mut last_progress_at = Instant::now();
            let mut speed_ema: f64 = 0.0;
            loop {
                tick.tick().await;
                let d = done_files.load(Ordering::Relaxed);
                let cb = cur_bytes.load(Ordering::Relaxed);
                let ct = cur_total.load(Ordering::Relaxed);
                let acc = acc_bytes.load(Ordering::Relaxed);
                let cur = acc + cb;
                let now = Instant::now();
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
                    // 首次心跳即进入 running：create 初始为 pending，若不置位前端
                    // 任务中心的聚合统计（速度/ETA/剩余文件）只统计 running 任务而恒为 0
                    p.status = TaskStatus::Running;
                    p.completed_files = d;
                    // 最终总数每 tick 写回：初始为 0，不写则前端"剩余文件"恒为 0
                    p.total_files = total_files;
                    p.completed_bytes = cur;
                    p.total_bytes = acc + ct;
                    p.speed_bps = speed_ema;
                    // ETA：剩余字节 / 速度；停滞（>4s 无推进）或速度过低（<1KB/s）时置 None
                    let remaining = p.total_bytes.saturating_sub(cur);
                    p.eta_seconds = if speed_ema >= 1024.0
                        && remaining > 0
                        && now.duration_since(last_progress_at) <= Duration::from_secs(4)
                    {
                        Some((remaining as f64 / speed_ema).ceil() as u64)
                    } else {
                        None
                    };
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
        set_phase(
            &app,
            progress,
            None,
            &format!("下载 {}", f.filename),
            i,
            StepStatus::Running,
        );
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
        // 下载 + 落盘：普通 kind 下载到 .elytra-part 后 rename 到 kind 目录；
        // world 走 install_world（临时区下载 → 解压 → 移动至 saves/）。
        // 失败重试/WaitingForUser 语义两种 kind 一致（download_with_decision 提取）。
        // None = 任务已结束（内部已 set_error/set_cancelled），直接返回。
        let placed = if f.kind == DownloadFileKind::World {
            install_world(
                &app,
                cancel,
                progress,
                &task_id,
                &mc,
                &task.version_dir_name,
                f,
                &cb,
                &emit_handle,
            )
            .await
        } else {
            download_with_decision(
                &app,
                cancel,
                progress,
                &task_id,
                &f.filename,
                &f.url,
                tmp,
                &cb,
                &emit_handle,
                |tmp| std::fs::rename(tmp, dest).map_err(|e| e.to_string()),
            )
            .await
        };
        if placed.is_none() {
            return;
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
        let task_id = p.task_id.clone();
        drop(p); // 先释放 progress 锁再 remove（remove 内部读 progress，防重入死锁）
        task::global().remove(&task_id);
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
        // World 语义兜底：saves 目录（实际落盘走 install_world 特殊分支）
        let dest = file_dest(
            Path::new("C:/mc"),
            "26.2",
            DownloadFileKind::World,
            "map.zip",
        );
        assert_eq!(dest, Path::new("C:/mc/versions/26.2/saves/map.zip"));
    }

    /// locate_world_root：根含 level.dat → (根, zip 名清洗)；单层子目录含 → (子目录, 子目录名)；
    /// 无 level.dat → None。
    #[test]
    fn locate_world_root_detects_level_dat() {
        let tmp = tempfile::tempdir().unwrap();
        // 根含 level.dat（zip 顶层直接是存档文件）
        std::fs::create_dir_all(tmp.path().join("u1")).unwrap();
        std::fs::write(tmp.path().join("u1/level.dat"), b"x").unwrap();
        let (root, name) = locate_world_root(&tmp.path().join("u1"), "oneblock.zip").unwrap();
        assert_eq!(root, tmp.path().join("u1"), "根即世界根");
        assert_eq!(name, "oneblock", "zip 文件名去 .zip 作为世界名");

        // 单层子目录含 level.dat（zip 顶层是存档文件夹）
        std::fs::create_dir_all(tmp.path().join("u2/My_World")).unwrap();
        std::fs::write(tmp.path().join("u2/My_World/level.dat"), b"x").unwrap();
        let (root, name) = locate_world_root(&tmp.path().join("u2"), "map.zip").unwrap();
        assert_eq!(root, tmp.path().join("u2/My_World"), "子目录即世界根");
        assert_eq!(
            name, "My_World",
            "子目录名作为世界名（清洗逻辑见 sanitize_world_name 单测）"
        );

        // 无 level.dat → None
        std::fs::create_dir_all(tmp.path().join("u3/nested")).unwrap();
        std::fs::write(tmp.path().join("u3/readme.txt"), b"x").unwrap();
        assert!(locate_world_root(&tmp.path().join("u3"), "map.zip").is_none());
    }

    /// resolve_save_dir：无冲突原名；存在则 _{n} 递增（跳过已占用后缀）。
    #[test]
    fn resolve_save_dir_suffixes_conflicts() {
        let tmp = tempfile::tempdir().unwrap();
        let saves = tmp.path().join("saves");
        std::fs::create_dir_all(&saves).unwrap();
        assert_eq!(
            resolve_save_dir(&saves, "bar"),
            saves.join("bar"),
            "无冲突原名"
        );
        std::fs::create_dir_all(saves.join("foo")).unwrap();
        std::fs::create_dir_all(saves.join("foo_1")).unwrap();
        assert_eq!(
            resolve_save_dir(&saves, "foo"),
            saves.join("foo_2"),
            "冲突递增到空位"
        );
    }

    /// 名清洗：非法字符替换为 _；空/点结尾回退 world。
    #[test]
    fn sanitize_world_name_cleans_illegal_chars() {
        assert_eq!(sanitize_world_name("My:World?"), "My_World_");
        assert_eq!(sanitize_world_name("oneblock"), "oneblock", "合法名原样");
        assert_eq!(sanitize_world_name(""), "world", "空名回退");
        assert_eq!(
            sanitize_world_name("world."),
            "world",
            "Windows 目录名不能以点结尾"
        );
        assert_eq!(sanitize_world_name("  "), "world", "全空白回退");
    }
}
