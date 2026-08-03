//! 下载任务编排：完整下载（run_task）与补全（run_repair_task）的步骤执行与进度上报。
//! 命令层（commands/task.rs）只负责注册任务与 spawn，执行逻辑全部在本模块。

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

/// 读取版本目录的 version json（按文件名扫描，不假设文件名=目录名）。
fn read_version_json(vdir: &std::path::Path) -> Result<VersionJson, String> {
    let vj_path = crate::config::launcher::args::find_file_in_dir(vdir, "json")
        .ok_or_else(|| format!("版本目录下未找到 JSON 文件: {}", vdir.display()))?;
    let text = std::fs::read_to_string(&vj_path)
        .map_err(|e| format!("读取版本文件失败：{e}"))?;
    serde_json::from_str(&text).map_err(|e| format!("解析版本文件失败：{e}"))
}

/// 扫描版本缺失文件（client.jar / 支持库 / 资源），不发起下载。
/// 支持库按本地 sha1 比对，资源按 assets index 与本地缓存比对（多线程）。
pub async fn scan_missing(version_dir_name: &str) -> Result<MissingReport, String> {
    use crate::downloader::library;

    let config: DownloaderConfig = persist::read("downloader").unwrap_or_default();
    let mc = config.minecraft_dir_abs();
    let vdir = mc.join("versions").join(version_dir_name);
    let vj = read_version_json(&vdir)?;

    // 1. client.jar（扫描目录内实际 jar；存在但 0 字节视为损坏/缺失）
    let jar_missing = match crate::config::launcher::args::find_file_in_dir(&vdir, "jar") {
        Some(p) => p.metadata().map(|m| m.len() == 0).unwrap_or(true),
        None => true,
    };

    // 2. 支持库（规则过滤 + 架构匹配，与 pre_launch_check 一致）
    let lib_dir = mc.join("libraries");
    let lib_pending = library::count_pending(&vj.libraries, &lib_dir) as u32;

    // 3. 资源（sha1 比对本地缓存，多线程；非下载任务，无需进度回调）
    let asst_pending = count_assets_all(&vj.asset_index.url, &config, &mc, |_| {}, |_| {}).await;

    Ok(MissingReport {
        client_jar: jar_missing,
        libraries: lib_pending,
        assets: asst_pending,
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
    steps.push("处理 native 库".to_string());
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
    let config: DownloaderConfig = persist::read("downloader").unwrap_or_default();
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
        jar_total_fallback: 0,
    }
    .spawn();

    let jar = async {
        if jar_missing {
            // 清理目录内损坏（0 字节）的 jar，避免与下载目标并存
            if let Some(bad) = crate::config::launcher::args::find_file_in_dir(&vdir, "jar") {
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

/// 完整下载任务主体：version json → client.jar / libraries / assets → natives → ModLoader/OptiFine。
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
) {
    let config: DownloaderConfig = persist::read("downloader").unwrap_or_default();
    let sources: Vec<DownloadSource> = config.file_sources_list().into_iter().cloned().collect();
    let src_refs: Vec<&DownloadSource> = sources.iter().collect();
    let mc = config.minecraft_dir_abs();

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
        let (name, p) = crate::downloader::resolve_version_dir(&mc, version_id);
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

    // ── 检测本地已有文件：先给全量总数（剩余=大数字），检测中实时上报已完成数，避免 UI 死等 ──
    let lib_dir = mc.join("libraries");
    let lib_total_all = crate::downloader::library::count_total(&vj.libraries) as u32;
    let lib_pending = crate::downloader::library::count_pending(&vj.libraries, &lib_dir) as u32;
    let lib_existing = lib_total_all - lib_pending;

    let jar_exists = vj.downloads.client.is_some();
    let jar_total = if jar_exists { 1u32 } else { 0u32 };

    // 资源清单：fetch asset index（网络）→ 报资产总数；再多线程 sha1 检测本地（逐批上报）
    let mut asst_total_all = 0u32;
    task::update_and_emit(&app, progress, |p| p.current_phase = "获取资源清单".into());
    let asst_pending = count_assets_all(
        &vj.asset_index.url,
        &config,
        &mc,
        |total| {
            asst_total_all = total;
            // 清单齐了：剩余=全量总数，已完成仅 version json
            set_total(&app, progress, 1 + jar_total + lib_total_all + total);
            set_completed(&app, progress, 1);
            task::update_and_emit(&app, progress, |p| p.current_phase = "检测本地资源文件".into());
        },
        |checked| {
            // 检测中：已完成 = version json + 已存在库 + 已检测资产
            set_completed(&app, progress, 1 + lib_existing + checked);
        },
    )
    .await;
    let asst_existing = asst_total_all.saturating_sub(asst_pending);
    set_completed(&app, progress, 1 + lib_existing + asst_existing);

    let total_files = 1 + jar_total + lib_total_all + asst_total_all;
    let initial_completed = 1 + lib_existing + asst_existing;

    // ── Parallel phase: client.jar + libraries + assets ──
    let jar_counter = Arc::new(AtomicU32::new(0));
    let jar_bytes_counter = Arc::new(AtomicU64::new(0));
    let jar_total_size = Arc::new(AtomicU64::new(0));
    let lib_counter = Arc::new(AtomicU32::new(0));
    let asst_counter = Arc::new(AtomicU32::new(0));
    let bytes_counter = Arc::<AtomicU64>::new(AtomicU64::new(0));

    // Set all phases to running BEFORE spawning emit loop to avoid lock races
    set_phase(&app, progress, "", "下载原版客户端文件", STEP_CLIENT, "running");
    set_phase(&app, progress, "", "下载原版支持库文件", STEP_LIBS, "running");
    set_phase(&app, progress, "", "下载原版资源文件", STEP_ASSETS, "running");

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
        &config, &mc, Some(Arc::clone(&asst_counter)), Some(Arc::clone(&bytes_counter)),
    );

    let parallel = tokio::try_join!(jar, libs, asst);
    emit_handle.abort();

    // 失败/取消优先上报，避免先发 100% 完成误导前端
    if cancelled(cancel) { return set_cancelled(&app, progress); }
    if let Err(errors) = parallel {
        return set_error(&app, progress, errors.join("; "));
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
        crate::task::emit(&app, &p);
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

                let done = {
                    let mut p = self.progress.lock().unwrap();

                    // 实时速度：字节增量差分 + EMA 平滑（每轮采样，间隔 >=300ms 才更新）
                    let cur_bytes = self.bytes.load(Ordering::Relaxed) + jb;
                    let now = Instant::now();
                    let dt = now.duration_since(last_sample).as_secs_f64();
                    if dt >= 0.3 && cur_bytes >= last_bytes {
                        let instant = (cur_bytes - last_bytes) as f64 / dt;
                        speed_ema = if speed_ema <= 0.0 { instant } else { speed_ema * 0.6 + instant * 0.4 };
                        last_bytes = cur_bytes;
                        last_sample = now;
                    }
                    p.speed_bps = speed_ema;
                    p.completed_files = completed;

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

                    // 总进度 = 已完成文件数 / 总文件数（与检测阶段 set_completed 同公式，
                    // 无缝衔接：检测完即 initial/total，下载中继续单调涨）
                    p.progress = if self.total_files > 0 {
                        (completed as f64 / self.total_files as f64 * 100.0).min(100.0)
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
    // 总进度 = completed/total，与 EmitLoop 同公式：
    // 检测阶段随已完成数平滑涨到初始值，下载阶段继续单调涨，无跳变
    task::update_and_emit(app, progress, |p| {
        p.total_files = n;
        if n > 0 {
            p.progress = (p.completed_files as f64 / n as f64 * 100.0).min(100.0);
        }
    });
}

fn set_completed(app: &AppHandle, progress: &Arc<Mutex<TaskProgress>>, n: u32) {
    task::update_and_emit(app, progress, |p| {
        p.completed_files = n;
        if p.total_files > 0 {
            p.progress = (n as f64 / p.total_files as f64 * 100.0).min(100.0);
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
    let text = crate::downloader::version::fetch_version_json_text(version_url, config).await?;
    serde_json::from_str(&text).map_err(|e| format!("解析 version json 失败: {}", e))
}

/// Count total and pending assets (sha1-matched against local cache, multi-threaded).
async fn count_assets_all(
    asset_index_url: &str,
    config: &DownloaderConfig,
    mc_dir: &Path,
    mut on_total: impl FnMut(u32),
    mut on_checked: impl FnMut(u32),
) -> u32 {
    use crate::downloader::hash::file_sha1_matches;
    let text = match crate::downloader::assets::fetch_asset_index(config, mc_dir, asset_index_url, None, None)
        .await
    {
        Ok(t) => t,
        _ => return 0,
    };
    let idx: serde_json::Value = match serde_json::from_str(&text) { Ok(v) => v, _ => return 0 };
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
        return 0;
    }
    // 清单已拿到：先报总数，让调用方立即 set_total（剩余=大数字，而非检测完才出现）
    on_total(total);

    let n_threads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        .clamp(1, 8);
    let chunk = items.len().div_ceil(n_threads);
    let pending = std::sync::atomic::AtomicU32::new(0);
    let checked = std::sync::atomic::AtomicU32::new(0);
    // 检测进度经 mpsc 汇总到主线程逐批回调（多线程直接调 emit 会造成锁竞争）
    let (tx, rx) = std::sync::mpsc::channel::<u32>();
    std::thread::scope(|s| {
        for slice in items.chunks(chunk) {
            let tx = tx.clone();
            let counter = &pending;
            let checked = &checked;
            s.spawn(move || {
                let mut p = 0u32;
                let mut done = 0u32;
                for (hash, dest) in slice {
                    if !(dest.exists() && file_sha1_matches(dest, hash)) {
                        p += 1;
                    }
                    done += 1;
                    if done % 500 == 0 {
                        checked.fetch_add(done, std::sync::atomic::Ordering::Relaxed);
                        let _ = tx.send(done);
                        done = 0;
                    }
                }
                checked.fetch_add(done, std::sync::atomic::Ordering::Relaxed);
                let _ = tx.send(done);
                counter.fetch_add(p, std::sync::atomic::Ordering::Relaxed);
            });
        }
        drop(tx);
        while rx.recv().is_ok() {
            on_checked(checked.load(std::sync::atomic::Ordering::Relaxed));
        }
    });
    pending.load(std::sync::atomic::Ordering::Relaxed)
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
