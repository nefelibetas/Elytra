// ── 完整下载任务主体（原版 / ModLoader / OptiFine / Fabric API）──

use parking_lot::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;

use super::emit::EventSink;

use std::path::Path;

use crate::config::downloader::{DownloadSource, DownloaderConfig};
use crate::downloader::fetcher;
use crate::downloader::path_lock::PathLock;
use crate::downloader::{progress, reuse};
use crate::game::loader;
use crate::game::resource::{assets, extract, library};
use crate::game::resource::assets::AssetIndexRef;
use crate::game::version::model::VersionJson;
use crate::game::version::dir::resolve_version_dir;
use crate::game::version::meta;
use crate::persist;
use crate::task::{
    self,
    progress::{
        cancelled, set_cancelled, set_completed, set_done, set_error, set_phase, set_total,
        step_done, EmitLoop,
    },
    repair::{count_assets_all, count_natives_missing},
    runner::progress::ProgressCounters,
    DownloadTask, FabricApi, LoaderChoice, StepInfo, StepKind, StepSpec, StepStatus, TaskProgress,
    TaskSpec, TaskStatus,
};
use crate::util;

struct TaskRuntime {
    started: std::time::Instant,
    config: DownloaderConfig,
    sources: Vec<DownloadSource>,
    mc: std::path::PathBuf,
    dir_name: String,
    vdir: std::path::PathBuf,
    reuse_root: Option<reuse::LocalRuntimeSource>,
    vdir_existed_before: bool,
    /// 目标目录互斥锁：必须存活到任务结束（Drop 释放占用）。
    _lock: PathLock,
}

/// 准备：解析目标目录（不依赖 version JSON）→ 复用根探测 → 目录互斥占用；Err 为可展示错误文案（调用方转 set_error）。
fn prepare_runtime(task: &DownloadTask) -> Result<TaskRuntime, String> {
    let config: DownloaderConfig = persist::downloader_config();
    let sources: Vec<DownloadSource> = config.file_sources_list().into_iter().cloned().collect();
    let mc = config.minecraft_dir_abs();

    // ── 目标目录提前确定（不依赖 version JSON），同路径互斥 ──
    let TargetDir { dir_name, vdir } = resolve_target_dir(task, &mc);
    // 本地复用根探测（迁移场景）：版本目录向上找含 assets/+libraries/ 的 .minecraft 根；命中则 libraries/assets/client jar 先走 size+sha1 本地复制，免全量重下（无根则 None）。
    let reuse_root = reuse::LocalRuntimeSource::discover(&vdir);
    // 任务开始前目录是否已存在：全新下载失败整删残缺目录；补全（目录已存在）失败不删。
    let vdir_existed_before = vdir.exists();
    let lock = PathLock::try_acquire(&vdir).ok_or_else(|| {
        format!("该版本正在被其他任务下载:\n{}", vdir.display())
    })?;

    Ok(TaskRuntime {
        started: std::time::Instant::now(),
        config,
        sources,
        mc,
        dir_name,
        vdir,
        reuse_root,
        vdir_existed_before,
        _lock: lock,
    })
}

/// 阶段 A：拉取版本 JSON（唯一硬前置）→ 非 loader 模式落盘原版 JSON → 登记取消时的清理路径；Err(()) = 已自行收口（set_error/set_cancelled），调用方直接返回。
async fn stage_fetch_version(
    app: &dyn EventSink,
    progress: &Arc<Mutex<TaskProgress>>,
    cancel: &Arc<AtomicBool>,
    runtime: &TaskRuntime,
    task: &DownloadTask,
) -> Result<(VersionJson, String), ()> {
    let config = &runtime.config;
    let vdir = &runtime.vdir;
    let t0 = runtime.started;

    eprintln!("[task] 阶段A: fetch version json");
    let (vj, parent_text) = match fetch_version_json(&task.version_url, config).await {
        Ok(v) => v,
        Err(e) => {
            eprintln!("[task] 阶段A 失败: {e}");
            set_error(app, progress, e);
            return Err(());
        }
    };
    eprintln!(
        "[task] 阶段A 完成 ({:.1}s): id={} libs={}",
        t0.elapsed().as_secs_f64(),
        vj.id,
        vj.libraries.len()
    );
    step_done(app, progress, 0);
    if cancelled(cancel) {
        set_cancelled(app, progress);
        return Err(());
    }

    // 建目录；非 loader 模式写原版 version JSON（loader 模式由 installer 写合并 JSON）
    std::fs::create_dir_all(vdir).ok();
    if task.loader.is_none() {
        // 写盘失败必须报错：否则版本缺 json，启动时报"未找到版本 JSON"
        match serde_json::to_string_pretty(&vj) {
            Ok(json) => {
                let jp = vdir.join(format!("{}.json", task.version_id));
                if let Err(e) = std::fs::write(&jp, &json) {
                    set_error(app, progress, format!("写入版本 JSON 失败\n{e}"));
                    return Err(());
                }
            }
            Err(e) => {
                set_error(app, progress, format!("序列化版本 JSON 失败\n{e}"));
                return Err(());
            }
        }
    }
    // 仅新建目录登记清理；补全已有版本取消时不删，避免误删用户文件。
    if !runtime.vdir_existed_before {
        // 先取 task_id 释放 progress 守卫再调 set_cleanup_path（后者取 inner 锁），否则 ABBA。
        let cleanup_task_id = progress.try_lock().map(|p| p.task_id.clone());
        if let Some(task_id) = cleanup_task_id {
            task::global().set_cleanup_path(&task_id, vdir.clone());
        }
    }
    Ok((vj, parent_text))
}

