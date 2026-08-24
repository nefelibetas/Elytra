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

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;
use tauri::AppHandle;

use crate::config::downloader::model::{DownloaderConfig, DownloadSource};
use crate::constants;
use crate::downloader::fetcher;
use crate::downloader::modpack::{
    cache_key, build_plan, safe_rel_path, ModpackRequest, OverridesRef, PackFile, PackPlan,
    PackSource,
};
use crate::downloader::model::VersionJson;
use crate::downloader::progress::ProgressCounters;
use crate::downloader::reuse::LocalRuntimeSource;
use crate::task::progress::{
    cancelled, set_cancelled, set_error, set_phase, set_total, spawn_heartbeat, step_done,
    HeartbeatStats,
};
use crate::util::mcim_mirror_urls;
use crate::task::{global, update_and_emit, StepInfo, StepStatus, TaskProgress, TaskStatus};

/// 包体缓存目录（exe 旁 Elytra/modpacks/，与 Settings.toml 同根约定）。
fn cache_dir() -> PathBuf {
    crate::util::exe_dir()
        .join(constants::CONFIG_DIR)
        .join("modpacks")
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

fn persist_config() -> DownloaderConfig {
    crate::persist::read(constants::SECT_DOWNLOADER).unwrap_or_default()
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

/// 取消感知包装：底层操作不支持取消参数（纯网络 JSON/同步 IO）时，
/// 与轮询取消令牌赛跑——先到者胜；落败方 future 被 drop（连接随之中断）。
async fn race_cancel<F, T>(cancel: &Arc<AtomicBool>, fut: F) -> Result<T, String>
where
    F: std::future::Future<Output = Result<T, String>>,
{
    tokio::select! {
        r = fut => r,
        _ = async {
            loop {
                if cancelled(cancel) {
                    return;
                }
                tokio::time::sleep(Duration::from_millis(120)).await;
            }
        } => Err("任务已取消".into()),
    }
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
        // 强校验优先；清单未提供 sha1 时降级为体积弱校验——与 fetch_verified_c
        // 空 sha1 跳过网络校验的信任模型一致，恢复场景不再整包重下
        let cached = zip_path.is_file()
            && match (sha1.is_empty(), *size) {
                (false, _) => crate::downloader::hash::file_sha1_matches(&zip_path, sha1),
                (true, s) if s > 0 => {
                    std::fs::metadata(&zip_path).map(|m| m.len()).ok() == Some(s)
                }
                _ => false,
            };
        if !cached {
            if let Err(e) = fetcher::fetch_verified_c(
                &mcim_mirror_urls(url),
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
    let plan = match race_cancel(cancel, build_plan(&zip_path, request, &config)).await {
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

    // 心跳：150ms 共享心跳刷新计数/速度/ETA（est_total_bytes 用 c.est_bytes，
    // 版本清单获取后仍可运行中增长）；进度百分比由 update_progress 按
    // completed_files/total_files（即完成步骤数）计算。收尾前 abort。
    let emit_handle = spawn_heartbeat(
        app,
        progress,
        HeartbeatStats {
            done_files: Arc::clone(&c.done_steps),
            total_files: c.total_steps,
            bytes: Arc::clone(&c.bytes),
            est_total_bytes: Arc::clone(&c.est_bytes),
        },
    );
    // ── 目标目录：预指定名保证解析期/安装期一致 ──
    let vdir = config.minecraft_dir_abs().join("versions").join(&plan.version_dir_name);
    let vdir_existed_before = vdir.exists();
    std::fs::create_dir_all(&vdir).ok();
    // 版本身份标记：重装/更新同名整合包时据此复用目录（内容 sha1 幂等跳过生效）
    let _ = std::fs::write(
        vdir.join("elytra-pack.json"),
        serde_json::json!({ "cache_key": cache_key(request) }).to_string(),
    );
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
                fetcher::fetch_verified_c(&mcim_mirror_urls(icon), &dest, None::<fn(u64, u64)>, "", Some(cancel)).await;
        }
    }
    let sources: Vec<DownloadSource> = config.file_sources_list().into_iter().cloned().collect();
    let src_refs: Vec<&DownloadSource> = sources.iter().collect();

    // ── B1：版本清单（唯一硬前置）──
    set_phase(app, progress, None, "版本清单", i_json, StepStatus::Running);
    let parent_text = match race_cancel(
        cancel,
        crate::downloader::version::fetch_version_json_text(&plan.version_url, &config),
    )
    .await {
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
            // 取消不在此处提前 return：install_fut 内部轮询同一 cancel 令牌，
            // 会杀掉 installer 子进程并自行返回 Err，随后由上方 Err 分支
            // 经 fail_or_cancel 走取消收口——中途 drop future 会遗留孤儿 JVM 进程
        }
    }

    // ── C0：包内资源复用 ──
    // 部分整合包会把 mod 物理捆绑在 overrides/ 内（常见于作者关闭 CurseForge
    // 第三方分发的场景）。清单内文件先与包内同名条目做 sha1 对比：一致则直接
    // 解包免下载；受限文件若已捆绑 → 就地落盘并从提示清单移除。
    let mut restricted = plan.restricted.clone();
    let mut pending: Vec<(usize, &PackFile)> = Vec::new();
    // C0 内部错误统一走此收口（run_install 返回 bool，不能用 ?）
    macro_rules! bail {
        ($e:expr) => {{
            emit_handle.abort();
            return fail_or_cancel(app, progress, cancel, $e, Some(cleanup));
        }};
    }
    {
        let mut pack_zip = plan.overrides.as_ref().and_then(|ov| {
            std::fs::File::open(&ov.zip_path)
                .ok()
                .and_then(|f| zip::ZipArchive::new(f).ok())
        });
        let index: std::collections::HashMap<String, String> = match pack_zip.as_mut() {
            Some(ar) => (0..ar.len())
                .filter_map(|i| ar.by_index(i).ok().map(|e| e.name().to_string()))
                .filter_map(|n| {
                    override_rel(&n, "overrides/")?;
                    Some((n["overrides/".len()..].to_string(), n))
                })
                .collect(),
            None => std::collections::HashMap::new(),
        };

        for (i, f) in plan.files.iter().enumerate() {
            let step_idx = content_start + i;
            let bundled = index.get(&f.path).and_then(|member| {
                let ar = pack_zip.as_mut()?;
                match (&f.sha1, member_sha1(ar, member)) {
                    // 有期望哈希：一致才采信包内副本；不一致以清单 URL 为权威源
                    (expected, Some(actual)) if expected.is_empty() || expected == &actual => {
                        Some(member.clone())
                    }
                    _ => None,
                }
            });
            match bundled {
                Some(member) => {
                    set_phase(
                        app,
                        progress,
                        None,
                        &format!("取自整合包 {}", f.path),
                        step_idx,
                        StepStatus::Running,
                    );
                    let rel_path = match safe_rel_path(&f.path) {
                        Ok(rel) => rel,
                        Err(e) => bail!(e),
                    };
                    let dest = vdir.join(&rel_path);
                    let res = match pack_zip.as_mut() {
                        Some(ar) => extract_member(ar, &member, &dest),
                        None => Ok(()),
                    };
                    if let Err(e) = res {
                        bail!(format!("释放包内资源失败\n{e}"));
                    }
                    c.est_bytes.fetch_sub(f.size, Ordering::Relaxed);
                    finish_step(app, progress, step_idx, &c);
                }
                None => pending.push((step_idx, f)),
            }
        }

        // 受限文件就地解决：捆绑副本按声明路径落盘（内容来自包作者本人）
        if !restricted.is_empty() && !index.is_empty() {
            for name in restricted.clone() {
                let candidate = format!("mods/{name}");
                let Some(member) = index.get(&candidate) else {
                    continue;
                };
                let Ok(rel) = safe_rel_path(&name) else {
                    continue;
                };
                let dest = vdir.join("mods").join(rel);
                if let Some(ar) = pack_zip.as_mut() {
                    if let Err(e) = extract_member(ar, member, &dest) {
                        bail!(e);
                    }
                    restricted.retain(|r| *r != name);
                }
            }
        }
    }

    // ── C1：内容文件并发下载 ──
    let concurrency = (config.concurrency as usize).clamp(1, 8);
    for chunk in pending.chunks(concurrency) {
        if cancelled(cancel) {
            emit_handle.abort();
            return fail_or_cancel(app, progress, cancel, String::new(), Some(cleanup));
        }
        let mut set = tokio::task::JoinSet::new();
        for entry in chunk.iter() {
            let idx = entry.0;
            let f: &PackFile = entry.1;
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
            let urls = mcim_mirror_urls(&f.url);
            let sha1 = f.sha1.clone();
            let label = f.path.clone();
            let cancel = cancel.clone();
            let bytes = c.bytes.clone();
            let size = f.size;
            set.spawn(async move {
                // 幂等：目标已存在且校验命中 → 跳过（恢复场景零重下）。
                // sha1 缺失时降级为体积匹配（信任模型同上）
                let hit = dest.is_file()
                    && if !sha1.is_empty() {
                        crate::downloader::hash::file_sha1_matches(&dest, &sha1)
                    } else {
                        size > 0
                            && std::fs::metadata(&dest).map(|m| m.len()).unwrap_or(0) == size
                    };
                if hit {
                    return Ok((idx, 0u64));
                }
                if cancelled(&cancel) {
                    return Ok((idx, 0u64));
                }
                // 字节回调走共享工厂（本地水位 + 增量累加进任务总字节）。
                // step_bytes 无消费者（本任务进度百分比由步骤数公式驱动），计数被丢弃；
                // 无需知道落盘字节数，故无需保留 worker 本地水位。
                let counters = ProgressCounters {
                    files: Arc::new(AtomicU32::new(0)),
                    bytes,
                    step_bytes: Arc::new(AtomicU64::new(0)),
                    step_total: Arc::new(AtomicU64::new(0)),
                };
                let cb = counters.byte_callback();
                let result =
                    fetcher::fetch_verified_c(&urls, &tmp, Some(cb), &sha1, Some(&cancel)).await;
                match result {
                    Ok(_) => {
                        std::fs::rename(&tmp, &dest)
                            .map_err(|e| (idx, format!("落盘失败\n{e}\n{}", dest.display())))?;
                        Ok((idx, 0u64))
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
        let ov = ov.clone();
        let vd = vdir.clone();
        if let Err(e) =
            race_cancel(cancel, async move {
                tokio::task::spawn_blocking(move || extract_overrides(&ov, &vd))
                    .await
                    .map_err(|e| format!("解压工作线程异常\n{e}"))?
            })
            .await
        {
            emit_handle.abort();
            return fail_or_cancel(app, progress, cancel, e, Some(cleanup));
        }
        finish_step(app, progress, idx, &c);
    }
    emit_handle.abort();

    // ── 受限文件收口：其余内容已全部就位，明确列出需手动下载的文件 ──
    if !restricted.is_empty() {
        let mut msg = format!(
            "已安装完成，但 {} 个文件因 CurseForge 作者关闭第三方分发无法自动下载（需手动从官网下载后放入 mods 目录）：",
            restricted.len()
        );
        for name in restricted.iter().take(20) {
            msg.push_str(&format!("\n· {name}"));
        }
        if restricted.len() > 20 {
            msg.push_str(&format!("\n…等共 {} 个", restricted.len()));
        }
        return fail_or_cancel(app, progress, cancel, msg, Some(cleanup));
    }

    finish_ok(app, progress, "完成");
    true
}

/// zip 成员流式 sha1（None=读取失败）。用于包内捆绑资源与清单声明的比对。
fn member_sha1(archive: &mut zip::ZipArchive<std::fs::File>, name: &str) -> Option<String> {
    let mut entry = archive.by_name(name).ok()?;
    crate::downloader::hash::reader_sha1_hex(&mut entry)
}

/// 解压单个 zip 成员到目标路径（词法穿越防护；父目录自动创建）。
fn extract_member(
    archive: &mut zip::ZipArchive<std::fs::File>,
    name: &str,
    out: &Path,
) -> Result<(), String> {
    let mut entry = archive.by_name(name).map_err(|e| format!("读取包内资源失败\n{e}"))?;
    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("创建目录失败\n{e}"))?;
    }
    let mut writer =
        std::fs::File::create(out).map_err(|e| format!("写出失败\n{e}\n{}", out.display()))?;
    std::io::copy(&mut entry, &mut writer)
        .map_err(|e| format!("解压失败\n{e}\n{}", out.display()))?;
    Ok(())
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
    #[test]
    fn member_sha1_and_extract_member_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let zip_path = dir.path().join("a.mrpack");
        write_test_zip(&zip_path, "modrinth.index.json"); // overrides/mods/a.jar
        let mut ar =
            zip::ZipArchive::new(std::fs::File::open(&zip_path).unwrap()).unwrap();

        let expected = crate::downloader::hash::sha1_hex(b"jar-bytes");
        assert_eq!(member_sha1(&mut ar, "overrides/mods/a.jar").as_deref(), Some(expected.as_str()));
        assert_eq!(member_sha1(&mut ar, "overrides/missing.jar"), None);

        // 单成员解压：内容逐字节还原到目标路径
        let out = dir.path().join("out").join("dup.jar");
        extract_member(&mut ar, "overrides/mods/a.jar", &out).unwrap();
        assert_eq!(std::fs::read(&out).unwrap(), b"jar-bytes");
    }


}
