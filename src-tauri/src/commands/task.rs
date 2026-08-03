use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tauri::{AppHandle, Emitter};

use crate::config::downloader::model::{DownloadSource, DownloaderConfig};
use crate::downloader::fetcher;
use crate::downloader::installer;
use crate::downloader::model::VersionJson;
use crate::persist;
use crate::task::{self, TaskProgress};

const STEP_VERJSON: usize = 0;
const STEP_CLIENT: usize = 1;
const STEP_LIBS: usize = 2;
const STEP_ASSETS: usize = 3;

#[tauri::command]
pub async fn start_download(
    app: AppHandle,
    version_id: String,
    version_url: String,
    mc_version: String,
    main_loader: Option<String>,
    main_loader_version: Option<String>,
    optifine_patch: Option<String>,
    version_dir_name: Option<String>,
) -> Result<String, String> {
    let task_name = match (&main_loader, &main_loader_version) {
        (Some(ld), Some(lv)) => format!("{} {}-{} 安装", mc_version, ld, lv),
        _ => format!("{} 下载", version_id),
    };

    let mut steps = vec!["下载原版json文件", "下载原版客户端文件", "下载原版支持库文件", "下载原版资源文件"];
    if main_loader.is_some() { steps.push("安装 ModLoader"); }
    if optifine_patch.is_some() { steps.push("安装 OptiFine"); }

    let tm = task::global();
    let (task_id, cancel, progress) = tm.create(&task_name, &steps);

    tauri::async_runtime::spawn(async move {
        run_task(app, &cancel, &progress, &version_id, &version_url, &mc_version,
                 main_loader.as_deref(), main_loader_version.as_deref(),
                 optifine_patch.as_deref(), version_dir_name).await;
    });

    Ok(task_id)
}

/// 扫描结果：缺失文件统计（补全文件功能，前端展示用）。
#[derive(Debug, Clone, serde::Serialize)]
pub struct MissingReport {
    /// 是否缺失客户端 client.jar
    pub client_jar: bool,
    /// 缺失的支持库数量
    pub libraries: u32,
    /// 缺失的资源文件数量
    pub assets: u32,
}

/// 在版本目录中扫描第一个 `.json` 文件（文件名可能不等于目录名，如 26.2-Fabric 目录内是 26.2.json）。
fn find_version_json(vdir: &std::path::Path) -> Option<std::path::PathBuf> {
    std::fs::read_dir(vdir).ok()?.filter_map(|e| e.ok()).find(|e| {
        e.path()
            .extension()
            .map(|ext| ext == "json")
            .unwrap_or(false)
    })
    .map(|e| e.path())
}

/// 读取版本目录的 version json（按文件名扫描，不假设文件名=目录名）。
fn read_version_json(vdir: &std::path::Path) -> Result<VersionJson, String> {
    let vj_path = find_version_json(vdir)
        .ok_or_else(|| format!("版本目录下未找到 JSON 文件: {}", vdir.display()))?;
    let text = std::fs::read_to_string(&vj_path)
        .map_err(|e| format!("读取版本文件失败：{e}"))?;
    serde_json::from_str(&text).map_err(|e| format!("解析版本文件失败：{e}"))
}

/// 扫描版本缺失文件（client.jar / 支持库 / 资源），不发起下载。
/// 支持库按本地 sha1 比对，资源按 assets index 与本地缓存比对（多线程）。
#[tauri::command]
pub async fn scan_missing_files(version_dir_name: String) -> Result<MissingReport, String> {
    use crate::downloader::library;
    use std::path::Path;

    let config: DownloaderConfig = persist::read("downloader").unwrap_or_default();
    let mc = Path::new(&config.minecraft_dir);
    let vdir = mc.join("versions").join(&version_dir_name);
    let vj = read_version_json(&vdir)?;

    // 1. client.jar（扫描目录内实际 jar；存在但 0 字节视为损坏/缺失）
    let jar_missing = match crate::config::launcher::args::find_jar_in_dir(&vdir) {
        Some(p) => p.metadata().map(|m| m.len() == 0).unwrap_or(true),
        None => true,
    };

    // 2. 支持库（规则过滤 + 架构匹配，与 pre_launch_check 一致）
    let lib_dir = mc.join("libraries");
    let lib_pending = library::count_pending(&vj.libraries, &lib_dir) as u32;

    // 3. 资源（sha1 比对本地缓存，多线程）
    let (_total, asst_pending) =
        count_assets_all(&vj.asset_index.url, &config, mc).await;

    Ok(MissingReport {
        client_jar: jar_missing,
        libraries: lib_pending,
        assets: asst_pending,
    })
}

