use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tauri::{AppHandle, Emitter};

use crate::config::downloader::model::{DownloadSource, DownloaderConfig};
use crate::constants;
use crate::downloader::fetcher;
use crate::downloader::mod_loader;
use crate::downloader::model::VersionJson;
use crate::persist;
use crate::task::{self, TaskProgress};

const STEP_VERJSON: usize = 0;
const STEP_CLIENT: usize = 1;
const STEP_LIBS: usize = 2;
const STEP_ASSETS: usize = 3;

/// 扫描结果：缺失文件统计（补全文件功能，前端展示用）。
#[derive(Debug, Clone, serde::Serialize)]
pub struct MissingReport {
    /// 是否缺失客户端 client.jar
    pub client_jar: bool,
    /// 缺失的支持库数量
    pub libraries: u32,
    /// 缺失的资源文件数量
    pub assets: u32,
    /// 缺失/损坏的 native classifier JAR 数量（sha1 校验）
    pub natives: u32,
}

/// 读取版本目录的 version json（按文件名扫描，不假设文件名=目录名）。
/// JSON 是修复的唯一数据源（libs/assets/natives 清单都在里面），缺失时无法本地重建：
/// 原版目录可能被重命名过、loader 目录的 json 是 installer 合成的，只能删除重装。
fn read_version_json(vdir: &std::path::Path) -> Result<VersionJson, String> {
    let vj_path =
        crate::config::launcher::args::find_file_in_dir(vdir, "json").ok_or_else(|| {
            format!(
                "版本 JSON 缺失\n目录 {} 下未找到 JSON 文件，无法修复，请删除该版本后重新下载安装",
                vdir.display()
            )
        })?;
    let text = std::fs::read_to_string(&vj_path).map_err(|e| format!("读取版本文件失败：{e}"))?;
    serde_json::from_str(&text).map_err(|e| {
        format!(
            "版本 JSON 损坏\n{} 解析失败: {e}，请删除该版本后重新下载安装",
            vj_path.display()
        )
    })
}

/// 统计缺失/损坏的 native classifier JAR 数（sha1 校验，与 process_natives 同遍历逻辑）。
fn count_natives_missing(libraries: &[crate::downloader::model::Library], lib_dir: &Path) -> u32 {
    use crate::downloader::extract::legacy_classifier_artifact;
    use crate::downloader::hash::file_sha1_matches;
    libraries
        .iter()
        .filter(|lib| {
            // 仅旧式 natives 参与；新式作为普通库由 count_pending 统计
            let Some(artifact) = legacy_classifier_artifact(lib) else {
                return false;
            };
            let dest = lib_dir.join(&artifact.path);
            !(dest.exists() && file_sha1_matches(&dest, &artifact.sha1))
        })
        .count() as u32
}