#[derive(Default)]
struct Counters {
    jar_done: Arc<AtomicU32>,
    jar_bytes: Arc<AtomicU64>,
    jar_total: Arc<AtomicU64>,
    lib_done: Arc<AtomicU32>,
    asst_done: Arc<AtomicU32>,
    natives_done: Arc<AtomicU32>,
    bytes: Arc<AtomicU64>,
    lib_step_bytes: Arc<AtomicU64>,
    asst_step_bytes: Arc<AtomicU64>,
    natives_step_bytes: Arc<AtomicU64>,
    loader_libs_step_bytes: Arc<AtomicU64>,
    loader_libs_step_total: Arc<AtomicU64>,
    optifine_step_bytes: Arc<AtomicU64>,
    optifine_step_total: Arc<AtomicU64>,
    fabric_step_bytes: Arc<AtomicU64>,
    fabric_step_total: Arc<AtomicU64>,
    cfg_done: Arc<AtomicBool>,
    libs_done: Arc<AtomicBool>,
    optifine_done: Arc<AtomicBool>,
    fabric_done: Arc<AtomicBool>,
}

/// 阶段 B 分析出的步骤索引（`None` = 该步骤未启用，执行点不推进）。
struct StepIndex {
    jar: Option<usize>,
    lib: Option<usize>,
    asst: Option<usize>,
    natives: Option<usize>,
    cfg: Option<usize>,
    libs: Option<usize>,
    optifine: Option<usize>,
    fabric: Option<usize>,
}

struct DownloadCtx<'a> {
    app: &'a dyn EventSink,
    cancel: &'a Arc<AtomicBool>,
    progress: &'a Arc<Mutex<TaskProgress>>,
    config: &'a DownloaderConfig,
    sources: &'a [DownloadSource],
    src_refs: &'a [&'a DownloadSource],
    reuse_root: Option<&'a reuse::LocalRuntimeSource>,
    vj: &'a VersionJson,
    dir_name: &'a str,
    vdir: &'a Path,
    lib_dir: &'a Path,
    ctr: &'a Counters,
    steps: &'a StepIndex,
}

/// 阶段 C 步骤：client jar（本地复用优先，失败降级多源 sha1 校验下载）。
async fn download_client_jar(ctx: &DownloadCtx<'_>) -> Result<(), Vec<String>> {
    let DownloadCtx {
        app,
        cancel,
        progress,
        sources,
        reuse_root,
        vj,
        dir_name,
        vdir,
        ctr,
        steps,
        ..
    } = *ctx;
    if let Some(c) = &vj.downloads.client {
        let jar_path = vdir.join(format!("{}.jar", dir_name));
        // 只写 jar 字节；不碰 ctr.bytes（libs/assets 增量累加，覆盖写会致速度跳变）
        let cb = ProgressCounters {
            bytes: Arc::clone(&ctr.jar_bytes),
            step_total: Arc::clone(&ctr.jar_total),
            ..Default::default()
        }
        .overwrite_callback();
        let urls: Vec<String> = sources.iter().map(|src| src.mirror_url(&c.url)).collect();
        let client_rel = format!("versions/{}/{}.jar", dir_name, dir_name);
        if let Some(src) = reuse_root.and_then(|r| {
            reuse::classify_local_artifact(r, &client_rel, Some(c.size), Some(&c.sha1))
        }) {
            if let Err(e) =
                reuse::copy_verified_local(&src, &jar_path, c.size, Some(cb.clone())).await
            {
                fetcher::fetch_verified_c(&urls, &jar_path, Some(cb), &c.sha1, Some(cancel))
                    .await
                    .map_err(|e2| vec![format!("{e2}（本地复用失败: {e}）")])?;
            }
        } else {
            fetcher::fetch_verified_c(&urls, &jar_path, Some(cb), &c.sha1, Some(cancel))
                .await
                .map_err(|e| vec![e.to_string()])?;
        }
        ctr.jar_done.store(1, Ordering::Relaxed);
    }
    if let Some(i) = steps.jar {
        step_done(app, progress, i);
    }
    Ok(())
}

