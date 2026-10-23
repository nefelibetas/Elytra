// ── 缺失扫描与修复任务 ──
// scan_missing 供命令层，run_repair_task 为任务主体，count_natives_missing / count_assets_all 为共享缺失统计助手（run_task 复用）。

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;

use parking_lot::Mutex;

use super::emit::EventSink;
use super::progress::{
    cancelled, set_cancelled, set_completed, set_error, set_phase, set_total, spawn_tail_loop,
    step_done, EmitLoop,
};
use crate::config::downloader::{DownloadSource, DownloaderConfig};
use crate::downloader::fetcher;
use crate::downloader::hash::file_sha1_matches;
use crate::downloader::progress;
use crate::game::resource::assets::AssetIndexRef;
use crate::game::resource::{assets, extract, library};
use crate::game::version::model::{Artifact, Library, VersionJson};
use crate::launcher;
use crate::persist;
use crate::task::{self, StepStatus, TaskProgress, TaskStatus};
use crate::util::sha1_sub_dir;

#[derive(Debug, Clone, serde::Serialize)]
pub struct MissingReport {
    pub client_jar: bool,
    pub libraries: u32,
    pub assets: u32,
    pub natives: u32,
}

/// 读取版本目录的 version json（按文件名扫描，不假设文件名=目录名）。
fn read_version_json(vdir: &Path) -> Result<VersionJson, String> {
    let dir_name = vdir.file_name().and_then(|s| s.to_str()).unwrap_or("");
    let vj_path = launcher::args::find_version_json(vdir, dir_name).ok_or_else(|| {
        format!(
            "版本 JSON 缺失\n目录 {} 下未找到版本 JSON 文件，无法修复，请删除该版本后重新下载安装",
            vdir.display()
        )
    })?;
    let text = std::fs::read_to_string(&vj_path).map_err(common_err_ctx_key!(
        "errors.version.read",
        "读取版本文件失败"
    ))?;
    serde_json::from_str(&text).map_err(|e| {
        format!(
            "版本 JSON 损坏\n{} 解析失败: {e}，请删除该版本后重新下载安装",
            vj_path.display()
        )
    })
}

/// 缺失/损坏的 native classifier JAR（sha1 校验）；过滤与 process_natives 一致：仅旧式参与，新式由 count_pending 统计。
pub fn missing_native_artifacts<'a, 'b>(
    libraries: &'a [Library],
    lib_dir: &'b Path,
) -> impl Iterator<Item = &'a Artifact> + use<'a, 'b> {
    libraries.iter().filter_map(move |lib| {
        let artifact = extract::legacy_classifier_artifact(lib)?;
        let dest = lib_dir.join(&artifact.path);
        if dest.exists() && file_sha1_matches(&dest, &artifact.sha1) {
            None
        } else {
            Some(artifact)
        }
    })
}

pub fn count_natives_missing(libraries: &[Library], lib_dir: &Path) -> u32 {
    missing_native_artifacts(libraries, lib_dir).count() as u32
}

pub fn natives_missing_bytes(libraries: &[Library], lib_dir: &Path) -> u64 {
    missing_native_artifacts(libraries, lib_dir)
        .map(|a| a.size)
        .sum()
}

/// 扫描版本缺失文件（client.jar / 支持库 / 资源 / natives），不发起下载；资源按 assets index 与本地缓存 sha1 比对（多线程）。
pub async fn scan_missing(version_dir_name: &str) -> Result<MissingReport, String> {
    let config: DownloaderConfig = persist::downloader_config();
    let mc = config.minecraft_dir_abs();
    let vdir = mc.join("versions").join(version_dir_name);
    let vj = read_version_json(&vdir)?;

    let jar_missing = match launcher::args::find_file_in_dir(&vdir, "jar") {
        Some(p) => {
            let len_zero = p.metadata().map(|m| m.len() == 0).unwrap_or(true);
            if len_zero {
                true
            } else {
                vj.downloads
                    .client
                    .as_ref()
                    .map(|c| !file_sha1_matches(&p, &c.sha1))
                    .unwrap_or(false)
            }
        }
        None => true,
    };

    // 2. 支持库（规则过滤 + 架构匹配，与 pre_launch_check 一致）
    let lib_dir = mc.join("libraries");
    let lib_pending = library::count_pending(&vj.libraries, &lib_dir) as u32;

    let natives_pending = count_natives_missing(&vj.libraries, &lib_dir);

    // 4. 资源（多线程 sha1；带 index id/sha1 走本地缓存；拉取/解析失败显式报错，不静默视为完整）
    let (asst_pending, _) = count_assets_all(
        AssetIndexRef {
            url: &vj.asset_index.url,
            id: Some(&vj.asset_index.id),
            sha1: Some(&vj.asset_index.sha1),
        },
        &config,
        &mc,
        None,
        |_, _| {},
        |_, _| {},
    )
    .await?;

    Ok(MissingReport {
        client_jar: jar_missing,
        libraries: lib_pending,
        assets: asst_pending,
        natives: natives_pending,
    })
}

