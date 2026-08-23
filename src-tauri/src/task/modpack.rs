// ── 整合包安装任务（单卡片：解析 → 本体 → 内容 全程一个任务）──
//
// 流程（同一 TaskManager 注册、同一 cancel token、同一步骤列表动态增长）：
//   A. 获取包体（在线源下载 / 本地文件）+ 解析清单 → PackPlan JSON
//   B. 游戏本体：版本 JSON → client.jar → 依赖库 → 资源 → 原生库 → ModLoader
//      （复用 library/assets/extract/mod_loader 领域函数——它们内部按 sha1
//        跳过已有文件，恢复场景免重下；因此无需 init_local_scan 级扫描）
//   C. 内容：计划内文件并发下载（sha1 校验）→ overrides/ 解压到版本目录
//
// 步骤表两段式：命令层创建时仅含 A 段占位，A 完成后重写为全量列表。
// 进度公式走"完成步骤数/总步骤数"（每个内容文件一步，粒度足够细）；
// 字节计数仅供速度/ETA 展示。

use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use tauri::AppHandle;

use crate::config::downloader::model::{DownloaderConfig, DownloadSource};
use crate::constants;
use crate::downloader::community;
use crate::downloader::fetcher;
use crate::downloader::modpack::{
    parse_curseforge_manifest, parse_modrinth_index, resolve_cf_files, sanitize_pack_name,
    safe_rel_path, ModpackRequest, OverridesRef, PackPlan, PackSource,
};
use crate::downloader::model::VersionJson;
use crate::downloader::progress::ProgressCounters;
use crate::downloader::reuse::LocalRuntimeSource;
use crate::task::progress::{
    cancelled, set_cancelled, set_error, set_phase, set_total, step_done, update_progress,
};
use crate::task::{global, update_and_emit, StepInfo, StepStatus, TaskProgress, TaskStatus};

/// 包体缓存目录（exe 旁 Elytra/modpacks/，与 Settings.toml 同根约定）。
fn cache_dir() -> PathBuf {
    crate::util::exe_dir()
        .join(constants::CONFIG_DIR)
        .join("modpacks")
}

/// FNV-1a 64：来源标识 → 短哈希（缓存键防同名覆盖；跨进程稳定）。
fn fnv1a64(data: &str) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in data.as_bytes() {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

/// 缓存键：`{净化包名}-{来源哈希8}`。
pub fn cache_key(request: &ModpackRequest) -> String {
    let seed = match &request.source {
        PackSource::Local { path } => path.clone(),
        PackSource::Online { url, .. } => url.clone(),
    };
    format!("{}-{:08x}", sanitize_pack_name(&request.name), fnv1a64(&seed))
}

/// 包体缓存路径（.mrpack/.zip 按来源扩展名）。
pub fn zip_cache_path(request: &ModpackRequest) -> PathBuf {
    let raw = match &request.source {
        PackSource::Local { path } => path.clone(),
        PackSource::Online { url, .. } => url.clone(),
    };
    let ext = if Path::new(raw.split('?').next().unwrap_or(&raw))
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("mrpack"))
    {
        ".mrpack"
    } else {
        ".zip"
    };
    cache_dir().join(format!("{}{}", cache_key(request), ext))
}

/// 计划 JSON 路径。
pub fn plan_path_for(request: &ModpackRequest) -> PathBuf {
    cache_dir().join(format!("{}.plan.json", cache_key(request)))
}

/// 命令层创建任务时的初始步骤（A 段；B/C 段在解析完成后动态追加）。
pub fn initial_steps(request: &ModpackRequest) -> Vec<&'static str> {
    match request.source {
        PackSource::Online { .. } => vec!["下载整合包", "解析整合包"],
        PackSource::Local { .. } => vec!["解析整合包"],
    }
}

/// zip 成员名 → overrides 内相对路径（剥离前缀；空/目录项返回 None）。
fn override_rel<'a>(member: &'a str, prefix: &str) -> Option<&'a str> {
    let rest = member.strip_prefix(prefix)?;
    (!rest.is_empty() && !rest.ends_with('/')).then_some(rest)
}

/// 版本目录名冲突消解：`versions/{base}` 已存在则追加 `_{n}`
/// （对齐 resolve_version_dir 的 `{id}_{n}` 语义）。
fn unique_version_dir(mc: &Path, base: &str) -> String {
    if !mc.join("versions").join(base).exists() {
        return base.to_string();
    }
    for n in 1.. {
        let candidate = format!("{base}_{n}");
        if !mc.join("versions").join(&candidate).exists() {
            return candidate;
        }
    }
    unreachable!()
}

/// MCIM 对 Modrinth CDN 的镜像 URL（对齐 runner.rs 社区资源路由；
/// 非 cdn.modrinth.com 仅官方一源）。
fn mirrors_for(url: &str) -> Vec<String> {
    let official = url.to_string();
    let mirrored = official.replace("https://cdn.modrinth.com", "https://mod.mcimirror.top");
    if mirrored == official {
        vec![official]
    } else {
        vec![official, mirrored]
    }
}