async fn download_libraries_step(ctx: &DownloadCtx<'_>) -> Result<(), Vec<String>> {
    let DownloadCtx {
        app,
        cancel,
        progress,
        config,
        src_refs,
        reuse_root,
        vj,
        lib_dir,
        ctr,
        steps,
        ..
    } = *ctx;
    library::download_libraries(
        &vj.libraries,
        lib_dir,
        config.concurrency as usize,
        src_refs,
        reuse_root.cloned(),
        &ProgressCounters {
            files: Arc::clone(&ctr.lib_done),
            bytes: Arc::clone(&ctr.bytes),
            step_bytes: Arc::clone(&ctr.lib_step_bytes),
            ..Default::default()
        },
        Some(cancel),
    )
    .await?;
    if let Some(i) = steps.lib {
        step_done(app, progress, i);
    }
    Ok(())
}

async fn download_assets_step(ctx: &DownloadCtx<'_>) -> Result<(), Vec<String>> {
    let DownloadCtx {
        app,
        cancel,
        progress,
        config,
        reuse_root,
        vj,
        ctr,
        steps,
        ..
    } = *ctx;
    assets::download_assets(
        AssetIndexRef {
            url: &vj.asset_index.url,
            id: Some(&vj.asset_index.id),
            sha1: Some(&vj.asset_index.sha1),
        },
        config,
        config.concurrency as usize,
        reuse_root.cloned(),
        &ProgressCounters {
            files: Arc::clone(&ctr.asst_done),
            bytes: Arc::clone(&ctr.bytes),
            step_bytes: Arc::clone(&ctr.asst_step_bytes),
            ..Default::default()
        },
        Some(cancel),
    )
    .await?;
    if let Some(i) = steps.asst {
        step_done(app, progress, i);
    }
    Ok(())
}

async fn extract_natives_step(ctx: &DownloadCtx<'_>) -> Result<(), Vec<String>> {
    let DownloadCtx {
        app,
        cancel,
        progress,
        src_refs,
        vj,
        lib_dir,
        ctr,
        steps,
        ..
    } = *ctx;
    extract::process_natives(
        &vj.libraries,
        lib_dir,
        src_refs,
        &ProgressCounters {
            files: Arc::clone(&ctr.natives_done),
            bytes: Arc::clone(&ctr.bytes),
            step_bytes: Arc::clone(&ctr.natives_step_bytes),
            ..Default::default()
        },
        Some(cancel),
    )
    .await?;
    if let Some(i) = steps.natives {
        step_done(app, progress, i);
    }
    Ok(())
}