/// 按缺失情况动态生成修复任务步骤（与 run_repair_task 的索引构造顺序一致）。
fn repair_steps(report: &MissingReport) -> Vec<String> {
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
    steps.push("处理 native 库".to_string());
    steps
}

/// 提交补全任务到下载管理：只下载实际缺失的部分，步骤按缺失情况动态生成。
/// 返回任务 id。
#[tauri::command]
pub async fn submit_missing_download(
    app: AppHandle,
    version_dir_name: String,
) -> Result<String, String> {
    let report = scan_missing_files(version_dir_name.clone()).await?;

    let steps = repair_steps(&report);
    let tm = task::global();
    let (task_id, cancel, progress) = tm.create(
        &format!("{} 版本修复", version_dir_name),
        &steps.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
    );

    tauri::async_runtime::spawn(async move {
        run_repair_task(app, &cancel, &progress, &version_dir_name, report).await;
    });

    Ok(task_id)
}

/// 补全任务主体：基于本地 version json 下载缺失文件，进度经 task-progress 事件推送。
/// `report` 为提交时扫描结果（步骤数与其一致），运行中按对应步骤索引更新。
async fn run_repair_task(
    app: AppHandle,
    cancel: &Arc<AtomicBool>,
    progress: &Arc<Mutex<TaskProgress>>,
    version_dir_name: &str,
    report: MissingReport,
) {
    let config: DownloaderConfig = persist::read("downloader").unwrap_or_default();
    let sources: Vec<crate::config::downloader::model::DownloadSource> =
        config.file_sources_list().into_iter().cloned().collect();
    let src_refs: Vec<&crate::config::downloader::model::DownloadSource> = sources.iter().collect();
    let mc = std::path::PathBuf::from(&config.minecraft_dir);
    let vdir = mc.join("versions").join(version_dir_name);
    let vj = match read_version_json(&vdir) {
        Ok(v) => v,
        Err(e) => return set_error(&app, progress, e),
    };

    let jar_missing = report.client_jar;
    let lib_pending = report.libraries;
    let asst_pending = report.assets;
    let total_files = if jar_missing { 1u32 } else { 0u32 } + lib_pending + asst_pending;
    println!(
        "[repair] {} missing: jar={} libs={} assets={} total={}",
        version_dir_name, jar_missing, lib_pending, asst_pending, total_files
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
    let natives_idx = idx;

    set_total(&app, progress, total_files);
    set_completed(&app, progress, 0);
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

    let jc = Arc::clone(&jar_counter);
    let lc = Arc::clone(&lib_counter);
    let ac = Arc::clone(&asst_counter);
    let jbc = Arc::clone(&jar_bytes);
    let jt = Arc::clone(&jar_total);
    let bc = Arc::clone(&bytes_counter);
    let p_guard = Arc::clone(progress);
    let p_app = app.clone();
    let p_cancel = Arc::clone(cancel);
    let p_total = total_files;

    let emit_handle = tokio::spawn(async move {
        let started = std::time::Instant::now();
        loop {
            let jd = jc.load(Ordering::Relaxed);
            let ld = lc.load(Ordering::Relaxed);
            let ad = ac.load(Ordering::Relaxed);
            let done = {
                let mut p = p_guard.lock().unwrap();
                // 分步进度（按实际存在的步骤索引更新）
                if let Some(i) = jar_idx {
                    if let Some(st) = p.steps.get_mut(i) {
                        if st.status == "running" {
                            // client.jar 用字节进度（下载中渐进，单文件不再 0/100 跳变）
                            let t = jt.load(Ordering::Relaxed);
                            st.progress = if t > 0 {
                                (jbc.load(Ordering::Relaxed) as f64 / t as f64 * 100.0).min(100.0)
                            } else {
                                (jd as f64) * 100.0
                            };
                        }
                    }
                }
                if let Some(i) = libs_idx {
                    if let Some(st) = p.steps.get_mut(i) {
                        if st.status == "running" {
                            st.progress = if lib_pending > 0 {
                                (ld as f64 / lib_pending as f64 * 100.0).min(100.0)
                            } else {
                                100.0
                            };
                        }
                    }
                }
                if let Some(i) = asst_idx {
                    if let Some(st) = p.steps.get_mut(i) {
                        if st.status == "running" {
                            st.progress = if asst_pending > 0 {
                                (ad as f64 / asst_pending as f64 * 100.0).min(100.0)
                            } else {
                                100.0
                            };
                        }
                    }
                }
                let elapsed = started.elapsed().as_secs_f64().max(0.001);
                p.speed_bps = (bc.load(Ordering::Relaxed) + jbc.load(Ordering::Relaxed)) as f64 / elapsed;
                p.completed_files = jd + ld + ad;
                // 总进度 = 分步进度平均（与完整下载一致；jar 步骤为字节级，总进度渐进）
                let step_sum: f64 = p.steps.iter().map(|s| s.progress).sum();
                let step_count = p.steps.len().max(1) as f64;
                p.progress = (step_sum / step_count).min(100.0);
                let _ = p_app.emit(crate::events::TASK_PROGRESS, &*p);
                p.completed_files >= p_total || p_cancel.load(Ordering::SeqCst)
            };
            if done {
                break;
            }
            tokio::time::sleep(Duration::from_millis(150)).await;
        }
    });

    let jar = async {
        if jar_missing {
            // 清理目录内损坏（0 字节）的 jar，避免与下载目标并存
            if let Some(bad) = crate::config::launcher::args::find_jar_in_dir(&vdir) {
                if bad.metadata().map(|m| m.len() == 0).unwrap_or(false) {
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
        Some(Arc::clone(&lib_counter)),
        Some(Arc::clone(&bytes_counter)),
    );

    let asst = crate::downloader::assets::download_assets(
        &vj.asset_index.url,
        &vj.asset_index.id,
        Some(&vj.asset_index.sha1),
        &config,
        &mc,
        Some(Arc::clone(&asst_counter)),
        Some(Arc::clone(&bytes_counter)),
    );

    let parallel = tokio::try_join!(jar, libs, asst);
    emit_handle.abort();

    if cancelled(cancel) {
        return set_cancelled(&app, progress);
    }
    if let Err(errors) = parallel {
        println!("[repair] {} download failed: {}", version_dir_name, errors.join("; "));
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

    // natives（幂等：已解压跳过）
    set_phase(&app, progress, "", "处理 native 库", natives_idx, "running");
    if let Err(errors) = crate::downloader::extract::process_natives(
        &vj.libraries,
        &lib_dir,
        &src_refs,
        None,
        None,
    )
    .await
    {
        println!("[repair] {} natives failed: {}", version_dir_name, errors.join("; "));
        return set_error(&app, progress, errors.join("; "));
    }
    step_done(&app, progress, natives_idx);

    task::update_and_emit(&app, progress, |p| {
        p.status = "done".into();
        p.completed_files = p.total_files;
        p.progress = 100.0;
        p.current_phase = "修复完成".into();
    });
}

async fn run_task(
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
) {
    let config: DownloaderConfig = persist::read("downloader").unwrap_or_default();
    let sources: Vec<DownloadSource> = config.file_sources_list().into_iter().cloned().collect();
    let src_refs: Vec<&DownloadSource> = sources.iter().collect();
    let mc = Path::new(&config.minecraft_dir);

    set_phase(&app, progress, "running", "下载原版json文件", STEP_VERJSON, "running");
    if cancelled(cancel) { return set_cancelled(&app, progress); }

    // ── Step 0: fetch version.json in-memory ──
    let vj = match fetch_version_json(version_url, &config).await {
        Ok(v) => v,
        Err(e) => return set_error(&app, progress, e),
    };
    step_done(&app, progress, STEP_VERJSON);
    if cancelled(cancel) { return set_cancelled(&app, progress); }

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
        let (name, p) = crate::downloader::resolve_version_dir(mc, version_id);
        dir_name = name;
        vdir = p;
    }

    // 确保目录存在并写入 version JSON（所有路径都需要）
    std::fs::create_dir_all(&vdir).ok();
    if let Ok(json) = serde_json::to_string_pretty(&vj) {
        // JSON 文件用版本 ID 命名，不是目录名（目录可能因冲突被重命名）
        let jp = vdir.join(format!("{}.json", version_id));
        std::fs::write(&jp, &json).ok();
    }
    task::global().set_cleanup_path(&progress.try_lock().unwrap().task_id, vdir.clone());

    // ── Count total work & already-existing ──
    let lib_dir = mc.join("libraries");
    let lib_total_all = crate::downloader::library::count_total(&vj.libraries) as u32;
    let lib_pending = crate::downloader::library::count_pending(&vj.libraries, &lib_dir) as u32;
    let lib_existing = lib_total_all - lib_pending;

    let jar_exists = vj.downloads.client.is_some();
    let jar_total = if jar_exists { 1u32 } else { 0u32 };

    let (asst_total_all, asst_pending) = count_assets_all(&vj.asset_index.url, &config, mc).await;
    let asst_existing = asst_total_all - asst_pending;

    let total_files = 1 + jar_total + lib_total_all + asst_total_all;
    let initial_completed = 1 + lib_existing + asst_existing;

    set_total(&app, progress, total_files);
    set_completed(&app, progress, initial_completed);

    // ── Parallel phase: client.jar + libraries + assets ──
    let jar_counter = Arc::new(AtomicU32::new(0));
    let jar_bytes_counter = Arc::new(AtomicU64::new(0));
    let jar_total_size = Arc::new(AtomicU64::new(0));
    let lib_counter = Arc::new(AtomicU32::new(0));
    let asst_counter = Arc::new(AtomicU32::new(0));
    let bytes_counter = Arc::<AtomicU64>::new(AtomicU64::new(0));

    let jc = Arc::clone(&jar_counter);
    let jbc = Arc::clone(&jar_bytes_counter);
    let jsize = Arc::clone(&jar_total_size);
    let lc = Arc::clone(&lib_counter);
    let ac = Arc::clone(&asst_counter);
    let bc = Arc::clone(&bytes_counter);
    let p_guard = Arc::clone(progress);
    let p_app = app.clone();
    let p_cancel = Arc::clone(cancel);

    let p_lib_pending = lib_pending;
    let p_asst_pending = asst_pending;
    let p_total_files = total_files;
    let p_initial_completed = initial_completed;
    let p_jar_fallback = vj.downloads.client.as_ref().map(|c| c.size).unwrap_or(0);

    // Set all phases to running BEFORE spawning emit loop to avoid lock races
    set_phase(&app, progress, "", "下载原版客户端文件", STEP_CLIENT, "running");
    set_phase(&app, progress, "", "下载原版支持库文件", STEP_LIBS, "running");
    set_phase(&app, progress, "", "下载原版资源文件", STEP_ASSETS, "running");

    let emit_handle = tokio::spawn(async move {
        let started = Instant::now();
        loop {
            let jd = jc.load(Ordering::Relaxed);
            let jb = jbc.load(Ordering::Relaxed);
            let js = jsize.load(Ordering::Relaxed);
            let ld = lc.load(Ordering::Relaxed);
            let ad = ac.load(Ordering::Relaxed);
            let total = p_initial_completed + jd + ld + ad;

            {
                let mut p = p_guard.lock().unwrap();
                let elapsed = started.elapsed().as_secs_f64().max(0.001);
                p.speed_bps = bc.load(Ordering::Relaxed) as f64 / elapsed;
                p.completed_files = total;

                // 分步进度
                if let Some(st) = p.steps.get_mut(STEP_CLIENT) {
                    if st.status == "running" {
                        let jar_total = if js > 0 { js } else { p_jar_fallback };
                        st.progress = if jar_total > 0 { (jb as f64 / jar_total as f64 * 100.0).min(100.0) } else { 100.0 };
                    }
                }
                if let Some(st) = p.steps.get_mut(STEP_LIBS) {
                    if st.status == "running" {
                        st.progress = if p_lib_pending > 0 { (ld as f64 / p_lib_pending as f64) * 100.0 } else { 100.0 };
                    }
                }
                if let Some(st) = p.steps.get_mut(STEP_ASSETS) {
                    if st.status == "running" {
                        st.progress = if p_asst_pending > 0 { (ad as f64 / p_asst_pending as f64) * 100.0 } else { 100.0 };
                    }
                }

                // 总进度 = 各分步进度的平均值（始终与分步显示一致）
                let step_sum: f64 = p.steps.iter().map(|s| s.progress).sum();
                let step_count = p.steps.len().max(1) as f64;
                p.progress = (step_sum / step_count).min(100.0);

                let _ = p_app.emit(crate::events::TASK_PROGRESS, &*p);
            }

            if total >= p_total_files || p_cancel.load(Ordering::SeqCst) { break; }
            tokio::time::sleep(Duration::from_millis(150)).await;
        }
    });

    let jar = async {
        if let Some(ref c) = vj.downloads.client {
            let urls: Vec<String> = sources.iter().map(|src| src.mirror_url(&c.url)).collect();
            let jar_path = vdir.join(format!("{}.jar", dir_name));
            let bytes = Arc::clone(&bytes_counter);
            let jbytes = Arc::clone(&jar_bytes_counter);
            let jar_total = Arc::clone(&jar_total_size);
            let cb = move |downloaded: u64, total: u64| {
                bytes.store(downloaded, Ordering::Relaxed);
                jbytes.store(downloaded, Ordering::Relaxed);
                if total > 0 {
                    jar_total.store(total, Ordering::Relaxed);
                }
            };
            fetcher::fetch_fallback(&urls, &jar_path, Some(cb)).await.map_err(|e| vec![e.to_string()])?;
            // client.jar sha1 完整性校验（防止镜像 CDN 返回不完整/错误内容）
            if !c.sha1.is_empty() && !crate::downloader::hash::file_sha1_matches(&jar_path, &c.sha1) {
                let _ = std::fs::remove_file(&jar_path);
                return Err(vec!["client.jar 校验失败（sha1 不匹配）".to_string()]);
            }
            jar_counter.store(1, Ordering::Relaxed);
        }
        Ok::<_, Vec<String>>(())
    };

    let lib_dir = mc.join("libraries");
    let libs = crate::downloader::library::download_libraries(
        &vj.libraries, &lib_dir, config.concurrency as usize, &src_refs,
        Some(Arc::clone(&lib_counter)), Some(Arc::clone(&bytes_counter)),
    );

    let asst = crate::downloader::assets::download_assets(
        &vj.asset_index.url, &vj.asset_index.id, Some(&vj.asset_index.sha1),
        &config, mc, Some(Arc::clone(&asst_counter)), Some(Arc::clone(&bytes_counter)),
    );

    let parallel = tokio::try_join!(jar, libs, asst);
    emit_handle.abort();

    // 失败/取消优先上报，避免先发 100% 完成误导前端
    if cancelled(cancel) { return set_cancelled(&app, progress); }
    if parallel.is_err() {
        return set_error(&app, progress, parallel.unwrap_err().join("; "));
    }
    if cancelled(cancel) { return set_cancelled(&app, progress); }

    // Emit final state after downloads complete (emit loop may have been aborted mid-cycle)
    {
        let mut p = progress.lock().unwrap();
        p.completed_files = total_files;
        p.progress = 100.0;
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
        crate::task::emit(&app, &*p);
    }

    // ── Native classifier JAR 下载 ──
    set_phase(&app, progress, "", "处理 native 库", STEP_LIBS, "running");
    let natives = crate::downloader::extract::process_natives(
        &vj.libraries, &lib_dir, &src_refs,
        Some(Arc::clone(&lib_counter)), Some(Arc::clone(&bytes_counter)),
    ).await;
    if let Err(errors) = natives {
        return set_error(&app, progress, errors.join("; "));
    }
    if cancelled(cancel) { return set_cancelled(&app, progress); }

    // ── ModLoader ──
    let mut step_idx = 4usize;
    if let (Some(ld), Some(lv)) = (main_loader, main_loader_version) {
        set_phase(&app, progress, "", &format!("安装 {}", ld), step_idx, "running");
        let result = match ld {
            "fabric" => installer::install_fabric(mc_version, lv, &config).await,
            "forge" => installer::install_forge(mc_version, lv, &config).await,
            "neoforge" => installer::install_neoforge(mc_version, lv, &config).await,
            "quilt" => installer::install_fabric(mc_version, lv, &config).await,
            _ => Err(format!("未知加载器: {}", ld)),
        };
        match result {
            Ok(_) => step_done(&app, progress, step_idx),
            Err(e) => return set_error(&app, progress, e),
        }
        step_idx += 1;
    }
    if cancelled(cancel) { return set_cancelled(&app, progress); }

    // ── OptiFine ──
    if let Some(of) = optifine_patch {
        set_phase(&app, progress, "", "安装 OptiFine", step_idx, "running");
        match installer::install_optifine(mc_version, of, "HD_U", &config).await {
            Ok(_) => step_done(&app, progress, step_idx),
            Err(e) => return set_error(&app, progress, e),
        }
    }

    task::update_and_emit(&app, progress, |p| {
        p.status = "done".into();
        p.progress = 100.0;
        p.current_phase = "完成".into();
    });
}

#[tauri::command]
pub async fn cancel_task(task_id: String) -> Result<(), String> {
    task::global().cancel(&task_id);
    Ok(())
}

#[tauri::command]
pub async fn list_tasks() -> Result<Vec<TaskProgress>, String> {
    Ok(task::global().list())
}

#[tauri::command]
pub async fn remove_task(task_id: String) -> Result<(), String> {
    task::global().remove(&task_id);
    Ok(())
}

fn cancelled(cancel: &Arc<AtomicBool>) -> bool {
    cancel.load(Ordering::SeqCst)
}

fn set_phase(app: &AppHandle, progress: &Arc<Mutex<TaskProgress>>, status: &str, phase: &str, step: usize, step_status: &str) {
    task::update_and_emit(app, progress, |p| {
        if !status.is_empty() { p.status = status.into(); }
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

fn set_total(app: &AppHandle, progress: &Arc<Mutex<TaskProgress>>, n: u32) {
    task::update_and_emit(app, progress, |p| {
        p.total_files = n;
        if p.total_files > 0 {
            p.progress = (p.completed_files as f64 / p.total_files as f64 * 100.0).min(100.0);
        }
    });
}

fn set_completed(app: &AppHandle, progress: &Arc<Mutex<TaskProgress>>, n: u32) {
    task::update_and_emit(app, progress, |p| {
        p.completed_files = n;
        if p.total_files > 0 {
            p.progress = (p.completed_files as f64 / p.total_files as f64 * 100.0).min(100.0);
        }
    });
}

fn set_error(app: &AppHandle, progress: &Arc<Mutex<TaskProgress>>, msg: String) {
    task::update_and_emit(app, progress, |p| {
        p.status = "error".into();
        p.error = Some(msg);
    });
}

fn set_cancelled(app: &AppHandle, progress: &Arc<Mutex<TaskProgress>>) {
    task::update_and_emit(app, progress, |p| {
        p.status = "cancelled".into();
        p.current_phase = "已取消".into();
    });
}

async fn fetch_version_json(
    version_url: &str,
    config: &DownloaderConfig,
) -> Result<VersionJson, String> {
    let text = config
        .fetch_with_fallback(config.file_source_mode, |s| s.mirror_url(version_url))
        .await
        .map_err(|e| format!("获取 version json 失败: {e}"))?;
    serde_json::from_str(&text).map_err(|e| format!("解析 version json 失败: {}", e))
}

/// Count total and pending assets (sha1-matched against local cache, multi-threaded).
/// Uses the same http client as `download_assets` for consistency.
async fn count_assets_all(asset_index_url: &str, config: &DownloaderConfig, mc_dir: &Path) -> (u32, u32) {
    use crate::downloader::hash::file_sha1_matches;
    let text = match config
        .fetch_with_fallback(config.file_source_mode, |s| s.mirror_url(asset_index_url))
        .await
    {
        Ok(t) => t,
        _ => return (0, 0),
    };
    let idx: serde_json::Value = match serde_json::from_str(&text) { Ok(v) => v, _ => return (0, 0) };
    let objects_dir = mc_dir.join("assets").join("objects");

    // 先收集全部 (hash, 目标路径)，再分块多线程 sha1 校验（数千个文件，单线程过慢）
    let mut items: Vec<(String, std::path::PathBuf)> = Vec::new();
    if let Some(map) = idx["objects"].as_object() {
        for (_path, obj) in map {
            let Some(hash) = obj.get("hash").and_then(|v| v.as_str()) else { continue; };
            items.push((hash.to_string(), objects_dir.join(&hash[..2]).join(hash)));
        }
    }
    let total = items.len() as u32;
    if total == 0 {
        return (0, 0);
    }

    let n_threads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        .clamp(1, 8);
    let chunk = items.len().div_ceil(n_threads);
    let pending = std::sync::atomic::AtomicU32::new(0);
    std::thread::scope(|s| {
        for slice in items.chunks(chunk) {
            let counter = &pending;
            s.spawn(move || {
                let mut p = 0u32;
                for (hash, dest) in slice {
                    if dest.exists() && file_sha1_matches(dest, hash) {
                        continue;
                    }
                    p += 1;
                }
                counter.fetch_add(p, std::sync::atomic::Ordering::Relaxed);
            });
        }
    });
    (total, pending.load(std::sync::atomic::Ordering::Relaxed))
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
        // 仅缺资源：步骤只含资源 + native
        let steps = repair_steps(&MissingReport {
            client_jar: false,
            libraries: 0,
            assets: 5,
        });
        assert_eq!(steps, vec!["下载资源文件", "处理 native 库"]);
    }

    #[test]
    fn test_repair_steps_only_jar() {
        let steps = repair_steps(&MissingReport {
            client_jar: true,
            libraries: 0,
            assets: 0,
        });
        assert_eq!(steps, vec!["下载客户端文件", "处理 native 库"]);
    }
}