fn persist_config() -> DownloaderConfig {
    crate::persist::read(constants::SECT_DOWNLOADER).unwrap_or_default()
}

enum ManifestKind {
    Modrinth,
    CurseForge,
}

/// 从 zip 根读取清单文本（按成员名嗅探来源）。
fn read_manifest(
    archive: &mut zip::ZipArchive<std::fs::File>,
) -> Result<(String, ManifestKind), String> {
    for (name, kind) in [
        ("modrinth.index.json", ManifestKind::Modrinth),
        ("manifest.json", ManifestKind::CurseForge),
    ] {
        if let Ok(mut entry) = archive.by_name(name) {
            let mut text = String::new();
            entry
                .read_to_string(&mut text)
                .map_err(|e| format!("读取清单失败\n{e}"))?;
            return Ok((text, kind));
        }
    }
    Err("不是有效的整合包文件\n缺少 modrinth.index.json 或 manifest.json".into())
}

/// 读 zip 清单 → 构建 PackPlan（CF 走批量换链网络请求）。
async fn build_plan(
    zip_path: &Path,
    request: &ModpackRequest,
    config: &DownloaderConfig,
) -> Result<PackPlan, String> {
    let file = std::fs::File::open(zip_path).map_err(|e| format!("打开整合包失败\n{e}"))?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| format!("读取整合包失败\n{e}"))?;

    let (index_text, kind) = read_manifest(&mut archive)?;
    let (mut mc_version, loader, files, restricted) = match kind {
        ManifestKind::Modrinth => {
            let parsed = parse_modrinth_index(&index_text)?;
            (parsed.mc_version, parsed.loader, parsed.files, Vec::new())
        }
        ManifestKind::CurseForge => {
            let parsed = parse_curseforge_manifest(&index_text)?;
            let ids: Vec<i64> = parsed.files.iter().map(|(_, fid)| *fid).collect();
            // 批量换链（一次 POST 换全部直链；作者关闭第三方分发的文件进 restricted）
            let data = community::curseforge_files(&ids).await?;
            let (resolved, locked) = resolve_cf_files(&data, &parsed.files);
            (parsed.mc_version, parsed.loader, resolved, locked)
        }
    };
    mc_version = mc_version.trim().to_string();

    // 原版 version json URL（官方清单多源回退 + TTL 缓存）
    let manifest = crate::downloader::manifest::fetch_version_list(config).await?;
    let version_url = manifest
        .versions
        .iter()
        .find(|v| v.id == mc_version)
        .map(|v| v.url.clone())
        .ok_or_else(|| format!("官方源未收录该 Minecraft 版本\n{mc_version}"))?;

    // overrides 目录检测
    let has_overrides = (0..archive.len()).any(|i| {
        archive
            .by_index(i)
            .map(|e| e.name().starts_with("overrides/"))
            .unwrap_or(false)
    });

    if files.is_empty() && !has_overrides && restricted.is_empty() {
        return Err("整合包没有可安装的内容（无文件且无覆盖）".into());
    }
    // 目标路径穿越防护提前到计划期：坏条目解析期即报错，不带病安装
    for f in &files {
        safe_rel_path(&f.path)?;
    }

    let base_dir = format!("{}-{}", sanitize_pack_name(&request.name), mc_version);
    let version_dir_name = unique_version_dir(&config.minecraft_dir_abs(), &base_dir);

    Ok(PackPlan {
        name: request.name.clone(),
        mc_version,
        loader,
        version_url,
        version_dir_name,
        files,
        restricted,
        overrides: has_overrides.then(|| OverridesRef {
            zip_path: zip_path.display().to_string(),
            prefix: "overrides/".into(),
        }),
    })
}

fn write_plan(plan_path: &Path, plan: &PackPlan) -> Result<(), String> {
    if let Some(dir) = plan_path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string_pretty(plan).map_err(|e| e.to_string())?;
    std::fs::write(plan_path, json).map_err(|e| e.to_string())
}

// ── 单卡片执行体 ──

/// 共享计数器束（心跳与各阶段共用）。
struct Counters {
    /// 已完成步骤数（进度分子；每步 +1，内容文件粒度一步）
    done_steps: Arc<AtomicU32>,
    /// 累计下载字节（速度/ETA 展示用；进度百分比走步骤数公式）
    bytes: Arc<AtomicU64>,
    /// 总字节估算（版本 JSON 获取后补齐 assets/libraries/client 部分）
    est_bytes: Arc<AtomicU64>,
    /// 总步骤数
    total_steps: u32,
}

impl Counters {
    fn new(total_steps: u32, est_bytes: u64) -> Self {
        Self {
            done_steps: Arc::new(AtomicU32::new(0)),
            bytes: Arc::new(AtomicU64::new(0)),
            est_bytes: Arc::new(AtomicU64::new(est_bytes)),
            total_steps,
        }
    }