/// 按缺失情况生成修复步骤（顺序与 run_repair_task 的索引构造一致）。
pub fn repair_steps(report: &MissingReport) -> Vec<String> {
    repair_step_specs(report)
        .into_iter()
        .map(|s| s.name)
        .collect()
}

/// 修复步骤（带 kind，前端可翻译）；顺序与 [`repair_steps`] 一致。
pub fn repair_step_specs(report: &MissingReport) -> Vec<task::StepSpec> {
    let mut steps = Vec::new();
    if report.client_jar {
        steps.push(task::StepSpec {
            name: "下载客户端文件".to_string(),
            kind: task::StepKind::RepairClient,
        });
    }
    if report.libraries > 0 {
        steps.push(task::StepSpec {
            name: "下载支持库文件".to_string(),
            kind: task::StepKind::RepairLibraries,
        });
    }
    if report.assets > 0 {
        steps.push(task::StepSpec {
            name: "下载资源文件".to_string(),
            kind: task::StepKind::RepairAssets,
        });
    }
    if report.natives > 0 {
        steps.push(task::StepSpec {
            name: "处理 native 库".to_string(),
            kind: task::StepKind::RepairNatives,
        });
    }
    steps
}

struct RepairCtx<'a> {
    app: &'a dyn EventSink,
    cancel: &'a Arc<AtomicBool>,
    progress: &'a Arc<Mutex<TaskProgress>>,
    version_dir_name: &'a str,
    config: DownloaderConfig,
    sources: Vec<DownloadSource>,
    vdir: PathBuf,
    lib_dir: PathBuf,
    vj: VersionJson,
    /// 提交时缺失快照（驱动步骤索引与总量）
    jar_missing: bool,
    lib_pending: u32,
    asst_pending: u32,
    total_files: u32,
    jar_idx: Option<usize>,
    libs_idx: Option<usize>,
    asst_idx: Option<usize>,
    natives_idx: Option<usize>,
    /// 进度原子计数器（下载回调与收尾心跳共享）
    ctr: RepairCounters,
}

/// repair 任务进度计数器组（`Default` 全 0 初始化）。
#[derive(Default)]
struct RepairCounters {
    jar_files: Arc<AtomicU32>,
    lib_files: Arc<AtomicU32>,
    asst_files: Arc<AtomicU32>,
    /// jar_bytes/jar_total 为覆盖式（下载回调按当前绝对值），libs/assets 字节为累加式
    jar_bytes: Arc<AtomicU64>,
    jar_total: Arc<AtomicU64>,
    bytes: Arc<AtomicU64>,
}