fn spawn_emit_loop(
    ctx: &DownloadCtx<'_>,
    sink: Arc<dyn EventSink>,
    scan: &LocalScan,
    total_files: u32,
    jar_total_fallback: u64,
) -> tokio::task::JoinHandle<()> {
    let DownloadCtx {
        cancel,
        progress,
        vj,
        ctr,
        steps,
        ..
    } = *ctx;
    let _ = vj;
    EmitLoop {
        sink,
        cancel: Arc::clone(cancel),
        progress: Arc::clone(progress),
        jar_done: Arc::clone(&ctr.jar_done),
        lib_done: Arc::clone(&ctr.lib_done),
        asst_done: Arc::clone(&ctr.asst_done),
        natives_done: Arc::clone(&ctr.natives_done),
        jar_bytes: Arc::clone(&ctr.jar_bytes),
        jar_total: Arc::clone(&ctr.jar_total),
        bytes: Arc::clone(&ctr.bytes),
        lib_bytes: Arc::clone(&ctr.lib_step_bytes),
        asst_bytes: Arc::clone(&ctr.asst_step_bytes),
        natives_bytes: Arc::clone(&ctr.natives_step_bytes),
        lib_pending_bytes: scan.lib_total_bytes.saturating_sub(scan.lib_existing_bytes),
        asst_pending_bytes: scan.asst_pending_bytes,
        natives_pending_bytes: scan.natives_pending_bytes,
        loader_libs_bytes: Arc::clone(&ctr.loader_libs_step_bytes),
        loader_libs_total: Arc::clone(&ctr.loader_libs_step_total),
        optifine_bytes: Arc::clone(&ctr.optifine_step_bytes),
        optifine_total: Arc::clone(&ctr.optifine_step_total),
        fabric_bytes: Arc::clone(&ctr.fabric_step_bytes),
        fabric_total: Arc::clone(&ctr.fabric_step_total),
        jar_step: steps.jar,
        lib_step: steps.lib,
        asst_step: steps.asst,
        natives_step: steps.natives,
        loader_libs_step: steps.libs,
        optifine_step: steps.optifine,
        fabric_step: steps.fabric,
        cfg_done: Arc::clone(&ctr.cfg_done),
        libs_done: Arc::clone(&ctr.libs_done),
        optifine_done: Arc::clone(&ctr.optifine_done),
        fabric_done: Arc::clone(&ctr.fabric_done),
        lib_pending: scan.lib_pending,
        asst_pending: scan.asst_pending,
        natives_pending: scan.natives_pending,
        initial_completed: scan.initial_completed,
        total_files,
        initial_bytes: scan.initial_bytes,
        total_bytes: scan.total_bytes,
        jar_total_fallback,
    }
    .spawn()
}

async fn install_loader_step(
    ctx: &DownloadCtx<'_>,
    loader_choice: Option<&LoaderChoice>,
    mc_version: &str,
    parent_text: &str,
) -> Result<(), Vec<String>> {
    let DownloadCtx {
        app,
        cancel,
        progress,
        config,
        dir_name,
        vdir,
        ctr,
        steps,
        ..
    } = *ctx;
    if let Some(loader) = loader_choice {
        let hook = |ev: loader::LoaderEvent| match ev {
            loader::LoaderEvent::ConfigDone => {
                if let Some(i) = steps.cfg {
                    step_done(app, progress, i);
                }
                ctr.cfg_done.store(true, Ordering::Relaxed);
            }
            loader::LoaderEvent::LibsDone => {
                if let Some(i) = steps.libs {
                    step_done(app, progress, i);
                }
                ctr.libs_done.store(true, Ordering::Relaxed);
            }
        };
        loader::install_loader(
            loader.kind,
            mc_version,
            &loader.version,
            loader::VersionTarget {
                dir_name,
                vdir,
            },
            parent_text,
            loader::InstallCtx {
                config,
                pc: &ProgressCounters {
                    bytes: Arc::clone(&ctr.bytes),
                    step_bytes: Arc::clone(&ctr.loader_libs_step_bytes),
                    step_total: Arc::clone(&ctr.loader_libs_step_total),
                    ..Default::default()
                },
                // Forge/NeoForge（installer 式）用于 kill 子进程；Fabric/Quilt 忽略
                cancel,
                hook: Some(&hook),
            },
        )
        .await
        .map_err(|e| vec![e])?;
    }
    Ok(())
}

async fn install_optifine_step(
    ctx: &DownloadCtx<'_>,
    patch: Option<&str>,
    loader_choice: Option<&LoaderChoice>,
    mc_version: &str,
) -> Result<(), Vec<String>> {
    let Some(patch) = patch else {
        return Ok(());
    };
    let DownloadCtx {
        app,
        cancel,
        progress,
        config,
        dir_name,
        vdir,
        ctr,
        steps,
        ..
    } = *ctx;
    // OptiFine 场景随主 ModLoader：Fabric/Quilt → OptiFabric 桥接进 mods；Forge/NeoForge → 直接进 mods；无 loader → 纯净版独立版本 + classpath 库
    let mode = match loader_choice {
        Some(l) => match l.kind {
            task::LoaderKind::Fabric | task::LoaderKind::Quilt => {
                loader::OptifineMode::FabricLike
            }
            task::LoaderKind::Forge | task::LoaderKind::NeoForge => {
                loader::OptifineMode::ForgeLike
            }
        },
        None => loader::OptifineMode::Vanilla,
    };
    loader::install_optifine(
        mc_version,
        patch,
        "HD_U",
        loader::VersionTarget {
            dir_name,
            vdir,
        },
        mode,
        loader::InstallCtx {
            config,
            pc: &ProgressCounters {
                bytes: Arc::clone(&ctr.bytes),
                step_bytes: Arc::clone(&ctr.optifine_step_bytes),
                step_total: Arc::clone(&ctr.optifine_step_total),
                ..Default::default()
            },
            cancel,
            hook: None,
        },
    )
    .await
    .map_err(|e| vec![e])?;
    if let Some(i) = steps.optifine {
        step_done(app, progress, i);
    }
    ctr.optifine_done.store(true, Ordering::Relaxed);
    Ok(())
}