    fn counter(&self) -> ProgressCounters {
        ProgressCounters {
            files: Arc::new(AtomicU32::new(0)),
            bytes: Arc::clone(&self.bytes),
            step_bytes: Arc::new(AtomicU64::new(0)),
            step_total: Arc::new(AtomicU64::new(0)),
        }
    }
}

/// 完成一步：步骤状态置 done + 完成计数 +1。
fn finish_step(
    app: &AppHandle,
    progress: &Arc<Mutex<TaskProgress>>,
    idx: usize,
    c: &Counters,
) {
    step_done(app, progress, idx);
    c.done_steps.fetch_add(1, Ordering::Relaxed);
}

/// 失败收口：错误消息区分用户取消；全新目录随失败整删（对齐 run_task 语义，
/// 取消路径的删除由 TaskManager cleanup 移交异步处理，不在此重复删）。
fn fail_or_cancel(
    app: &AppHandle,
    progress: &Arc<Mutex<TaskProgress>>,
    cancel: &Arc<AtomicBool>,
    msg: String,
    vdir_for_cleanup: Option<(&Path, bool)>,
) -> bool {
    if cancelled(cancel) {
        set_cancelled(app, progress);
    } else {
        set_error(app, progress, msg);
        if let Some((vdir, existed_before)) = vdir_for_cleanup {
            if !existed_before {
                let _ = std::fs::remove_dir_all(vdir);
            }
        }
    }
    false
}

/// 成功收口（对齐既有 runner：Done 后自行 remove 注册）。
fn finish_ok(app: &AppHandle, progress: &Arc<Mutex<TaskProgress>>, phase: &str) {
    update_and_emit(app, progress, |p| {
        p.status = TaskStatus::Done;
        p.progress = 100.0;
        p.completed_files = p.total_files;
        p.current_phase = phase.into();
    });
    if let Some(p) = progress.try_lock() {
        let id = p.task_id.clone();
        drop(p);
        global().remove(&id);
    }
}

/// 重写步骤表：前 `done_count` 步置 done，其余 pending（A 段完成后追加 B/C 段）。
fn replace_steps(
    app: &AppHandle,
    progress: &Arc<Mutex<TaskProgress>>,
    names: &[String],
    done_count: usize,
) {
    update_and_emit(app, progress, |p| {
        p.steps = names
            .iter()
            .enumerate()
            .map(|(i, n)| StepInfo {
                name: n.clone(),
                status: if i < done_count {
                    StepStatus::Done
                } else {
                    StepStatus::Pending
                },
                progress: 0.0,
            })
            .collect();
        p.total_files = names.len() as u32;
    });
}