/// 扫描版本缺失文件（client.jar / 支持库 / 资源 / natives），不发起下载。
/// 支持库与资源按本地 sha1 比对，资源按 assets index 与本地缓存比对（多线程）。
pub async fn scan_missing(version_dir_name: &str) -> Result<MissingReport, String> {
    use crate::downloader::library;

    let config: DownloaderConfig = persist::read(constants::SECT_DOWNLOADER).unwrap_or_default();
    let mc = config.minecraft_dir_abs();
    let vdir = mc.join("versions").join(version_dir_name);
    let vj = read_version_json(&vdir)?;

    // 1. client.jar（扫描目录内实际 jar；0 字节或 sha1 不匹配视为缺失/损坏）
    let jar_missing = match crate::config::launcher::args::find_file_in_dir(&vdir, "jar") {
        Some(p) => {
            let len_zero = p.metadata().map(|m| m.len() == 0).unwrap_or(true);
            if len_zero {
                true
            } else {
                vj.downloads
                    .client
                    .as_ref()
                    .map(|c| !crate::downloader::hash::file_sha1_matches(&p, &c.sha1))
                    .unwrap_or(false)
            }
        }
        None => true,
    };

    // 2. 支持库（规则过滤 + 架构匹配，与 pre_launch_check 一致）
    let lib_dir = mc.join("libraries");
    let lib_pending = library::count_pending(&vj.libraries, &lib_dir) as u32;

    // 3. natives（classifier JAR 存在且 sha1 匹配才算完整）
    let natives_pending = count_natives_missing(&vj.libraries, &lib_dir);

    // 4. 资源（sha1 比对本地缓存，多线程；带 index id/sha1 走本地缓存，避免重复拉取；
    //    拉取/解析失败显式报错，不再静默视为"完整"）
    let (asst_pending, _) = count_assets_all(
        &vj.asset_index.url,
        Some(&vj.asset_index.id),
        Some(&vj.asset_index.sha1),
        &config,
        &mc,
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

/// 按缺失情况动态生成修复任务步骤（与 run_repair_task 的索引构造顺序一致）。
pub fn repair_steps(report: &MissingReport) -> Vec<String> {
    let mut steps = Vec::new();
    if report.client_jar {
        steps.push("下载客户端文件".to_string());
    }
    if report.libraries > 0 {
        steps.push("下载支持库文件".to_string());
    }
    if report.assets > 0 {
        steps.push("下载资源文件".to_string());
    }
    if report.natives > 0 {
        steps.push("处理 native 库".to_string());
    }
    steps
}

/// 补全任务主体：基于本地 version json 下载缺失文件，进度经 task-progress 事件推送。
/// `report` 为提交时扫描结果（步骤数与其一致），运行中按对应步骤索引更新。
pub async fn run_repair_task(
    app: AppHandle,
    cancel: &Arc<AtomicBool>,
    progress: &Arc<Mutex<TaskProgress>>,
    version_dir_name: &str,
    report: MissingReport,
) {
    let config: DownloaderConfig = persist::read(constants::SECT_DOWNLOADER).unwrap_or_default();
    let sources: Vec<DownloadSource> = config.file_sources_list().into_iter().cloned().collect();
    let src_refs: Vec<&DownloadSource> = sources.iter().collect();
    let mc = config.minecraft_dir_abs();
    let vdir = mc.join("versions").join(version_dir_name);
    let vj = match read_version_json(&vdir) {
        Ok(v) => v,
        Err(e) => return set_error(&app, progress, e),
    };

    let jar_missing = report.client_jar;
    let lib_pending = report.libraries;
    let asst_pending = report.assets;
    let natives_pending = report.natives;
    let total_files =
        if jar_missing { 1u32 } else { 0u32 } + lib_pending + asst_pending + natives_pending;
    println!(
        "[repair] {} missing: jar={} libs={} assets={} natives={} total={}",
        version_dir_name, jar_missing, lib_pending, asst_pending, natives_pending, total_files
    );
    if total_files == 0 {
        return task::update_and_emit(&app, progress, |p| {
            p.status = "done".into();
            p.progress = 100.0;
            p.current_phase = "文件完整，无需修复".into();
        });
    }

    // 动态步骤 → 各阶段在 steps 中的索引（与 submit 生成的顺序一致）
    let mut idx = 0usize;
    let jar_idx = if jar_missing {
        let i = idx;
        idx += 1;
        Some(i)
    } else {
        None
    };
    let libs_idx = if lib_pending > 0 {
        let i = idx;
        idx += 1;
        Some(i)
    } else {
        None
    };
    let asst_idx = if asst_pending > 0 {
        let i = idx;
        idx += 1;
        Some(i)
    } else {
        None
    };
    // natives 恒为最后一个步骤，无需推进 idx
    let natives_idx = (natives_pending > 0).then_some(idx);

    set_total(&app, progress, total_files, 0);
    set_completed(&app, progress, 0, 0);
    if let Some(i) = jar_idx {
        set_phase(&app, progress, "running", "修复中", i, "running");
    }

    let jar_counter = Arc::new(AtomicU32::new(0));
    let lib_counter = Arc::new(AtomicU32::new(0));
    let asst_counter = Arc::new(AtomicU32::new(0));
    // jar 字节独立计数（下载回调是覆盖式），libs/assets 字节为累加式
    let jar_bytes = Arc::new(AtomicU64::new(0));
    let jar_total = Arc::new(AtomicU64::new(0));
    let bytes_counter = Arc::<AtomicU64>::new(AtomicU64::new(0));

    let emit_handle = EmitLoop {
        app: app.clone(),
        cancel: Arc::clone(cancel),
        progress: Arc::clone(progress),
        jar_done: Arc::clone(&jar_counter),
        lib_done: Arc::clone(&lib_counter),
        asst_done: Arc::clone(&asst_counter),
        jar_bytes: Arc::clone(&jar_bytes),
        jar_total: Arc::clone(&jar_total),
        bytes: Arc::clone(&bytes_counter),
        jar_step: jar_idx,
        lib_step: libs_idx,
        asst_step: asst_idx,
        lib_pending,
        asst_pending,
        initial_completed: 0,
        total_files,
        initial_bytes: 0,
        total_bytes: 0,
        jar_total_fallback: 0,
    }
    .spawn();

    let jar = async {
        if jar_missing {
            // 清理目录内损坏（0 字节或 sha1 不匹配）的 jar，避免与下载目标并存
            if let Some(bad) = crate::config::launcher::args::find_file_in_dir(&vdir, "jar") {
                let bad_jar = bad.metadata().map(|m| m.len() == 0).unwrap_or(true)
                    || vj
                        .downloads
                        .client
                        .as_ref()
                        .map(|c| !crate::downloader::hash::file_sha1_matches(&bad, &c.sha1))
                        .unwrap_or(false);
                if bad_jar {
                    let _ = std::fs::remove_file(&bad);
                }
            }
            if let Some(ref c) = vj.downloads.client {
                let urls: Vec<String> = sources.iter().map(|src| src.mirror_url(&c.url)).collect();
                let jar_path = vdir.join(format!("{version_dir_name}.jar"));
                let jbc2 = Arc::clone(&jar_bytes);
                let jt2 = Arc::clone(&jar_total);
                if c.size > 0 {
                    jt2.store(c.size, Ordering::Relaxed);
                }
                let cb = move |downloaded: u64, total: u64| {
                    jbc2.store(downloaded, Ordering::Relaxed);
                    if total > 0 {
                        jt2.store(total, Ordering::Relaxed);
                    }
                };
                fetcher::fetch_fallback(&urls, &jar_path, Some(cb))
                    .await
                    .map_err(|e| vec![e.to_string()])?;
                if !c.sha1.is_empty()
                    && !crate::downloader::hash::file_sha1_matches(&jar_path, &c.sha1)
                {
                    let _ = std::fs::remove_file(&jar_path);
                    return Err(vec!["client.jar 校验失败（sha1 不匹配）".to_string()]);
                }
            }
            jar_counter.store(1, Ordering::Relaxed);
        }
        Ok::<_, Vec<String>>(())
    };

    let lib_dir = mc.join("libraries");
    let libs = crate::downloader::library::download_libraries(
        &vj.libraries,
        &lib_dir,
        config.concurrency as usize,
        &src_refs,
        Arc::clone(&lib_counter),
        Arc::clone(&bytes_counter),
    );

    let asst = crate::downloader::assets::download_assets(
        &vj.asset_index.url,
        &vj.asset_index.id,
        &vj.asset_index.sha1,
        &config,
        config.concurrency as usize,
        Arc::clone(&asst_counter),
        Arc::clone(&bytes_counter),
    );

    let parallel = tokio::try_join!(jar, libs, asst);
    emit_handle.abort();

    if cancelled(cancel) {
        return set_cancelled(&app, progress);
    }
    if let Err(errors) = parallel {
        println!(
            "[repair] {} download failed: {}",
            version_dir_name,
            errors.join("; ")
        );
        return set_error(&app, progress, errors.join("; "));
    }

    // 各下载步骤标记完成（按实际索引）
    if let Some(i) = jar_idx {
        step_done(&app, progress, i);
    }
    if let Some(i) = libs_idx {
        step_done(&app, progress, i);
    }
    if let Some(i) = asst_idx {
        step_done(&app, progress, i);
    }

    // natives（幂等：已解压跳过；仅报告有缺失时才执行，与 steps 索引对应）
    if let Some(i) = natives_idx {
        set_phase(&app, progress, "", "处理 native 库", i, "running");
        if let Err(errors) = crate::downloader::extract::process_natives(
            &vj.libraries,
            &lib_dir,
            &src_refs,
            Arc::clone(&lib_counter),
            Arc::clone(&bytes_counter),
        )
        .await
        {
            println!(
                "[repair] {} natives failed: {}",
                version_dir_name,
                errors.join("; ")
            );
            return set_error(&app, progress, errors.join("; "));
        }
        step_done(&app, progress, i);
    }

    task::update_and_emit(&app, progress, |p| {
        p.status = "done".into();
        p.completed_files = p.total_files;
        p.progress = 100.0;
        p.current_phase = "修复完成".into();
    });
}

/// 完整下载任务主体：version json → client.jar / libraries / assets → natives → ModLoader/OptiFine → Fabric API。
#[allow(clippy::too_many_arguments)]
pub async fn run_task(
    app: AppHandle,
    cancel: &Arc<AtomicBool>,
    progress: &Arc<Mutex<TaskProgress>>,
    version_id: &str,
    version_url: &str,
    mc_version: &str,
    main_loader: Option<&str>,
    main_loader_version: Option<&str>,
    optifine_patch: Option<&str>,
    version_dir_name: Option<String>,
    fabric_api_url: Option<&str>,
    fabric_api_version: Option<&str>,
) {
    let config: DownloaderConfig = persist::read(constants::SECT_DOWNLOADER).unwrap_or_default();
    let sources: Vec<DownloadSource> = config.file_sources_list().into_iter().cloned().collect();
    let src_refs: Vec<&DownloadSource> = sources.iter().collect();
    let mc = config.minecraft_dir_abs();

    set_phase(
        &app,
        progress,
        "running",
        "下载原版json文件",
        STEP_VERJSON,
        "running",
    );
    if cancelled(cancel) {
        return set_cancelled(&app, progress);
    }

    // ── Step 0: fetch version.json in-memory ──
    let (vj, parent_text) = match fetch_version_json(version_url, &config).await {
        Ok(v) => v,
        Err(e) => return set_error(&app, progress, e),
    };
    step_done(&app, progress, STEP_VERJSON);
    if cancelled(cancel) {
        return set_cancelled(&app, progress);
    }

    // Determine target dir & register cleanup
    let dir_name: String;
    let vdir: std::path::PathBuf;
    if let Some(ref pre) = version_dir_name {
        dir_name = pre.clone();
        vdir = mc.join("versions").join(&dir_name);
    } else if let (Some(ld), Some(lv)) = (main_loader, main_loader_version) {
        dir_name = format!("{}-{}-{}", mc_version, ld, lv);
        vdir = mc.join("versions").join(&dir_name);
    } else {
        let (name, p) = crate::downloader::resolve_version_dir(&mc, version_id);
        dir_name = name;
        vdir = p;
    }

    // 确保目录存在；非 loader 模式下写入原版 version JSON（loader 模式由 installer 写合并 JSON）
    std::fs::create_dir_all(&vdir).ok();
    if main_loader.is_none() {
        if let Ok(json) = serde_json::to_string_pretty(&vj) {
            let jp = vdir.join(format!("{}.json", version_id));
            std::fs::write(&jp, &json).ok();
        }
    }
    task::global().set_cleanup_path(&progress.try_lock().unwrap().task_id, vdir.clone());

    // ── 检测本地已有文件：先给全量总数（剩余=大数字），检测中实时上报已完成字节，避免 UI 死等 ──
    let lib_dir = mc.join("libraries");
    let lib_total_all = crate::downloader::library::count_total(&vj.libraries) as u32;
    let lib_pending = crate::downloader::library::count_pending(&vj.libraries, &lib_dir) as u32;
    let lib_existing = lib_total_all - lib_pending;
    let lib_total_bytes = crate::downloader::library::total_lib_bytes(&vj.libraries);
    let lib_existing_bytes =
        crate::downloader::library::existing_lib_bytes(&vj.libraries, &lib_dir);

    let jar_exists = vj.downloads.client.is_some();
    let jar_total = if jar_exists { 1u32 } else { 0u32 };
    let jar_total_bytes = vj.downloads.client.as_ref().map(|c| c.size).unwrap_or(0);

    // 资源清单：fetch asset index（网络）→ 报资产总数（文件+字节）；再多线程 sha1 检测本地（逐批上报）
    let mut asst_total_all = 0u32;
    let mut asst_total_bytes = 0u64;
    task::update_and_emit(&app, progress, |p| p.current_phase = "获取资源清单".into());
    let (asst_pending, asst_pending_bytes) = match count_assets_all(
        &vj.asset_index.url,
        Some(&vj.asset_index.id),
        Some(&vj.asset_index.sha1),
        &config,
        &mc,
        |total, total_bytes| {
            asst_total_all = total;
            asst_total_bytes = total_bytes;
            // 清单齐了：剩余=全量总数，已完成仅 version json
            set_total(
                &app,
                progress,
                1 + jar_total + lib_total_all + total,
                1 + jar_total_bytes + lib_total_bytes + total_bytes,
            );
            set_completed(&app, progress, 1, 1);
            task::update_and_emit(&app, progress, |p| {
                p.current_phase = "检测本地资源文件".into()
            });
        },
        |checked_files, checked_bytes| {
            // 检测中：已完成 = version json + 已存在库 + 已检测资产（文件与字节）
            set_completed(
                &app,
                progress,
                1 + lib_existing + checked_files,
                1 + lib_existing_bytes + checked_bytes,
            );
        },
    )
    .await
    {
        Ok(v) => v,
        Err(e) => return set_error(&app, progress, e),
    };
    let asst_existing = asst_total_all.saturating_sub(asst_pending);
    let asst_existing_bytes = asst_total_bytes.saturating_sub(asst_pending_bytes);
    set_completed(
        &app,
        progress,
        1 + lib_existing + asst_existing,
        1 + lib_existing_bytes + asst_existing_bytes,
    );

    // 收尾步骤（natives/ModLoader/OptiFine/Fabric API）按步骤计入总文件数，
    // 否则下载完成时剩余文件提前归零、进度虚高 100%（收尾期间仍显示"剩余 0"）
    let mut total_files = 1 + jar_total + lib_total_all + asst_total_all;
    total_files += 1; // natives（恒执行）
    if main_loader.is_some() && main_loader_version.is_some() {
        total_files += 1;
    }
    if optifine_patch.is_some() {
        total_files += 1;
    }
    if fabric_api_version.is_some() {
        total_files += 1;
    }
    let initial_completed = 1 + lib_existing + asst_existing;
    let total_bytes = 1 + jar_total_bytes + lib_total_bytes + asst_total_bytes;
    let initial_bytes = 1 + lib_existing_bytes + asst_existing_bytes;

    // ── Parallel phase: client.jar + libraries + assets ──
    let jar_counter = Arc::new(AtomicU32::new(0));
    let jar_bytes_counter = Arc::new(AtomicU64::new(0));
    let jar_total_size = Arc::new(AtomicU64::new(0));
    let lib_counter = Arc::new(AtomicU32::new(0));
    let asst_counter = Arc::new(AtomicU32::new(0));
    let bytes_counter = Arc::<AtomicU64>::new(AtomicU64::new(0));

    // Set all phases to running BEFORE spawning emit loop to avoid lock races
    set_phase(
        &app,
        progress,
        "",
        "下载原版客户端文件",
        STEP_CLIENT,
        "running",
    );
    set_phase(
        &app,
        progress,
        "",
        "下载原版支持库文件",
        STEP_LIBS,
        "running",
    );
    set_phase(
        &app,
        progress,
        "",
        "下载原版资源文件",
        STEP_ASSETS,
        "running",
    );

    let emit_handle = EmitLoop {
        app: app.clone(),
        cancel: Arc::clone(cancel),
        progress: Arc::clone(progress),
        jar_done: Arc::clone(&jar_counter),
        lib_done: Arc::clone(&lib_counter),
        asst_done: Arc::clone(&asst_counter),
        jar_bytes: Arc::clone(&jar_bytes_counter),
        jar_total: Arc::clone(&jar_total_size),
        bytes: Arc::clone(&bytes_counter),
        jar_step: Some(STEP_CLIENT),
        lib_step: Some(STEP_LIBS),
        asst_step: Some(STEP_ASSETS),
        lib_pending,
        asst_pending,
        initial_completed,
        total_files,
        initial_bytes,
        total_bytes,
        jar_total_fallback: vj.downloads.client.as_ref().map(|c| c.size).unwrap_or(0),
    }
    .spawn();

    let jar = async {
        if let Some(ref c) = vj.downloads.client {
            let urls: Vec<String> = sources.iter().map(|src| src.mirror_url(&c.url)).collect();
            let jar_path = vdir.join(format!("{}.jar", dir_name));
            let jbytes = Arc::clone(&jar_bytes_counter);
            let jar_total = Arc::clone(&jar_total_size);
            let cb = move |downloaded: u64, total: u64| {
                // 只写 jar 自己的字节计数；不碰 bytes_counter（那是 libs/assets 的增量累加，
                // 覆盖写会冲掉已累加值导致速度跳变）
                jbytes.store(downloaded, Ordering::Relaxed);
                if total > 0 {
                    jar_total.store(total, Ordering::Relaxed);
                }
            };
            fetcher::fetch_fallback(&urls, &jar_path, Some(cb))
                .await
                .map_err(|e| vec![e.to_string()])?;
            // client.jar sha1 完整性校验（防止镜像 CDN 返回不完整/错误内容）
            if !c.sha1.is_empty() && !crate::downloader::hash::file_sha1_matches(&jar_path, &c.sha1)
            {
                let _ = std::fs::remove_file(&jar_path);
                return Err(vec!["client.jar 校验失败（sha1 不匹配）".to_string()]);
            }
            jar_counter.store(1, Ordering::Relaxed);
        }
        Ok::<_, Vec<String>>(())
    };

    let lib_dir = mc.join("libraries");
    let libs = crate::downloader::library::download_libraries(
        &vj.libraries,
        &lib_dir,
        config.concurrency as usize,
        &src_refs,
        Arc::clone(&lib_counter),
        Arc::clone(&bytes_counter),
    );

    let asst = crate::downloader::assets::download_assets(
        &vj.asset_index.url,
        &vj.asset_index.id,
        &vj.asset_index.sha1,
        &config,
        config.concurrency as usize,
        Arc::clone(&asst_counter),
        Arc::clone(&bytes_counter),
    );

    let parallel = tokio::try_join!(jar, libs, asst);
    emit_handle.abort();

    // 失败/取消优先上报，避免先发 100% 完成误导前端
    if cancelled(cancel) {
        return set_cancelled(&app, progress);
    }
    if let Err(errors) = parallel {
        return set_error(&app, progress, errors.join("; "));
    }
    if cancelled(cancel) {
        return set_cancelled(&app, progress);
    }

    // Emit final state after downloads complete (emit loop may have been aborted mid-cycle)
    {
        let mut p = progress.lock().unwrap();
        // 收尾步骤（natives/ModLoader/OptiFine/Fabric API）尚未执行：
        // completed 只计已完成的下载（不再顶格，剩余文件保持真实）
        p.completed_files = initial_completed
            + jar_counter.load(Ordering::Relaxed)
            + lib_counter.load(Ordering::Relaxed)
            + asst_counter.load(Ordering::Relaxed);
        p.progress = 99.9; // 封顶：100% 只由任务收尾显式设置
        p.speed_bps = 0.0;
        if let Some(st) = p.steps.get_mut(STEP_CLIENT) {
            st.status = "done".into();
            st.progress = 100.0;
        }
        if let Some(st) = p.steps.get_mut(STEP_LIBS) {
            st.status = "done".into();
            st.progress = 100.0;
        }
        if let Some(st) = p.steps.get_mut(STEP_ASSETS) {
            st.status = "done".into();
            st.progress = 100.0;
        }
        crate::task::emit(&app, &p);
    }

    // ── Native classifier JAR 下载 ──
    set_phase(&app, progress, "", "处理 native 库", STEP_LIBS, "running");
    let natives = crate::downloader::extract::process_natives(
        &vj.libraries,
        &lib_dir,
        &src_refs,
        Arc::clone(&lib_counter),
        Arc::clone(&bytes_counter),
    )
    .await;
    if let Err(errors) = natives {
        return set_error(&app, progress, errors.join("; "));
    }
    bump_completed(&app, progress);
    if cancelled(cancel) {
        return set_cancelled(&app, progress);
    }

    // ── ModLoader ──
    let mut step_idx = 4usize;
    if let (Some(ld), Some(lv)) = (main_loader, main_loader_version) {
        set_phase(
            &app,
            progress,
            "",
            &format!("安装 {}", ld),
            step_idx,
            "running",
        );
        let result = match ld {
            "fabric" => mod_loader::install_fabric(mc_version, lv, &parent_text, &config).await,
            "forge" => mod_loader::install_forge(mc_version, lv, &config).await,
            "neoforge" => mod_loader::install_neoforge(mc_version, lv, &config).await,
            "quilt" => mod_loader::install_fabric(mc_version, lv, &parent_text, &config).await,
            _ => Err(format!("未知加载器: {}", ld)),
        };
        match result {
            Ok(_) => {
                step_done(&app, progress, step_idx);
                bump_completed(&app, progress);
            }
            Err(e) => return set_error(&app, progress, e),
        }
        step_idx += 1;
    }
    if cancelled(cancel) {
        return set_cancelled(&app, progress);
    }

    // ── OptiFine ──
    if let Some(of) = optifine_patch {
        set_phase(&app, progress, "", "安装 OptiFine", step_idx, "running");
        match mod_loader::install_optifine(mc_version, of, "HD_U", &config).await {
            Ok(_) => {
                step_done(&app, progress, step_idx);
                bump_completed(&app, progress);
            }
            Err(e) => return set_error(&app, progress, e),
        }
        step_idx += 1;
    }
    if cancelled(cancel) {
        return set_cancelled(&app, progress);
    }

    // ── Fabric API ──
    if let (Some(url), Some(ver)) = (fabric_api_url, fabric_api_version) {
        set_phase(
            &app,
            progress,
            "",
            &format!("下载 Fabric API {}", ver),
            step_idx,
            "running",
        );
        // URL 最后一段做 percent-decode（文件名中的 `+` 被编码为 %2B）
        let filename = percent_encoding::percent_decode_str(
            url.rsplit('/').next().unwrap_or("fabric-api.jar"),
        )
        .decode_utf8_lossy()
        .into_owned();
        // 版本隔离：游戏从 gameDir/mods（= versions/<id>/mods）加载 mod
        let mods_dir = vdir.join("mods");
        std::fs::create_dir_all(&mods_dir).ok();
        let dest = mods_dir.join(filename);
        // 多源候选：Modrinth 官方 CDN → MCIM 换域名镜像 → BMCLAPI maven 镜像
        // （官方 CDN 国内不可达时快速降级，避免单 URL 长时间假死）
        let mut urls = vec![url.to_string()];
        let mcim = url.replace("https://cdn.modrinth.com", "https://mod.mcimirror.top");
        if mcim != url {
            urls.push(mcim);
        }
        if let Some(bmclapi) = sources.iter().find(|s| s.manifest_url.contains("bmclapi")) {
            urls.push(format!(
                "{}/net/fabricmc/fabric-api/fabric-api/{ver}/fabric-api-{ver}.jar",
                bmclapi.maven_url
            ));
        }
        match fetcher::fetch_bounded(&urls, &dest, None::<fn(u64, u64)>).await {
            Ok(_) => {
                step_done(&app, progress, step_idx);
                bump_completed(&app, progress);
            }
            Err(e) => return set_error(&app, progress, format!("下载 Fabric API 失败\n{}", e)),
        }
    }

    task::update_and_emit(&app, progress, |p| {
        p.status = "done".into();
        p.completed_files = p.total_files;
        p.progress = 100.0;
        p.current_phase = "完成".into();
    });
}

/// 下载任务共享的进度上报循环：读原子计数器 → 更新分步/总进度 → emit。
/// run_task（完整下载）与 run_repair_task（补全）共用，消除整段复制。
struct EmitLoop {
    app: AppHandle,
    cancel: Arc<AtomicBool>,
    progress: Arc<Mutex<TaskProgress>>,
    jar_done: Arc<AtomicU32>,
    lib_done: Arc<AtomicU32>,
    asst_done: Arc<AtomicU32>,
    jar_bytes: Arc<AtomicU64>,
    jar_total: Arc<AtomicU64>,
    bytes: Arc<AtomicU64>,
    /// client.jar 步骤索引（None = 任务无该步骤）
    jar_step: Option<usize>,
    lib_step: Option<usize>,
    asst_step: Option<usize>,
    lib_pending: u32,
    asst_pending: u32,
    /// 初始已完成数（完整下载 = 预扫描已存在文件；补全 = 0）
    initial_completed: u32,
    total_files: u32,
    /// 字节维度：检测完的已存在字节与清单总字节（bytes 为 0 时进度回退文件数公式）
    initial_bytes: u64,
    total_bytes: u64,
    /// jar 总大小兜底（version.json 的 size；content-length 缺失时用）
    jar_total_fallback: u64,
}

impl EmitLoop {
    fn spawn(self) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut last_bytes: u64 = 0;
            let mut last_sample = Instant::now();
            let mut speed_ema: f64 = 0.0;
            loop {
                let jd = self.jar_done.load(Ordering::Relaxed);
                let jb = self.jar_bytes.load(Ordering::Relaxed);
                let js = self.jar_total.load(Ordering::Relaxed);
                let ld = self.lib_done.load(Ordering::Relaxed);
                let ad = self.asst_done.load(Ordering::Relaxed);
                let completed = self.initial_completed + jd + ld + ad;
                // 字节维度：initial（已存在）+ jar 当前字节 + libs/assets 下载增量
                let completed_bytes = self.initial_bytes + jb + self.bytes.load(Ordering::Relaxed);

                let done = {
                    let mut p = self.progress.lock().unwrap();

                    // 实时速度：字节增量差分 + EMA 平滑（每轮采样，间隔 >=300ms 才更新）
                    let cur_bytes = self.bytes.load(Ordering::Relaxed) + jb;
                    let now = Instant::now();
                    let dt = now.duration_since(last_sample).as_secs_f64();
                    if dt >= 0.3 && cur_bytes >= last_bytes {
                        let instant = (cur_bytes - last_bytes) as f64 / dt;
                        speed_ema = if speed_ema <= 0.0 {
                            instant
                        } else {
                            speed_ema * 0.6 + instant * 0.4
                        };
                        last_bytes = cur_bytes;
                        last_sample = now;
                    }
                    p.speed_bps = speed_ema;
                    p.completed_files = completed;
                    p.completed_bytes = completed_bytes;

                    // 分步进度：client.jar 字节级（total 未知时用完成标志），libs/assets 按完成数
                    if let Some(i) = self.jar_step {
                        if let Some(st) = p.steps.get_mut(i) {
                            if st.status == "running" {
                                let jar_total = if js > 0 { js } else { self.jar_total_fallback };
                                st.progress = if jar_total > 0 {
                                    (jb as f64 / jar_total as f64 * 100.0).min(100.0)
                                } else {
                                    (jd as f64) * 100.0
                                };
                            }
                        }
                    }
                    if let Some(i) = self.lib_step {
                        if let Some(st) = p.steps.get_mut(i) {
                            if st.status == "running" {
                                st.progress = if self.lib_pending > 0 {
                                    (ld as f64 / self.lib_pending as f64 * 100.0).min(100.0)
                                } else {
                                    100.0
                                };
                            }
                        }
                    }
                    if let Some(i) = self.asst_step {
                        if let Some(st) = p.steps.get_mut(i) {
                            if st.status == "running" {
                                st.progress = if self.asst_pending > 0 {
                                    (ad as f64 / self.asst_pending as f64 * 100.0).min(100.0)
                                } else {
                                    100.0
                                };
                            }
                        }
                    }

                    // 总进度 = 已完成字节 / 总字节（与检测阶段 set_completed 同公式，
                    // 无缝衔接：检测完即 initial/total，下载中继续单调涨；大文件按字节占比，
                    // 不再出现"剩余 1 个 jar 却卡 99%"）。
                    // bytes 为 0 时回退文件数占比（repair 等无字节来源场景）。
                    // 封顶 99.9：done 前永不显示 100%，真正的 100% 由任务收尾显式设置。
                    p.progress = if self.total_bytes > 0 {
                        (completed_bytes as f64 / self.total_bytes as f64 * 100.0).min(99.9)
                    } else if self.total_files > 0 {
                        (completed as f64 / self.total_files as f64 * 100.0).min(99.9)
                    } else {
                        0.0
                    };

                    let _ = self.app.emit(crate::events::TASK_PROGRESS, &*p);
                    completed >= self.total_files || self.cancel.load(Ordering::SeqCst)
                };
                if done {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(150)).await;
            }
        })
    }
}