/// 准备：汇总缺失总量 → 生成步骤索引 → 推送总量/初始分步状态；None = 无需继续（JSON 缺失已 set_error，或 total==0 已收口 Done）。
fn prepare_repair<'a>(
    app: &'a dyn EventSink,
    cancel: &'a Arc<AtomicBool>,
    progress: &'a Arc<Mutex<TaskProgress>>,
    version_dir_name: &'a str,
    report: &MissingReport,
) -> Option<RepairCtx<'a>> {
    let config: DownloaderConfig = persist::downloader_config();
    let sources: Vec<DownloadSource> = config.file_sources_list().into_iter().cloned().collect();
    let mc = config.minecraft_dir_abs();
    let vdir = mc.join("versions").join(version_dir_name);
    let vj = match read_version_json(&vdir) {
        Ok(v) => v,
        Err(e) => {
            set_error(app, progress, e);
            return None;
        }
    };

    let jar_missing = report.client_jar;
    let lib_pending = report.libraries;
    let asst_pending = report.assets;
    let natives_pending = report.natives;
    let total_files =
        if jar_missing { 1u32 } else { 0u32 } + lib_pending + asst_pending + natives_pending;
    eprintln!(
        "[repair] {} missing: jar={} libs={} assets={} natives={} total={}",
        version_dir_name, jar_missing, lib_pending, asst_pending, natives_pending, total_files
    );
    if total_files == 0 {
        close_repair_noop(app, progress);
        return None;
    }

    let (jar_idx, libs_idx, asst_idx, natives_idx) = repair_step_indices(report);

    set_total(app, progress, total_files, 0);
    set_completed(app, progress, 0, 0);
    if let Some(i) = jar_idx {
        set_phase(
            app,
            progress,
            Some(TaskStatus::Running),
            "修复中",
            i,
            StepStatus::Running,
        );
    }

    Some(RepairCtx {
        app,
        cancel,
        progress,
        version_dir_name,
        config,
        sources,
        vdir,
        lib_dir: mc.join("libraries"),
        vj,
        jar_missing,
        lib_pending,
        asst_pending,
        total_files,
        jar_idx,
        libs_idx,
        asst_idx,
        natives_idx,
        ctr: RepairCounters::default(),
    })
}

fn close_repair_noop(app: &dyn EventSink, progress: &Arc<Mutex<TaskProgress>>) {
    task::update_and_emit(app, progress, |p| {
        p.status = TaskStatus::Done;
        p.progress = 100.0;
        p.current_phase = "文件完整，无需修复".into();
        p.current_phase_key = Some("task.backend.phase.no_repair".into());
        p.current_phase_params = None;
    });
    if let Some(p) = progress.try_lock() {
        let task_id = p.task_id.clone();
        drop(p); // 先释放 progress 锁再 fail（fail 内部读 progress，防死锁）
        task::global().fail(&task_id);
    }
}

/// 进度上报循环（EmitLoop）。repair 无 ModLoader 安装步骤，安装字段全为占位。
fn spawn_repair_emit_loop(
    ctx: &RepairCtx<'_>,
    sink: Arc<dyn EventSink>,
) -> tokio::task::JoinHandle<()> {
    EmitLoop {
        sink,
        cancel: Arc::clone(ctx.cancel),
        progress: Arc::clone(ctx.progress),
        jar_done: Arc::clone(&ctx.ctr.jar_files),
        lib_done: Arc::clone(&ctx.ctr.lib_files),
        asst_done: Arc::clone(&ctx.ctr.asst_files),
        // natives 在 EmitLoop 生命周期外单独执行，计数/标志全为占位
        natives_done: Arc::new(AtomicU32::new(0)),
        jar_bytes: Arc::clone(&ctx.ctr.jar_bytes),
        jar_total: Arc::clone(&ctx.ctr.jar_total),
        bytes: Arc::clone(&ctx.ctr.bytes),
        lib_bytes: Arc::new(AtomicU64::new(0)),
        asst_bytes: Arc::new(AtomicU64::new(0)),
        natives_bytes: Arc::new(AtomicU64::new(0)),
        lib_pending_bytes: 0,
        asst_pending_bytes: 0,
        natives_pending_bytes: 0,
        loader_libs_bytes: Arc::new(AtomicU64::new(0)),
        loader_libs_total: Arc::new(AtomicU64::new(0)),
        optifine_bytes: Arc::new(AtomicU64::new(0)),
        optifine_total: Arc::new(AtomicU64::new(0)),
        fabric_bytes: Arc::new(AtomicU64::new(0)),
        fabric_total: Arc::new(AtomicU64::new(0)),
        jar_step: ctx.jar_idx,
        lib_step: ctx.libs_idx,
        asst_step: ctx.asst_idx,
        natives_step: None,
        loader_libs_step: None,
        optifine_step: None,
        fabric_step: None,
        cfg_done: Arc::new(AtomicBool::new(false)),
        libs_done: Arc::new(AtomicBool::new(false)),
        optifine_done: Arc::new(AtomicBool::new(false)),
        fabric_done: Arc::new(AtomicBool::new(false)),
        lib_pending: ctx.lib_pending,
        asst_pending: ctx.asst_pending,
        natives_pending: 0,
        initial_completed: 0,
        total_files: ctx.total_files,
        initial_bytes: 0,
        total_bytes: 0,
        jar_total_fallback: 0,
    }
    .spawn()
}