async fn download_fabric_api_step(
    ctx: &DownloadCtx<'_>,
    api: Option<&FabricApi>,
) -> Result<(), Vec<String>> {
    let Some(api) = api else {
        return Ok(());
    };
    let DownloadCtx {
        app,
        cancel,
        progress,
        vdir,
        ctr,
        steps,
        ..
    } = *ctx;
    let url = api.url.as_str();
    // URL 最后一段 percent-decode（文件名中的 `+` 被编码为 %2B）
    let filename =
        percent_encoding::percent_decode_str(util::url_file_name(url).unwrap_or("fabric-api.jar"))
            .decode_utf8_lossy()
            .into_owned();
    // 版本隔离：游戏从 gameDir/mods（= versions/<id>/mods）加载 mod
    let mods_dir = vdir.join("mods");
    std::fs::create_dir_all(&mods_dir).ok();
    let dest = mods_dir.join(filename);
    // 多源候选（Auto 探测官方 CDN / MCIM 镜像 TTFB）；fabric-api 只发布在 Modrinth/GitHub，maven 镜像无该构件，勿加 maven 候选（只会 404 污染错误信息）。
    let urls = util::community_download_urls(url).await;
    let progress_cb = ProgressCounters {
        bytes: Arc::clone(&ctr.bytes),
        step_bytes: Arc::clone(&ctr.fabric_step_bytes),
        step_total: Arc::clone(&ctr.fabric_step_total),
        ..Default::default()
    }
    .step_progress();
    fetcher::fetch_bounded_c(&urls, &dest, Some(progress_cb), Some(cancel))
        .await
        .map_err(|e| vec![format!("下载 Fabric API 失败\n{}", e)])?;
    if let Some(i) = steps.fabric {
        step_done(app, progress, i);
    }
    ctr.fabric_done.store(true, Ordering::Relaxed);
    Ok(())
}