fn cancelled(cancel: &Arc<AtomicBool>) -> bool {
    cancel.load(Ordering::SeqCst)
}

fn set_phase(
    app: &AppHandle,
    progress: &Arc<Mutex<TaskProgress>>,
    status: &str,
    phase: &str,
    step: usize,
    step_status: &str,
) {
    task::update_and_emit(app, progress, |p| {
        if !status.is_empty() {
            p.status = status.into();
        }
        p.current_phase = phase.into();
        if let Some(st) = p.steps.get_mut(step) {
            st.status = step_status.into();
        }
    });
}

fn step_done(app: &AppHandle, progress: &Arc<Mutex<TaskProgress>>, idx: usize) {
    task::update_and_emit(app, progress, |p| {
        if let Some(st) = p.steps.get_mut(idx) {
            st.status = "done".into();
            st.progress = 100.0;
        }
    });
}

/// 总进度统一公式：优先字节占比（方案 A：全程字节，大文件不再卡 99%），
/// bytes 为 0 时回退文件数占比（repair 等无字节来源的场景）。
/// 封顶 99.9：100% 只由任务收尾显式设置（见 run_task/repair 的 done 分支）。
fn update_progress(p: &mut TaskProgress) {
    p.progress = if p.total_bytes > 0 {
        (p.completed_bytes as f64 / p.total_bytes as f64 * 100.0).min(99.9)
    } else if p.total_files > 0 {
        (p.completed_files as f64 / p.total_files as f64 * 100.0).min(99.9)
    } else {
        0.0
    };
}