/// 安装主体（单卡片）。返回是否成功。
pub async fn run_install(
    app: &AppHandle,
    cancel: &Arc<AtomicBool>,
    progress: &Arc<Mutex<TaskProgress>>,
    request: &ModpackRequest,
) -> bool {
    let config = persist_config();
    let online = matches!(request.source, PackSource::Online { .. });
    let zip_path = zip_cache_path(request);

    // ── A1：获取包体（幂等：本地缓存 sha1 命中直接跳过）──
    let mut pack_bytes_est = 0u64;
    if let PackSource::Online { url, sha1, size } = &request.source {
        set_phase(app, progress, Some(TaskStatus::Running), "下载整合包", 0, StepStatus::Running);
        pack_bytes_est = *size;
        let cached =
            !sha1.is_empty() && zip_path.is_file() && crate::downloader::hash::file_sha1_matches(&zip_path, sha1);
        if !cached {
            let last = Arc::new(AtomicU64::new(0));
            let cb_last = last.clone();
            // 字节增量直接进共享计数（此时心跳未启动，仅为 est 校准积累）
            let _ = &cb_last;
            if let Err(e) = fetcher::fetch_verified_c(
                &mirrors_for(url),
                &zip_path,
                None::<fn(u64, u64)>,
                sha1,
                Some(cancel),
            )
            .await
            {
                return fail_or_cancel(app, progress, cancel, format!("下载整合包失败\n{e}"), None);
            }
            pack_bytes_est = std::fs::metadata(&zip_path).map(|m| m.len()).unwrap_or(*size);
        }
        step_done(app, progress, 0);
    }
    if cancelled(cancel) {
        set_cancelled(app, progress);
        return false;
    }
    if pack_bytes_est == 0 {
        pack_bytes_est = std::fs::metadata(&zip_path).map(|m| m.len()).unwrap_or(0);
    }

    // ── A2：解析清单并生成计划 ──
    let parse_idx = usize::from(online);
    set_phase(
        app,
        progress,
        Some(TaskStatus::Running),
        "解析整合包",
        parse_idx,
        StepStatus::Running,
    );
    let plan = match build_plan(&zip_path, request, &config).await {
        Ok(p) => p,
        Err(e) => return fail_or_cancel(app, progress, cancel, e, None),
    };
    if let Err(e) = write_plan(&plan_path_for(request), &plan) {
        return fail_or_cancel(app, progress, cancel, format!("写入安装计划失败\n{e}"), None);
    }
    step_done(app, progress, parse_idx);
    let a_done = parse_idx + 1; // A 段已完成步骤数

    // ── 重建完整步骤表（B 段本体 + C 段内容）──
    let mut names: Vec<String> = vec![
        "版本清单".into(),
        "游戏主文件".into(),
        "依赖库".into(),
        "资源文件".into(),
        "原生库".into(),
    ];
    let (i_json, i_jar, i_libs, i_asst, i_nat) = (0, 1, 2, 3, 4);
    let (i_cfg, i_lbs) = if plan.loader.is_some() {
        let label = plan.loader.as_ref().map(|l| l.kind.display_name()).unwrap_or("");
        let cfg = names.len();
        names.push(format!("{label} 配置"));
        let lbs = names.len();
        names.push(format!("{label} 文件"));
        (Some(cfg), Some(lbs))
    } else {
        (None, None)
    };
    let content_start = names.len();
    for f in &plan.files {
        names.push(format!("下载 {}", f.path));
    }
    let i_overrides = plan.overrides.as_ref().map(|_| {
        let idx = names.len();
        names.push("解压覆盖文件".into());
        idx
    });

    let mut full_names: Vec<String> = initial_steps(request).iter().map(|s| s.to_string()).collect();
    full_names.extend(names.iter().cloned());
    replace_steps(app, progress, &full_names, a_done);

    // 总字节估算（版本清单获取后补齐 B 段部分）
    let content_bytes: u64 = plan.files.iter().map(|f| f.size).sum();
    let override_bytes = plan.overrides.as_ref().map(sum_override_bytes).unwrap_or(0);
    let c = Counters::new(
        full_names.len() as u32,
        pack_bytes_est + content_bytes + override_bytes,
    );
    set_total(app, progress, c.total_steps, 0);

    // 心跳：150ms 刷新计数/速度/ETA；进度百分比由 update_progress 按
    // completed_files/total_files（即完成步骤数）计算
    let emit_handle = tauri::async_runtime::spawn({
        let app = app.clone();
        let progress = progress.clone();
        let done_steps = c.done_steps.clone();
        let bytes = c.bytes.clone();
        let est_bytes = c.est_bytes.clone();
        let total_steps = c.total_steps;
        async move {
            let mut tick = tokio::time::interval(Duration::from_millis(150));
            let mut last_bytes = 0u64;
            let mut last_sample = Instant::now();
            let mut speed_ema: f64 = 0.0;
            loop {
                tick.tick().await;
                let cur = bytes.load(Ordering::Relaxed);
                let est = est_bytes.load(Ordering::Relaxed);
                let now = Instant::now();
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
                update_and_emit(&app, &progress, |p| {
                    p.status = TaskStatus::Running;
                    p.completed_files = done_steps.load(Ordering::Relaxed);
                    p.total_files = total_steps;
                    p.completed_bytes = cur;
                    p.total_bytes = est;
                    p.speed_bps = speed_ema;
                    let remaining = est.saturating_sub(cur);
                    p.eta_seconds = (speed_ema >= 1024.0 && remaining > 0)
                        .then(|| (remaining as f64 / speed_ema).ceil() as u64);
                    update_progress(p);
                });
            }
        }
    });

    // ── 目标目录：预指定名保证解析期/安装期一致 ──
    let vdir = config.minecraft_dir_abs().join("versions").join(&plan.version_dir_name);
    let vdir_existed_before = vdir.exists();
    std::fs::create_dir_all(&vdir).ok();
    if !vdir_existed_before {
        // 取消时整删本次新建的版本目录（对齐 run_task）
        let tid = progress.lock().task_id.clone();
        global().set_cleanup_path(&tid, vdir.clone());
    }
    let cleanup = (vdir.as_path(), vdir_existed_before);
    let lib_dir = config.minecraft_dir_abs().join("libraries");
    let reuse_root = LocalRuntimeSource::discover(&vdir);

    // ── A3：项目图标持久化（装饰性内容：缺失/下载失败静默跳过，不阻塞安装）──
    if let Some(icon) = request.icon_url.as_deref().filter(|s| !s.trim().is_empty()) {
        let dest = vdir.join("pack-icon.png");
        if !dest.exists() {
            // 空 sha1 跳过校验；取消感知；MCIM 双源回退对 cdn.modrinth.com 适用
            let _ =
                fetcher::fetch_verified_c(&mirrors_for(icon), &dest, None::<fn(u64, u64)>, "", Some(cancel)).await;
        }
    }
    let sources: Vec<DownloadSource> = config.file_sources_list().into_iter().cloned().collect();
    let src_refs: Vec<&DownloadSource> = sources.iter().collect();

    // ── B1：版本清单（唯一硬前置）──
    set_phase(app, progress, None, "版本清单", i_json, StepStatus::Running);
    let parent_text = match crate::downloader::version::fetch_version_json_text(&plan.version_url, &config).await {
        Ok(t) => t,
        Err(e) => {
            emit_handle.abort();
            return fail_or_cancel(app, progress, cancel, format!("拉取版本清单失败\n{e}"), Some(cleanup));
        }
    };
    let vj: VersionJson = match serde_json::from_str(&parent_text) {
        Ok(v) => v,
        Err(e) => {
            emit_handle.abort();
            return fail_or_cancel(app, progress, cancel, format!("解析版本清单失败\n{e}"), Some(cleanup));
        }
    };
    // 补齐 B 段总量估算：client + 库 + 资源索引声明总量
    let lib_est: u64 = vj
        .libraries
        .iter()
        .filter_map(|l| l.downloads.as_ref().and_then(|d| d.artifact.as_ref()))
        .map(|a| a.size)
        .sum();
    c.est_bytes.fetch_add(
        vj.downloads.client.as_ref().map(|cl| cl.size).unwrap_or(0)
            + lib_est
            + vj.asset_index.total_size,
        Ordering::Relaxed,
    );
    // 无 loader 时写原版 JSON（loader 模式由 installer 写合并 JSON）。
    // 写盘失败必须报错：假成功会让版本缺 json、启动时报"未找到版本 JSON"。
    if plan.loader.is_none() {
        let json = serde_json::to_string_pretty(&vj)
            .map_err(|e| format!("序列化版本 JSON 失败\n{e}"));
        match json {
            Ok(json) => {
                let jp = vdir.join(format!("{}.json", plan.version_dir_name));
                if let Err(e) = std::fs::write(&jp, &json) {
                    emit_handle.abort();
                    return fail_or_cancel(
                        app,
                        progress,
                        cancel,
                        format!("写入版本 JSON 失败\n{e}"),
                        Some(cleanup),
                    );
                }
            }
            Err(e) => {
                emit_handle.abort();
                return fail_or_cancel(app, progress, cancel, e, Some(cleanup));
            }
        }
    }
    finish_step(app, progress, i_json, &c);
    if cancelled(cancel) {
        emit_handle.abort();
        return fail_or_cancel(app, progress, cancel, String::new(), Some(cleanup));
    }

    // ── B2：client.jar ──
    set_phase(app, progress, None, "游戏主文件", i_jar, StepStatus::Running);
    if let Some(cl) = &vj.downloads.client {
        let jar_path = vdir.join(format!("{}.jar", plan.version_dir_name));
        let fresh = !(jar_path.exists() && crate::downloader::hash::file_sha1_matches(&jar_path, &cl.sha1));
        if fresh {
            let urls: Vec<String> = sources.iter().map(|src| src.mirror_url(&cl.url)).collect();
            if let Err(e) = fetcher::fetch_verified_c(&urls, &jar_path, None::<fn(u64, u64)>, &cl.sha1, Some(cancel)).await {
                emit_handle.abort();
                return fail_or_cancel(app, progress, cancel, format!("下载游戏主文件失败\n{e}"), Some(cleanup));
            }
            c.bytes.fetch_add(cl.size, Ordering::Relaxed);
        }
    }
    finish_step(app, progress, i_jar, &c);
    if cancelled(cancel) {
        emit_handle.abort();
        return fail_or_cancel(app, progress, cancel, String::new(), Some(cleanup));
    }

    // ── B3/B4/B5：依赖库 / 资源 / 原生库（领域函数内部 sha1 跳过已有文件；
    //     内部并发由 config.concurrency 驱动，字节进共享计数）──
    set_phase(app, progress, None, "依赖库", i_libs, StepStatus::Running);
    if let Err(e) = crate::downloader::library::download_libraries(
        &vj.libraries,
        &lib_dir,
        config.concurrency as usize,
        &src_refs,
        reuse_root.clone(),
        &c.counter(),
        Some(cancel),
    )
    .await
    {
        emit_handle.abort();
        return fail_or_cancel(app, progress, cancel, format!("下载依赖库失败\n{}", e.join("; ")), Some(cleanup));
    }
    finish_step(app, progress, i_libs, &c);

    set_phase(app, progress, None, "资源文件", i_asst, StepStatus::Running);
    if let Err(e) = crate::downloader::assets::download_assets(
        &vj.asset_index.url,
        &vj.asset_index.id,
        &vj.asset_index.sha1,
        &config,
        config.concurrency as usize,
        reuse_root.clone(),
        &c.counter(),
        Some(cancel),
    )
    .await
    {
        emit_handle.abort();
        return fail_or_cancel(app, progress, cancel, format!("下载资源文件失败\n{}", e.join("; ")), Some(cleanup));
    }
    finish_step(app, progress, i_asst, &c);

    set_phase(app, progress, None, "原生库", i_nat, StepStatus::Running);
    if let Err(e) = crate::downloader::extract::process_natives(
        &vj.libraries,
        &lib_dir,
        &src_refs,
        &c.counter(),
        Some(cancel),
    )
    .await
    {
        emit_handle.abort();
        return fail_or_cancel(app, progress, cancel, format!("处理原生库失败\n{}", e.join("; ")), Some(cleanup));
    }
    finish_step(app, progress, i_nat, &c);

    // ── B6：ModLoader（钩子推进 配置/文件 两步）──
    if let Some(loader) = &plan.loader {
        let i_cfg_ref = i_cfg;
        let i_lbs_ref = i_lbs;

        // 钩子只置内存标志；步骤推进由下方轮询循环完成（钩子非 async，
        // 不能触碰 progress 锁；install_* 借用外部状态，与主循环同作用域轮询）。
        use crate::downloader::mod_loader::{self, LoaderEvent};
        let cfg_flag = AtomicBool::new(false);
        let lbs_flag = AtomicBool::new(false);
        let hook = |ev: LoaderEvent| match ev {
            LoaderEvent::ConfigDone => cfg_flag.store(true, Ordering::Relaxed),
            LoaderEvent::LibsDone => lbs_flag.store(true, Ordering::Relaxed),
        };
        let counters = c.counter();
        let install_fut = async {
            match loader.kind {
                crate::task::LoaderKind::Fabric => {
                    mod_loader::install_fabric(&plan.mc_version, &loader.version, &plan.version_dir_name, &vdir, &parent_text, &config, &counters, Some(&hook)).await
                }
                crate::task::LoaderKind::Quilt => {
                    mod_loader::install_quilt(&plan.mc_version, &loader.version, &plan.version_dir_name, &vdir, &parent_text, &config, &counters, Some(&hook)).await
                }
                crate::task::LoaderKind::Forge => {
                    mod_loader::install_forge(&plan.mc_version, &loader.version, &plan.version_dir_name, &vdir, &parent_text, &config, &counters, cancel, Some(&hook)).await
                }
                crate::task::LoaderKind::NeoForge => {
                    mod_loader::install_neoforge(&loader.version, &plan.version_dir_name, &vdir, &parent_text, &config, &counters, cancel, Some(&hook)).await
                }
            }
        };
        let mut install_fut = Box::pin(install_fut);
        let mut cfg_marked = false;
        let mut lbs_marked = false;
        loop {
            tokio::select! {
                biased;
                _ = tokio::time::sleep(Duration::from_millis(200)) => {
                    if !cfg_marked && cfg_flag.load(Ordering::Relaxed) {
                        if let Some(idx) = i_cfg_ref { finish_step(app, progress, idx, &c); cfg_marked = true; }
                    }
                    if !lbs_marked && lbs_flag.load(Ordering::Relaxed) {
                        if let Some(idx) = i_lbs_ref { finish_step(app, progress, idx, &c); lbs_marked = true; }
                    }
                }
                res = &mut install_fut => {
                    // 收尾：确保两步都标记（安装器可能不发 LibsDone 之外的事件序）
                    if !cfg_marked { if let Some(idx) = i_cfg_ref { finish_step(app, progress, idx, &c); } }
                    if !lbs_marked { if let Some(idx) = i_lbs_ref { finish_step(app, progress, idx, &c); } }
                    if let Err(e) = res {
                        emit_handle.abort();
                        return fail_or_cancel(app, progress, cancel, format!("ModLoader 安装失败\n{e}"), Some(cleanup));
                    }
                    break;
                }
            }
            if cancelled(cancel) {
                emit_handle.abort();
                return fail_or_cancel(app, progress, cancel, String::new(), Some(cleanup));
            }
        }
    }

    // ── C1：内容文件并发下载 ──
    let concurrency = (config.concurrency as usize).clamp(1, 8);
    for (chunk_no, chunk) in plan.files.chunks(concurrency).enumerate() {
        if cancelled(cancel) {
            emit_handle.abort();
            return fail_or_cancel(app, progress, cancel, String::new(), Some(cleanup));
        }
        let mut set = tokio::task::JoinSet::new();
        for (offset, f) in chunk.iter().enumerate() {
            let idx = content_start + chunk_no * concurrency + offset;
            let rel_path = match safe_rel_path(&f.path) {
                Ok(rel) => rel,
                Err(e) => {
                    emit_handle.abort();
                    return fail_or_cancel(app, progress, cancel, e, Some(cleanup));
                }
            };
            let dest = vdir.join(&rel_path);
            let tmp = dest.with_file_name(format!(
                "{}.elytra-part",
                dest.file_name().unwrap_or_default().to_string_lossy()
            ));
            if let Some(parent) = dest.parent() {
                if let Err(e) = std::fs::create_dir_all(parent) {
                    emit_handle.abort();
                    return fail_or_cancel(
                        app,
                        progress,
                        cancel,
                        format!("创建目录失败\n{e}\n{}", parent.display()),
                        Some(cleanup),
                    );
                }
            }
            set_phase(app, progress, None, &format!("下载 {}", f.path), idx, StepStatus::Running);
            let urls = mirrors_for(&f.url);
            let sha1 = f.sha1.clone();
            let label = f.path.clone();
            let cancel = cancel.clone();
            let bytes = c.bytes.clone();
            set.spawn(async move {
                // 幂等：目标已存在且 sha1 命中 → 跳过（恢复场景零重下）
                if !sha1.is_empty()
                    && dest.is_file()
                    && crate::downloader::hash::file_sha1_matches(&dest, &sha1)
                {
                    return Ok((idx, 0u64));
                }
                if cancelled(&cancel) {
                    return Ok((idx, 0u64));
                }
                let last = Arc::new(AtomicU64::new(0));
                let cb_last = last.clone();
                let cb_bytes = bytes.clone();
                let cb = move |done: u64, _total: u64| {
                    let prev = cb_last.swap(done, Ordering::Relaxed);
                    cb_bytes.fetch_add(done.saturating_sub(prev), Ordering::Relaxed);
                };
                let result =
                    fetcher::fetch_verified_c(&urls, &tmp, Some(cb), &sha1, Some(&cancel)).await;
                match result {
                    Ok(_) => {
                        std::fs::rename(&tmp, &dest)
                            .map_err(|e| (idx, format!("落盘失败\n{e}\n{}", dest.display())))?;
                        Ok((idx, last.load(Ordering::Relaxed)))
                    }
                    Err(e) => {
                        let _ = std::fs::remove_file(&tmp);
                        Err((idx, format!("下载 {label} 失败\n{e}")))
                    }
                }
            });
        }
        while let Some(joined) = set.join_next().await {
            match joined {
                Ok(Ok((idx, _))) => {
                    finish_step(app, progress, idx, &c);
                }
                Ok(Err((_, msg))) => {
                    emit_handle.abort();
                    return fail_or_cancel(app, progress, cancel, msg, Some(cleanup));
                }
                Err(e) => {
                    emit_handle.abort();
                    return fail_or_cancel(app, progress, cancel, format!("下载工作线程异常\n{e}"), Some(cleanup));
                }
            }
        }
    }

    // ── C2：overrides 解压 ──
    if let (Some(ov), Some(idx)) = (&plan.overrides, i_overrides) {
        set_phase(app, progress, None, "解压覆盖文件", idx, StepStatus::Running);
        if let Err(e) = extract_overrides(ov, &vdir) {
            emit_handle.abort();
            return fail_or_cancel(app, progress, cancel, e, Some(cleanup));
        }
        finish_step(app, progress, idx, &c);
    }
    emit_handle.abort();

    // ── 受限文件收口：其余内容已全部就位，明确列出需手动下载的文件 ──
    if !plan.restricted.is_empty() {
        let mut msg = format!(
            "已安装完成，但 {} 个文件因 CurseForge 作者关闭第三方分发无法自动下载（需手动从官网下载后放入 mods 目录）：",
            plan.restricted.len()
        );
        for name in plan.restricted.iter().take(20) {
            msg.push_str(&format!("\n· {name}"));
        }
        if plan.restricted.len() > 20 {
            msg.push_str(&format!("\n…等共 {} 个", plan.restricted.len()));
        }
        return fail_or_cancel(app, progress, cancel, msg, Some(cleanup));
    }

    finish_ok(app, progress, "完成");
    true
}