/// 完整下载任务主体：version json → client.jar / libraries / assets → natives → ModLoader/OptiFine → Fabric API；配置由命令层校验后经 DownloadTask 传入。
pub async fn run_task(
    sink: Arc<dyn EventSink>,
    cancel: &Arc<AtomicBool>,
    progress: &Arc<Mutex<TaskProgress>>,
    task: &DownloadTask,
) {
    let app = sink.as_ref();
    eprintln!(
        "[task] 开始: id={} mc={} loader={:?} url={}",
        task.version_id, task.mc_version, task.loader, task.version_url
    );
    let runtime = match prepare_runtime(task) {
        Ok(rt) => rt,
        Err(e) => return set_error(app, progress, e),
    };

    let mut specs = task.steps();
    set_phase(
        app,
        progress,
        Some(TaskStatus::Running),
        &specs[0].name,
        0,
        StepStatus::Running,
    );
    if cancelled(cancel) {
        return set_cancelled(app, progress);
    }

    // ── 阶段 A：fetch version.json in-memory（唯一硬前置）──
    let (vj, parent_text) = match stage_fetch_version(app, progress, cancel, &runtime, task).await {
        Ok(v) => v,
        Err(()) => return,
    };

    let TaskRuntime {
        started: t0,
        config,
        sources,
        mc,
        dir_name,
        vdir,
        reuse_root,
        vdir_existed_before,
        _lock,
    } = runtime;
    let src_refs: Vec<&DownloadSource> = sources.iter().collect();

    let mc_version = &task.mc_version;
    let loader = task.loader.as_ref();
    let optifine_patch = task.optifine_patch.as_deref();
    let fabric_api = task.fabric_api.as_ref();

    // ── 阶段 B：本地缺失检测 + 步骤布局 + 总进度基线（含资产清单 fetch 与多线程 sha1 检测）──
    eprintln!("[task] 阶段B: 本地扫描开始");
    let lib_dir = mc.join("libraries");
    let scan = match init_local_scan(
        app,
        progress,
        ScanInput {
            vj: &vj,
            config: &config,
            mc: &mc,
            lib_dir: &lib_dir,
        },
        &mut specs,
        cancel,
    )
    .await
    {
        Ok(s) => s,
        Err(e) => {
            // 扫描被取消（count_assets_all 轮询返回）走取消收口，而非报错
            if cancelled(cancel) {
                eprintln!("[task] 阶段B 取消");
                return set_cancelled(app, progress);
            }
            eprintln!("[task] 阶段B 失败: {e}");
            return set_error(app, progress, e);
        }
    };
    eprintln!(
        "[task] 阶段B 完成 ({:.1}s): jar={} libs_pending={} asst_pending={} natives={}",
        t0.elapsed().as_secs_f64(),
        scan.jar_total,
        scan.lib_pending,
        scan.asst_pending,
        scan.natives_pending,
    );
    let mut total_files =
        1 + scan.jar_total + scan.lib_total_all + scan.asst_total_all + scan.natives_pending;
    if loader.is_some() {
        total_files += 2; // LoaderConfig + LoaderLibs
    }
    if optifine_patch.is_some() {
        total_files += 1;
    }
    if fabric_api.is_some() {
        total_files += 1;
    }
    set_total(app, progress, total_files, scan.total_bytes);
    set_completed(app, progress, scan.initial_completed, scan.initial_bytes);
    let ctr = Counters::default();
    let steps = StepIndex {
        jar: scan.jar_step,
        lib: scan.lib_step,
        asst: scan.asst_step,
        natives: scan.natives_step,
        cfg: scan.cfg_step,
        libs: scan.libs_step,
        optifine: scan.optifine_step,
        fabric: scan.fabric_step,
    };
    let ctx = DownloadCtx {
        app,
        cancel,
        progress,
        config: &config,
        sources: &sources,
        src_refs: &src_refs,
        reuse_root: reuse_root.as_ref(),
        vj: &vj,
        dir_name: &dir_name,
        vdir: &vdir,
        lib_dir: &lib_dir,
        ctr: &ctr,
        steps: &steps,
    };

    // 统一转 running（json 保持 done）；在 spawn EmitLoop 前完成，避免锁竞争
    for (i, s) in specs.iter().enumerate() {
        if s.kind != StepKind::VersionJson {
            set_phase(app, progress, None, &s.name, i, StepStatus::Running);
        }
    }

    let _emit_handle = spawn_emit_loop(
        &ctx,
        Arc::clone(&sink),
        &scan,
        total_files,
        vj.downloads.client.as_ref().map(|c| c.size).unwrap_or(0),
    );

    eprintln!(
        "[task] 阶段C: 并行下载开始 ({:.1}s)",
        t0.elapsed().as_secs_f64()
    );
    // 用 join! 而非 try_join!：后者短路后各 step 内部 spawn 的批处理 worker 仍在写盘，与失败整删目录竞争（Windows 句柄占用令删除失败/目录重建）；须等全部收尾再收口。
    let (jar, libs, assets, natives, loader_r, optifine_r, fabric_r) = tokio::join!(
        download_client_jar(&ctx),
        download_libraries_step(&ctx),
        download_assets_step(&ctx),
        extract_natives_step(&ctx),
        install_loader_step(&ctx, loader, mc_version, &parent_text),
        install_optifine_step(&ctx, optifine_patch, loader, mc_version),
        download_fabric_api_step(&ctx, fabric_api),
    );
    eprintln!("[task] 阶段C 并行结束 ({:.1}s)", t0.elapsed().as_secs_f64());

    if cancelled(cancel) {
        eprintln!("[task] 取消收口");
        return set_cancelled(app, progress);
    }
    let mut errors: Vec<String> = Vec::new();
    for r in [jar, libs, assets, natives, loader_r, optifine_r, fabric_r] {
        if let Err(mut e) = r {
            errors.append(&mut e);
        }
    }
    if !errors.is_empty() {
        // 全新下载失败整删残缺目录；补全（目录已存在）不删。登记清理路径由 set_error 后台整删。
        if !vdir_existed_before {
            let cleanup_id = progress.try_lock().map(|p| p.task_id.clone());
            if let Some(id) = cleanup_id {
                task::global().set_cleanup_path(&id, vdir.clone());
            }
        }
        eprintln!("[task] 失败: {}", errors.join("; "));
        return set_error(app, progress, errors.join("; "));
    }
    if cancelled(cancel) {
        eprintln!("[task] 取消收口(下载后)");
        return set_cancelled(app, progress);
    }

    // 收尾：显式设 100% 并发 done 终态事件（EmitLoop 封顶 99.9%，前端据此移除卡片并通知）
    eprintln!("[task] 完成 ({:.1}s)", t0.elapsed().as_secs_f64());
    set_done(app, progress, "完成");
}