fn set_total(app: &AppHandle, progress: &Arc<Mutex<TaskProgress>>, files: u32, bytes: u64) {
    task::update_and_emit(app, progress, |p| {
        p.total_files = files;
        p.total_bytes = bytes;
        update_progress(p);
    });
}

fn set_completed(app: &AppHandle, progress: &Arc<Mutex<TaskProgress>>, files: u32, bytes: u64) {
    task::update_and_emit(app, progress, |p| {
        p.completed_files = files;
        p.completed_bytes = bytes;
        update_progress(p);
    });
}

/// 收尾步骤（natives/ModLoader/OptiFine/Fabric API）每完成一个，文件计数 +1。
/// 与 total_files 中的收尾步骤数对应，保证收尾期间剩余文件显示真实。
fn bump_completed(app: &AppHandle, progress: &Arc<Mutex<TaskProgress>>) {
    task::update_and_emit(app, progress, |p| {
        p.completed_files = p.completed_files.saturating_add(1);
        update_progress(p);
    });
}

fn set_error(app: &AppHandle, progress: &Arc<Mutex<TaskProgress>>, msg: String) {
    task::update_and_emit(app, progress, |p| {
        p.status = "error".into();
        p.error = Some(msg);
    });
    // 移除任务注册（前端收到 error 事件后清理卡片）。
    // 不整删下载产物：半截/0B 文件由 sha1 校验与空响应防护在重试时自愈，
    // 已下载内容保留可复用（含 loader 依赖的原版版本 JSON）。
    if let Ok(p) = progress.try_lock() {
        task::global().fail(&p.task_id);
    }
}