/// 解压 overrides/ 到版本目录根（词法穿越防护对齐 jdk safe_runtime_path）。
fn extract_overrides(ov: &OverridesRef, vdir: &Path) -> Result<(), String> {
    let file = std::fs::File::open(&ov.zip_path).map_err(|e| format!("打开整合包缓存失败\n{e}"))?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| format!("读取整合包缓存失败\n{e}"))?;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| format!("读取条目失败\n{e}"))?;
        let Some(rel) = override_rel(entry.name(), &ov.prefix) else {
            continue;
        };
        let out = vdir.join(safe_rel_path(rel)?);
        if entry.is_dir() {
            std::fs::create_dir_all(&out).map_err(|e| format!("创建目录失败\n{e}"))?;
            continue;
        }
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("创建目录失败\n{e}"))?;
        }
        let mut writer =
            std::fs::File::create(&out).map_err(|e| format!("写出失败\n{e}\n{}", out.display()))?;
        std::io::copy(&mut entry, &mut writer)
            .map_err(|e| format!("解压失败\n{e}\n{}", out.display()))?;
    }
    Ok(())
}

/// overrides 解压总量（未压缩字节，进度基线用；读不到按 0）。
fn sum_override_bytes(ov: &OverridesRef) -> u64 {
    let Ok(file) = std::fs::File::open(&ov.zip_path) else {
        return 0;
    };
    let Ok(mut archive) = zip::ZipArchive::new(file) else {
        return 0;
    };
    let mut total = 0u64;
    for i in 0..archive.len() {
        let Ok(entry) = archive.by_index(i) else {
            continue;
        };
        if override_rel(entry.name(), &ov.prefix).is_some() {
            total += entry.size();
        }
    }
    total
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write as _;

    fn opts() -> zip::write::SimpleFileOptions {
        zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored)
    }

    #[test]
    fn override_rel_strips_prefix_and_skips_dirs() {
        assert_eq!(override_rel("overrides/mods/a.jar", "overrides/"), Some("mods/a.jar"));
        assert_eq!(override_rel("overrides/", "overrides/"), None);
        assert_eq!(override_rel("meta/other", "overrides/"), None);
    }

    /// 构造测试 zip：指定清单名 + overrides 内容。
    fn write_test_zip(path: &Path, manifest_name: &str) {
        let file = std::fs::File::create(path).unwrap();
        let mut w = zip::ZipWriter::new(file);
        w.start_file(manifest_name, opts()).unwrap();
        w.write_all(b"{}").unwrap();
        w.start_file("overrides/mods/a.jar", opts()).unwrap();
        w.write_all(b"jar-bytes").unwrap();
        w.start_file("overrides/config/opt.cfg", opts()).unwrap();
        w.write_all(b"k=v").unwrap();
        w.finish().unwrap();
    }

    #[test]
    fn read_manifest_sniffs_both_formats() {
        let dir = tempfile::tempdir().unwrap();
        let mrpack = dir.path().join("a.mrpack");
        write_test_zip(&mrpack, "modrinth.index.json");
        let mut ar = zip::ZipArchive::new(std::fs::File::open(&mrpack).unwrap()).unwrap();
        let (text, kind) = read_manifest(&mut ar).unwrap();
        assert_eq!(text, "{}");
        assert!(!matches!(kind, ManifestKind::CurseForge));

        let cfzip = dir.path().join("b.zip");
        write_test_zip(&cfzip, "manifest.json");
        let mut ar2 = zip::ZipArchive::new(std::fs::File::open(&cfzip).unwrap()).unwrap();
        assert!(matches!(read_manifest(&mut ar2).unwrap().1, ManifestKind::CurseForge));

        // 无清单 → 明确报错
        let plain = dir.path().join("c.zip");
        write_test_zip(&plain, "other.txt");
        let mut ar3 = zip::ZipArchive::new(std::fs::File::open(&plain).unwrap()).unwrap();
        assert!(read_manifest(&mut ar3).is_err());
    }

    #[test]
    fn extract_overrides_writes_files_and_blocks_traversal() {
        let dir = tempfile::tempdir().unwrap();
        let zip_path = dir.path().join("evil.zip");
        let file = std::fs::File::create(&zip_path).unwrap();
        let mut w = zip::ZipWriter::new(file);
        w.start_file("overrides/mods/ok.jar", opts()).unwrap();
        w.write_all(b"a").unwrap();
        w.start_file("overrides/../escape.txt", opts()).unwrap();
        w.write_all(b"x").unwrap();
        w.finish().unwrap();

        let vdir = tempfile::tempdir().unwrap();
        let ov = OverridesRef {
            zip_path: zip_path.display().to_string(),
            prefix: "overrides/".into(),
        };
        let err = extract_overrides(&ov, vdir.path()).unwrap_err();
        assert!(err.contains(".."), "穿越条目应报错: {err}");
        // 前序合法条目已落盘；恶意条目未逃逸出版本目录
        assert!(vdir.path().join("mods").join("ok.jar").is_file());
        assert!(!vdir.path().join("escape.txt").exists());
    }

    #[test]
    fn unique_version_dir_suffixes_conflicts() {
        let mc = tempfile::tempdir().unwrap();
        assert_eq!(unique_version_dir(mc.path(), "Pack-1.20.1"), "Pack-1.20.1");
        std::fs::create_dir_all(mc.path().join("versions").join("Pack-1.20.1")).unwrap();
        assert_eq!(unique_version_dir(mc.path(), "Pack-1.20.1"), "Pack-1.20.1_1");
    }

    #[test]
    fn cache_key_differs_by_source_and_is_stable() {
        let online = ModpackRequest {
            name: "Demo".into(),
            icon_url: None,
            source: PackSource::Online { url: "https://x/a.mrpack".into(), sha1: String::new(), size: 0 },
        };
        let local = ModpackRequest {
            name: "Demo".into(),
            icon_url: None,
            source: PackSource::Local { path: "C:/a.mrpack".into() },
        };
        assert_ne!(cache_key(&online), cache_key(&local), "同名不同来源键应不同");
        assert_eq!(cache_key(&online), cache_key(&online.clone()), "同来源键稳定");
    }

    #[test]
    fn initial_steps_match_source_kind() {
        let online = ModpackRequest {
            name: "D".into(),
            icon_url: None,
            source: PackSource::Online { url: "u".into(), sha1: String::new(), size: 0 },
        };
        let local = ModpackRequest {
            name: "D".into(),
            icon_url: None,
            source: PackSource::Local { path: "p".into() },
        };
        assert_eq!(initial_steps(&online), vec!["下载整合包", "解析整合包"]);
        assert_eq!(initial_steps(&local), vec!["解析整合包"]);
    }
}