/// 拉取并解析版本 JSON，保留原始文本（ModLoader 合并需保留未知字段）。
async fn fetch_version_json(
    version_url: &str,
    config: &DownloaderConfig,
) -> Result<(VersionJson, String), String> {
    let text = meta::fetch_version_json_text(version_url, config).await?;
    let vj: VersionJson =
        serde_json::from_str(&text).map_err(common_err_ctx!("解析 version json 失败"))?;
    Ok((vj, text))
}

struct TargetDir {
    dir_name: String,
    vdir: std::path::PathBuf,
}

/// 解析目标版本目录（不依赖 version JSON）：预指定（补全/修复）→ 原样；loader → `{mc}-{loader}-{ver}`；原版 → `versions/{id}` 冲突时 `{id}_{n}`（缺 client jar 的残缺目录复用重下）。
fn resolve_target_dir(task: &DownloadTask, mc: &std::path::Path) -> TargetDir {
    if let Some(pre) = task.version_dir_name.as_deref() {
        return TargetDir {
            dir_name: pre.to_string(),
            vdir: mc.join("versions").join(pre),
        };
    }
    if let Some(loader) = task.loader.as_ref() {
        let dir_name = format!(
            "{}-{}-{}",
            task.mc_version,
            loader.kind.name(),
            loader.version
        );
        return TargetDir {
            dir_name: dir_name.clone(),
            vdir: mc.join("versions").join(&dir_name),
        };
    }
    let (name, p) = resolve_version_dir(mc, &task.version_id);
    TargetDir {
        dir_name: name,
        vdir: p,
    }
}

struct LocalScan {
    lib_pending: u32,
    lib_total_all: u32,
    lib_total_bytes: u64,
    lib_existing_bytes: u64,
    jar_total: u32,
    natives_pending: u32,
    natives_pending_bytes: u64,
    asst_total_all: u32,
    asst_pending: u32,
    asst_pending_bytes: u64,
    jar_step: Option<usize>,
    lib_step: Option<usize>,
    asst_step: Option<usize>,
    natives_step: Option<usize>,
    cfg_step: Option<usize>,
    libs_step: Option<usize>,
    optifine_step: Option<usize>,
    fabric_step: Option<usize>,
    total_bytes: u64,
    initial_completed: u32,
    initial_bytes: u64,
}

struct ScanInput<'a> {
    vj: &'a VersionJson,
    config: &'a DownloaderConfig,
    mc: &'a std::path::Path,
    lib_dir: &'a std::path::Path,
}