fn set_cancelled(app: &AppHandle, progress: &Arc<Mutex<TaskProgress>>) {
    task::update_and_emit(app, progress, |p| {
        p.status = "cancelled".into();
        p.current_phase = "已取消".into();
    });
}

/// 拉取并解析版本 JSON，同时保留原始文本（ModLoader 合并需保留未知字段）。
async fn fetch_version_json(
    version_url: &str,
    config: &DownloaderConfig,
) -> Result<(VersionJson, String), String> {
    let text = crate::downloader::version::fetch_version_json_text(version_url, config).await?;
    let vj: VersionJson =
        serde_json::from_str(&text).map_err(|e| format!("解析 version json 失败: {}", e))?;
    Ok((vj, text))
}

/// Count total and pending assets (sha1-matched against local cache, multi-threaded).
/// 拉取/解析失败返回 Err（调用方显式报错，不再静默视为"完整"）。
/// `index_id`/`index_sha1` 非空时走 fetch_asset_index 的本地缓存（读缓存命中则免网络，否则拉取并写缓存）。
async fn count_assets_all(
    asset_index_url: &str,
    index_id: Option<&str>,
    index_sha1: Option<&str>,
    config: &DownloaderConfig,
    mc_dir: &Path,
    mut on_total: impl FnMut(u32, u64),
    mut on_checked: impl FnMut(u32, u64),
) -> Result<(u32, u64), String> {
    use crate::downloader::hash::file_sha1_matches;
    let text = crate::downloader::assets::fetch_asset_index(
        config,
        mc_dir,
        asset_index_url,
        index_id,
        index_sha1,
    )
    .await?;
    let idx: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("解析资源索引失败: {e}"))?;
    let objects_dir = mc_dir.join("assets").join("objects");

    // 先收集全部 (hash, 目标路径, size)，再分块多线程 sha1 校验（数千个文件，单线程过慢）
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
                objects_dir.join(&hash[..2]).join(hash),
                size,
            ));
        }
    }
    let total = items.len() as u32;
    if total == 0 {
        return Ok((0, 0));
    }
    // 清单已拿到：先报总数（文件数 + 字节），让调用方立即 set_total（剩余=大数字，而非检测完才出现）
    on_total(total, total_bytes);

    let n_threads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        .clamp(1, 8);
    let chunk = items.len().div_ceil(n_threads);
    let pending = std::sync::atomic::AtomicU32::new(0);
    let pending_bytes = std::sync::atomic::AtomicU64::new(0);
    let checked = std::sync::atomic::AtomicU32::new(0);
    let existing_bytes = std::sync::atomic::AtomicU64::new(0);
    // 检测进度经 mpsc 汇总到主线程逐批回调（多线程直接调 emit 会造成锁竞争）
    let (tx, rx) = std::sync::mpsc::channel::<u32>();
    std::thread::scope(|s| {
        for slice in items.chunks(chunk) {
            let tx = tx.clone();
            let counter = &pending;
            let pb = &pending_bytes;
            let checked = &checked;
            let existing = &existing_bytes;
            s.spawn(move || {
                let mut p = 0u32;
                let mut pb_local = 0u64;
                let mut done = 0u32;
                for (hash, dest, size) in slice {
                    if dest.exists() && file_sha1_matches(dest, hash) {
                        existing.fetch_add(*size, std::sync::atomic::Ordering::Relaxed);
                    } else {
                        p += 1;
                        pb_local += *size;
                    }
                    done += 1;
                    if done.is_multiple_of(500) {
                        checked.fetch_add(done, std::sync::atomic::Ordering::Relaxed);
                        let _ = tx.send(done);
                        done = 0;
                    }
                }
                checked.fetch_add(done, std::sync::atomic::Ordering::Relaxed);
                let _ = tx.send(done);
                counter.fetch_add(p, std::sync::atomic::Ordering::Relaxed);
                pb.fetch_add(pb_local, std::sync::atomic::Ordering::Relaxed);
            });
        }
        drop(tx);
        while rx.recv().is_ok() {
            on_checked(
                checked.load(std::sync::atomic::Ordering::Relaxed),
                existing_bytes.load(std::sync::atomic::Ordering::Relaxed),
            );
        }
    });
    Ok((
        pending.load(std::sync::atomic::Ordering::Relaxed),
        pending_bytes.load(std::sync::atomic::Ordering::Relaxed),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

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
        // 仅缺资源：步骤只含资源（natives 完整时不附加 native 步骤）
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
        // 仅缺 natives：步骤只含 native（此前 natives 永远附在末尾导致永远无法触发）
        let steps = repair_steps(&MissingReport {
            client_jar: false,
            libraries: 0,
            assets: 0,
            natives: 1,
        });
        assert_eq!(steps, vec!["处理 native 库"]);
    }

    #[test]
    fn test_count_natives_missing() {
        use crate::downloader::hash::sha1_hex;
        use crate::downloader::model::{Artifact, Library, LibraryDownloads};
        use std::collections::HashMap;

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
}