/// client.jar / 支持库 / 资源三路并行（`join!`），任一失败返回错误文案列表。
async fn repair_downloads(ctx: &RepairCtx<'_>) -> Result<(), Vec<String>> {
    let src_refs: Vec<&DownloadSource> = ctx.sources.iter().collect();

    let pc_jar = progress::ProgressCounters {
        files: Arc::clone(&ctx.ctr.jar_files),
        bytes: Arc::clone(&ctx.ctr.jar_bytes),
        step_total: Arc::clone(&ctx.ctr.jar_total),
        ..Default::default()
    };
    let jar = repair_jar(
        &ctx.vj,
        &ctx.vdir,
        ctx.version_dir_name,
        ctx.jar_missing,
        &ctx.sources,
        &pc_jar,
        ctx.cancel,
    );

    let pc_lib = progress::ProgressCounters {
        files: Arc::clone(&ctx.ctr.lib_files),
        bytes: Arc::clone(&ctx.ctr.bytes),
        ..Default::default() // 占位：repair 分步进度走文件数回退
    };
    let libs = library::download_libraries(
        &ctx.vj.libraries,
        &ctx.lib_dir,
        ctx.config.concurrency as usize,
        &src_refs,
        None, // 修复任务不走本地运行时复用（保持"缺失补全"语义）
        &pc_lib,
        Some(ctx.cancel),
    );

    let pc_asst = progress::ProgressCounters {
        files: Arc::clone(&ctx.ctr.asst_files),
        bytes: Arc::clone(&ctx.ctr.bytes),
        ..Default::default() // 占位：repair 分步进度走文件数回退
    };
    let asst = assets::download_assets(
        AssetIndexRef {
            url: &ctx.vj.asset_index.url,
            id: Some(&ctx.vj.asset_index.id),
            sha1: Some(&ctx.vj.asset_index.sha1),
        },
        &ctx.config,
        ctx.config.concurrency as usize,
        None, // 同上
        &pc_asst,
        Some(ctx.cancel),
    );

    // 用 join! 等全部收尾：try_join! 短路后孤儿 worker 仍在写盘，与失败清理竞争。
    let (jar, libs, asst) = tokio::join!(jar, libs, asst);
    let mut errors: Vec<String> = Vec::new();
    for r in [jar, libs, asst] {
        if let Err(mut e) = r {
            errors.append(&mut e);
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

/// natives 阶段（幂等：已解压跳过；仅报告有缺失时才执行，与 steps 索引对应）。
async fn repair_natives(ctx: &RepairCtx<'_>) -> Result<(), Vec<String>> {
    let Some(i) = ctx.natives_idx else {
        return Ok(());
    };
    set_phase(
        ctx.app,
        ctx.progress,
        None,
        "处理 native 库",
        i,
        StepStatus::Running,
    );
    let src_refs: Vec<&DownloadSource> = ctx.sources.iter().collect();
    extract::process_natives(
        &ctx.vj.libraries,
        &ctx.lib_dir,
        &src_refs,
        &progress::ProgressCounters {
            files: Arc::clone(&ctx.ctr.lib_files),
            bytes: Arc::clone(&ctx.ctr.bytes),
            ..Default::default() // 占位：repair 分步进度走文件数回退
        },
        Some(ctx.cancel),
    )
    .await?;
    step_done(ctx.app, ctx.progress, i);
    Ok(())
}

/// 补全任务主体：基于本地 version json 下载缺失文件。`report` 为提交时扫描结果（步骤数与其一致），运行中按对应索引更新。
pub async fn run_repair_task(
    sink: Arc<dyn EventSink>,
    cancel: &Arc<AtomicBool>,
    progress: &Arc<Mutex<TaskProgress>>,
    version_dir_name: &str,
    report: MissingReport,
) {
    let app = sink.as_ref();

    let Some(ctx) = prepare_repair(app, cancel, progress, version_dir_name, &report) else {
        return;
    };

    let emit_handle = spawn_repair_emit_loop(&ctx, Arc::clone(&sink));
    let parallel = repair_downloads(&ctx).await;
    emit_handle.abort();

    if cancelled(cancel) {
        return set_cancelled(app, progress);
    }
    if let Err(errors) = parallel {
        eprintln!(
            "[repair] {} download failed: {}",
            version_dir_name,
            errors.join("; ")
        );
        return set_error(app, progress, errors.join("; "));
    }

    // 收尾心跳：EmitLoop 已 abort，natives 期间继续上报速度/字节进度
    let _tail_loop = spawn_tail_loop(Arc::clone(&sink), progress, Arc::clone(&ctx.ctr.bytes));

    if let Some(i) = ctx.jar_idx {
        step_done(app, progress, i);
    }
    if let Some(i) = ctx.libs_idx {
        step_done(app, progress, i);
    }
    if let Some(i) = ctx.asst_idx {
        step_done(app, progress, i);
    }

    if let Err(errors) = repair_natives(&ctx).await {
        eprintln!(
            "[repair] {} natives failed: {}",
            version_dir_name,
            errors.join("; ")
        );
        return set_error(app, progress, errors.join("; "));
    }

    task::update_and_emit(app, progress, |p| {
        p.status = TaskStatus::Done;
        p.completed_files = p.total_files;
        p.progress = 100.0;
        p.current_phase = "修复完成".into();
        p.current_phase_key = Some("task.backend.phase.repair_done".into());
        p.current_phase_params = None;
    });
    if let Some(p) = progress.try_lock() {
        let task_id = p.task_id.clone();
        drop(p); // 先释放 progress 锁再 fail（fail 内部读 progress，防死锁）
        task::global().fail(&task_id);
    }
}

/// 生成修复步骤索引（顺序与 repair_steps 一致：jar → libs → assets → natives；natives 恒最后一位）。
fn repair_step_indices(
    report: &MissingReport,
) -> (Option<usize>, Option<usize>, Option<usize>, Option<usize>) {
    let mut idx = 0usize;
    let jar_idx = if report.client_jar {
        let i = idx;
        idx += 1;
        Some(i)
    } else {
        None
    };
    let libs_idx = if report.libraries > 0 {
        let i = idx;
        idx += 1;
        Some(i)
    } else {
        None
    };
    let asst_idx = if report.assets > 0 {
        let i = idx;
        idx += 1;
        Some(i)
    } else {
        None
    };
    let natives_idx = (report.natives > 0).then_some(idx);
    (jar_idx, libs_idx, asst_idx, natives_idx)
}

/// 修复 client.jar：清理损坏（0 字节/sha1 不匹配）旧文件后逐源 sha1 校验下载；cancel 置位后中途立即中断，大 jar 不等自然结束/30s 读超时。
async fn repair_jar(
    vj: &VersionJson,
    vdir: &Path,
    version_dir_name: &str,
    jar_missing: bool,
    sources: &[DownloadSource],
    pc: &progress::ProgressCounters,
    cancel: &AtomicBool,
) -> Result<(), Vec<String>> {
    if jar_missing {
        if let Some(bad) = launcher::args::find_file_in_dir(vdir, "jar") {
            let bad_jar = bad.metadata().map(|m| m.len() == 0).unwrap_or(true)
                || vj
                    .downloads
                    .client
                    .as_ref()
                    .map(|c| !file_sha1_matches(&bad, &c.sha1))
                    .unwrap_or(false);
            if bad_jar {
                let _ = std::fs::remove_file(&bad);
            }
        }
        if let Some(ref c) = vj.downloads.client {
            let urls: Vec<String> = sources.iter().map(|src| src.mirror_url(&c.url)).collect();
            let jar_path = vdir.join(format!("{version_dir_name}.jar"));
            // EmitLoop 把 jar_bytes/jar_total 当当前绝对值消费（overwrite 覆盖式）；已知大小先预填分母。
            if c.size > 0 {
                pc.step_total.store(c.size, Ordering::Relaxed);
            }
            let cb = pc.overwrite_callback();
            // 逐源 sha1 校验下载：镜像返回不完整/错误内容自动换源重试；可取消变体中途即停。
            fetcher::fetch_verified_c(&urls, &jar_path, Some(cb), &c.sha1, Some(cancel))
                .await
                .map_err(|e| vec![e.to_string()])?;
        }
        pc.files.store(1, Ordering::Relaxed);
    }
    Ok(())
}

/// 统计资源总/缺失数（本地缓存 sha1 比对，多线程）。拉取/解析失败返回 Err（不静默视为完整）；index_id/sha1 非空走本地缓存；cancel 期间轮询，取消返回 Err("任务已取消")。
pub async fn count_assets_all(
    index: AssetIndexRef<'_>,
    config: &DownloaderConfig,
    mc_dir: &Path,
    cancel: Option<&AtomicBool>,
    mut on_total: impl FnMut(u32, u64),
    mut on_checked: impl FnMut(u32, u64),
) -> Result<(u32, u64), String> {
    let AssetIndexRef {
        url: asset_index_url,
        id: index_id,
        sha1: index_sha1,
    } = index;
    let text =
        assets::fetch_asset_index(config, mc_dir, asset_index_url, index_id, index_sha1).await?;
    eprintln!("[assets] 索引文本: {}B", text.len());
    let idx: serde_json::Value = serde_json::from_str(&text)
        .map_err(common_err_ctx_key!("errors.parse", "解析资源索引失败"))?;
    eprintln!("[assets] JSON 解析完成");
    let objects_dir = mc_dir.join("assets").join("objects");

    let mut items: Vec<(String, std::path::PathBuf, u64)> = Vec::new();
    let mut total_bytes: u64 = 0;
    if let Some(map) = idx["objects"].as_object() {
        for (_path, obj) in map {
            let Some(hash) = obj.get("hash").and_then(|v| v.as_str()) else {
                continue;
            };
            let size = obj.get("size").and_then(|v| v.as_u64()).unwrap_or(0);
            total_bytes += size;
            items.push((
                hash.to_string(),
                objects_dir.join(sha1_sub_dir(hash)).join(hash),
                size,
            ));
        }
    }
    let total = items.len() as u32;
    eprintln!("[assets] objects 清单: {} 个, 总 {}B", total, total_bytes);
    if total == 0 {
        return Ok((0, 0));
    }
    on_total(total, total_bytes);

    let n_threads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        .clamp(1, 8);
    let chunk = items.len().div_ceil(n_threads);
    let pending = AtomicU32::new(0);
    let pending_bytes = AtomicU64::new(0);
    let checked = AtomicU32::new(0);
    let existing_bytes = AtomicU64::new(0);
    // 检测进度经 mpsc 汇总到主线程回调（多线程直接 emit 有锁竞争）；recv_timeout 轮询期间检查取消。
    let n_slices = items.chunks(chunk).len();
    let finished = std::sync::atomic::AtomicUsize::new(0);
    let (tx, rx) = std::sync::mpsc::channel::<u32>();
    eprintln!(
        "[assets] 开始 sha1 扫描: {} items / {} 线程",
        items.len(),
        n_threads
    );
    std::thread::scope(|s| {
        for slice in items.chunks(chunk) {
            let tx = tx.clone();
            let counter = &pending;
            let pb = &pending_bytes;
            let checked = &checked;
            let existing = &existing_bytes;
            let finished = &finished;
            s.spawn(move || {
                let mut p = 0u32;
                let mut pb_local = 0u64;
                let mut done = 0u32;
                for (hash, dest, size) in slice {
                    if cancel.is_some_and(|c| c.load(Ordering::Relaxed)) {
                        break; // 已取消：尽早退出
                    }
                    if dest.exists() && file_sha1_matches(dest, hash) {
                        existing.fetch_add(*size, Ordering::Relaxed);
                    } else {
                        p += 1;
                        pb_local += *size;
                    }
                    done += 1;
                    if done.is_multiple_of(500) {
                        checked.fetch_add(done, Ordering::Relaxed);
                        let _ = tx.send(done);
                        done = 0;
                    }
                }
                checked.fetch_add(done, Ordering::Relaxed);
                let _ = tx.send(done);
                counter.fetch_add(p, Ordering::Relaxed);
                pb.fetch_add(pb_local, Ordering::Relaxed);
                finished.fetch_add(1, Ordering::Relaxed);
            });
        }
        drop(tx);
        loop {
            match rx.recv_timeout(std::time::Duration::from_millis(500)) {
                Ok(_) => {
                    on_checked(
                        checked.load(Ordering::Relaxed),
                        existing_bytes.load(Ordering::Relaxed),
                    );
                }
                Err(_) => {
                    // 500ms 无消息：可能仍在扫大文件或线程全退；线程全退/取消即结束。
                    if cancel.is_some_and(|c| c.load(Ordering::Relaxed))
                        || finished.load(Ordering::Relaxed) >= n_slices
                    {
                        break;
                    }
                }
            }
        }
    });
    if cancel.is_some_and(|c| c.load(Ordering::Relaxed)) {
        return Err("任务已取消".into());
    }
    Ok((
        pending.load(Ordering::Relaxed),
        pending_bytes.load(Ordering::Relaxed),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::downloader::DownloaderConfig;
    use crate::downloader::hash::sha1_hex;
    use crate::game::version::model::{Artifact, Library, LibraryDownloads};
    use std::collections::HashMap;

    /// 步骤索引必须与 `repair_steps` 的名字逐一对齐（两处手写同序）；穷举 16 种缺失组合锁死契约。
    #[test]
    fn test_repair_indices_align_with_steps() {
        for mask in 0u8..16 {
            let report = MissingReport {
                client_jar: mask & 1 != 0,
                libraries: u32::from(mask & 2 != 0),
                assets: u32::from(mask & 4 != 0),
                natives: u32::from(mask & 8 != 0),
            };
            let names = repair_steps(&report);
            let (jar, libs, asst, natives) = repair_step_indices(&report);

            for (idx, expect) in [
                (jar, "下载客户端文件"),
                (libs, "下载支持库文件"),
                (asst, "下载资源文件"),
                (natives, "处理 native 库"),
            ] {
                match idx {
                    Some(i) => assert_eq!(
                        names.get(i).map(String::as_str),
                        Some(expect),
                        "mask={mask}：索引 {i} 应落在「{expect}」"
                    ),
                    None => assert!(
                        !names.iter().any(|n| n == expect),
                        "mask={mask}：无「{expect}」步骤却给出了索引"
                    ),
                }
            }

            // 出现的步骤按顺序占据 0..n 连续下标，无空洞
            let mut next = 0usize;
            for (idx, present) in [
                (jar, report.client_jar),
                (libs, report.libraries > 0),
                (asst, report.assets > 0),
                (natives, report.natives > 0),
            ] {
                if present {
                    assert_eq!(idx, Some(next), "mask={mask}：下标应连续");
                    next += 1;
                }
            }
            assert_eq!(
                names.len(),
                next,
                "mask={mask}：步骤表长度应与出现的维度数一致"
            );
        }
    }

    #[test]
    fn test_repair_steps_all_missing() {
        let steps = repair_steps(&MissingReport {
            client_jar: true,
            libraries: 3,
            assets: 12,
            natives: 2,
        });
        assert_eq!(
            steps,
            vec![
                "下载客户端文件",
                "下载支持库文件",
                "下载资源文件",
                "处理 native 库"
            ]
        );
    }

    #[test]
    fn test_repair_steps_partial() {
        let steps = repair_steps(&MissingReport {
            client_jar: false,
            libraries: 0,
            assets: 5,
            natives: 0,
        });
        assert_eq!(steps, vec!["下载资源文件"]);
    }

    #[test]
    fn test_repair_steps_only_jar() {
        let steps = repair_steps(&MissingReport {
            client_jar: true,
            libraries: 0,
            assets: 0,
            natives: 0,
        });
        assert_eq!(steps, vec!["下载客户端文件"]);
    }

    #[test]
    fn test_repair_steps_only_natives() {
        let steps = repair_steps(&MissingReport {
            client_jar: false,
            libraries: 0,
            assets: 0,
            natives: 1,
        });
        assert_eq!(steps, vec!["处理 native 库"]);
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn test_count_natives_missing() {
        let tmp = std::env::temp_dir().join(format!("elytra_natives_test_{}", std::process::id()));
        let lib_dir = tmp.join("libraries");
        std::fs::create_dir_all(&lib_dir).unwrap();

        let good_sha1 = sha1_hex(b"good");
        let good_path = "net/example/natives-good.jar";
        let good_dest = lib_dir.join(good_path);
        std::fs::create_dir_all(good_dest.parent().unwrap()).unwrap();
        std::fs::write(&good_dest, b"good").unwrap();

        let bad_path = "net/example/natives-bad.jar";
        let bad_dest = lib_dir.join(bad_path);
        std::fs::create_dir_all(bad_dest.parent().unwrap()).unwrap();
        std::fs::write(&bad_dest, b"corrupted").unwrap();

        let missing_path = "net/example/natives-missing.jar";

        let mk = |classifier_path: &str, sha1: &str| Library {
            name: "net.example:lib:1.0".into(),
            downloads: Some(LibraryDownloads {
                artifact: None,
                classifiers: Some(HashMap::from([(
                    "natives-windows".into(),
                    Artifact {
                        path: classifier_path.into(),
                        url: "".into(),
                        sha1: sha1.into(),
                        size: 0,
                    },
                )])),
            }),
            rules: None,
            natives: Some(HashMap::from([(
                "windows".into(),
                "natives-windows".into(),
            )])),
            extract: None,
        };

        let libs = vec![
            Library {
                name: "plain".into(),
                downloads: None,
                rules: None,
                natives: None,
                extract: None,
            },
            mk(good_path, &good_sha1),    // 存在且 sha1 匹配 → 完整
            mk(bad_path, &good_sha1),     // 存在但 sha1 不匹配 → 缺失
            mk(missing_path, &good_sha1), // 不存在 → 缺失
        ];
        assert_eq!(count_natives_missing(&libs, &lib_dir), 2);
        std::fs::remove_dir_all(&tmp).ok();
    }

    /// 端到端网络测试：拉真实资源索引 → 对象清单 → 多线程 sha1 扫描。
    #[tokio::test]
    async fn count_assets_all_end_to_end() {
        let cfg = DownloaderConfig::default();
        let mc = std::env::temp_dir().join(format!("elytra_caa_e2e_{}", std::process::id()));
        let url =
            "https://piston-meta.mojang.com/v1/packages/cf75b185cb35b32e299b0c8e674fa202d7911a3c/32.json";
        let sha1 = "cf75b185cb35b32e299b0c8e674fa202d7911a3c";
        let mut total = 0u32;
        let mut total_bytes = 0u64;
        let mut checked = 0u32;
        let res = tokio::time::timeout(
            std::time::Duration::from_secs(30),
            count_assets_all(
                AssetIndexRef {
                    url,
                    id: Some("32"),
                    sha1: Some(sha1),
                },
                &cfg,
                &mc,
                None,
                |t, b| {
                    total = t;
                    total_bytes = b;
                },
                |c, _b| checked = c,
            ),
        )
        .await;
        let (pending, pending_bytes) = res
            .expect("B2 完整流程应在 30s 内完成（网络/代理问题会超时）")
            .expect("count_assets_all 应成功");
        assert!(total > 1000, "objects 清单应完整: {total}");
        assert_eq!(checked, total, "扫描进度应到全量");
        assert_eq!(pending, total, "空目录下应全部缺失");
        assert_eq!(
            pending_bytes, total_bytes,
            "空目录全缺失字节应等于清单总字节"
        );
        assert!(pending_bytes > 0);
        let _ = std::fs::remove_dir_all(&mc);
    }
}