/// 阶段 B：检测本地已有文件（库/jar/natives 快查 + 资产清单 fetch + 多线程 sha1），重建步骤列表（零缺失维度过滤）；资产清单 fetch 失败或 cancel 置位时返回 Err。
async fn init_local_scan(
    app: &dyn EventSink,
    progress: &Arc<Mutex<TaskProgress>>,
    scan: ScanInput<'_>,
    specs: &mut Vec<StepSpec>,
    cancel: &Arc<AtomicBool>,
) -> Result<LocalScan, String> {
    let ScanInput {
        vj,
        config,
        mc,
        lib_dir,
    } = scan;
    eprintln!("[task]   B1: 库/natives 统计");
    let lib_total_all = library::count_total(&vj.libraries) as u32;
    let lib_pending = library::count_pending(&vj.libraries, lib_dir) as u32;
    let lib_existing = lib_total_all - lib_pending;
    let lib_total_bytes = library::total_lib_bytes(&vj.libraries);
    let lib_existing_bytes = library::existing_lib_bytes(&vj.libraries, lib_dir);

    let jar_total = if vj.downloads.client.is_some() {
        1u32
    } else {
        0u32
    };
    let jar_total_bytes = vj.downloads.client.as_ref().map(|c| c.size).unwrap_or(0);

    // natives 缺失数预计算（下载阶段独立计数器；total 计入 pending 数）
    let natives_pending = count_natives_missing(&vj.libraries, lib_dir);
    let natives_pending_bytes = task::repair::natives_missing_bytes(&vj.libraries, lib_dir);

    // 资源清单：fetch asset index（网络）→ 报资产总数（文件+字节）→ 多线程 sha1 检测本地（逐批上报）
    let mut asst_total_all = 0u32;
    let mut asst_total_bytes = 0u64;
    task::update_and_emit(app, progress, |p| p.current_phase = "获取资源清单".into());
    eprintln!("[task]   B2: fetch asset index ({})", vj.asset_index.url);
    let (asst_pending, asst_pending_bytes) = count_assets_all(
        AssetIndexRef {
            url: &vj.asset_index.url,
            id: Some(&vj.asset_index.id),
            sha1: Some(&vj.asset_index.sha1),
        },
        config,
        mc,
        Some(cancel),
        |total, total_bytes| {
            asst_total_all = total;
            asst_total_bytes = total_bytes;
            eprintln!(
                "[assets] on_total: 清单 {} 文件 / {}B，emit 已入队",
                total, total_bytes
            );
            // 清单齐了：剩余=全量总数，已完成仅 version json（近似值，检测后覆盖为最终总数）
            set_total(
                app,
                progress,
                1 + jar_total + lib_total_all + total,
                1 + jar_total_bytes + lib_total_bytes + total_bytes,
            );
            eprintln!("[assets] on_total: set_total 完成");
            set_completed(app, progress, 1, 1);
            eprintln!("[assets] on_total: set_completed 完成");
            task::update_and_emit(app, progress, |p| {
                p.current_phase = "检测本地资源文件".into()
            });
            eprintln!("[assets] on_total: phase 更新完成");
        },
        |checked_files, checked_bytes| {
            // 检测中：已完成 = version json + 已存在库 + 已检测资产（文件与字节）
            set_completed(
                app,
                progress,
                1 + lib_existing + checked_files,
                1 + lib_existing_bytes + checked_bytes,
            );
        },
    )
    .await?;
    eprintln!(
        "[task]   B3: asset 扫描完成: total={} pending={} bytes={}",
        asst_total_all, asst_pending, asst_pending_bytes
    );
    let asst_existing = asst_total_all.saturating_sub(asst_pending);
    let asst_existing_bytes = asst_total_bytes.saturating_sub(asst_pending_bytes);
    set_completed(
        app,
        progress,
        1 + lib_existing + asst_existing,
        1 + lib_existing_bytes + asst_existing_bytes,
    );

    if lib_pending == 0 {
        specs.retain(|s| s.kind != StepKind::Libraries);
    }
    if asst_pending == 0 {
        specs.retain(|s| s.kind != StepKind::Assets);
    }
    task::update_and_emit(app, progress, |p| {
        p.steps = specs
            .iter()
            .enumerate()
            .map(|(i, s)| StepInfo {
                name: s.name.clone(),
                status: if i == 0 {
                    StepStatus::Done
                } else {
                    StepStatus::Pending
                },
                progress: if i == 0 { 100.0 } else { 0.0 },
            })
            .collect();
    });

    let pos = |k: StepKind| specs.iter().position(|s| s.kind == k);
    let initial_completed = 1 + lib_existing + asst_existing;
    let total_bytes = 1 + jar_total_bytes + lib_total_bytes + asst_total_bytes;
    let initial_bytes = 1 + lib_existing_bytes + asst_existing_bytes;

    Ok(LocalScan {
        lib_pending,
        lib_total_all,
        lib_total_bytes,
        lib_existing_bytes,
        jar_total,
        natives_pending,
        natives_pending_bytes,
        asst_total_all,
        asst_pending,
        asst_pending_bytes,
        jar_step: pos(StepKind::ClientJar),
        lib_step: pos(StepKind::Libraries),
        asst_step: pos(StepKind::Assets),
        natives_step: pos(StepKind::Natives),
        cfg_step: pos(StepKind::LoaderConfig),
        libs_step: pos(StepKind::LoaderLibs),
        optifine_step: pos(StepKind::OptiFine),
        fabric_step: pos(StepKind::FabricApi),
        total_bytes,
        initial_completed,
        initial_bytes,
    })
}

// JDK 安装任务在 task/jdk.rs；此处 re-export 兼容 commands/task.rs 的 runner:: 调用方。
