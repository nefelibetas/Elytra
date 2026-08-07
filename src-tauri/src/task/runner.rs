// ── 完整下载任务主体（原版 / ModLoader / OptiFine / Fabric API）──

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use tauri::AppHandle;

use crate::config::downloader::model::{DownloadSource, DownloaderConfig};
use crate::constants;
use crate::downloader::fetcher;
use crate::downloader::mod_loader;
use crate::downloader::model::VersionJson;
use crate::persist;
use crate::task::progress::{
    cancelled, set_cancelled, set_completed, set_error, set_phase, set_total, step_done, EmitLoop,
};
use crate::task::repair::{count_assets_all, count_natives_missing};
use crate::task::{self, DownloadTask, StepInfo, StepKind, TaskProgress, TaskSpec};

/// 完整下载任务主体：version json → client.jar / libraries / assets → natives → ModLoader/OptiFine → Fabric API。
/// 任务配置（含 ModLoader 选择）由命令层校验后经 DownloadTask 传入。
pub async fn run_task(
    app: AppHandle,
    cancel: &Arc<AtomicBool>,
    progress: &Arc<Mutex<TaskProgress>>,
    task: &DownloadTask,
) {
    let config: DownloaderConfig = persist::read(constants::SECT_DOWNLOADER).unwrap_or_default();
    let sources: Vec<DownloadSource> = config.file_sources_list().into_iter().cloned().collect();
    let src_refs: Vec<&DownloadSource> = sources.iter().collect();
    let mc = config.minecraft_dir_abs();

    let version_id = &task.version_id;
    let version_url = &task.version_url;
    let mc_version = &task.mc_version;
    let loader = task.loader.as_ref();
    let optifine_patch = task.optifine_patch.as_deref();
    let fabric_api = task.fabric_api.as_ref();

    // ── 目标目录提前确定（不依赖 version JSON），并做同路径互斥 ──
    let dir_name: String;
    let vdir: std::path::PathBuf;
    if let Some(pre) = task.version_dir_name.as_deref() {
        dir_name = pre.to_string();
        vdir = mc.join("versions").join(&dir_name);
    } else if let Some(loader) = loader {
        dir_name = format!("{}-{}-{}", mc_version, loader.kind.name(), loader.version);
        vdir = mc.join("versions").join(&dir_name);
    } else {
        let (name, p) = crate::downloader::resolve_version_dir(&mc, version_id);
        dir_name = name;
        vdir = p;
    }

    // 同版本互斥：路径被其他任务占用时拒绝，避免并发互相覆盖/清理
    let _lock = match task::PathLock::try_acquire(task::global(), &vdir) {
        Some(l) => l,
        None => {
            return set_error(
                &app,
                progress,
                format!("该版本正在被其他任务下载:\n{}", vdir.display()),
            )
        }
    };

    // 步骤分析：初始完整列表（json 步骤恒在，索引 0；零缺失维度检测后过滤）
    let mut specs = task.steps();

    set_phase(&app, progress, "running", &specs[0].name, 0, "running");
    if cancelled(cancel) {
        return set_cancelled(&app, progress);
    }

    // ── 阶段 A：fetch version.json in-memory（唯一硬前置，client/libs/assets/natives/ModLoader 均依赖）──
    let (vj, parent_text) = match fetch_version_json(version_url, &config).await {
        Ok(v) => v,
        Err(e) => return set_error(&app, progress, e),
    };
    step_done(&app, progress, 0);
    if cancelled(cancel) {
        return set_cancelled(&app, progress);
    }

    // 确保目录存在；非 loader 模式下写入原版 version JSON（loader 模式由 installer 写合并 JSON）
    std::fs::create_dir_all(&vdir).ok();
    if loader.is_none() {
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

    // natives 缺失数预计算（下载阶段独立计数器；total 计入 pending 数）
    let natives_pending = count_natives_missing(&vj.libraries, &lib_dir);

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
            // 清单齐了：剩余=全量总数，已完成仅 version json（近似值，检测后覆盖为最终总数）
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

    // ── 重建步骤列表：零缺失维度过滤（json 保持 done）──
    // 步骤 = 任务的函数：缺失检测结果决定哪些维度实际出现在步骤栏
    if lib_pending == 0 {
        specs.retain(|s| s.kind != StepKind::Libraries);
    }
    if asst_pending == 0 {
        specs.retain(|s| s.kind != StepKind::Assets);
    }
    task::update_and_emit(&app, progress, |p| {
        p.steps = specs
            .iter()
            .enumerate()
            .map(|(i, s)| StepInfo {
                name: s.name.clone(),
                status: if i == 0 {
                    "done".into()
                } else {
                    "pending".into()
                },
                progress: if i == 0 { 100.0 } else { 0.0 },
            })
            .collect();
    });

    // ── 动态步骤索引（执行点按 StepKind 定位，无硬编码常量）──
    let pos = |k: StepKind| specs.iter().position(|s| s.kind == k);
    let i_jar = pos(StepKind::ClientJar);
    let i_libs = pos(StepKind::Libraries);
    let i_asst = pos(StepKind::Assets);
    let i_nat = pos(StepKind::Natives);
    let i_cfg = pos(StepKind::LoaderConfig);
    let i_lbs = pos(StepKind::LoaderLibs);
    let i_of = pos(StepKind::OptiFine);
    let i_fab = pos(StepKind::FabricApi);

    // ── 总数：version json + jar + 全量库 + 全量资源 + natives 缺失数 + 安装步骤数 ──
    let mut total_files = 1 + jar_total + lib_total_all + asst_total_all + natives_pending;
    if loader.is_some() {
        total_files += 2; // LoaderConfig + LoaderLibs
    }
    if optifine_patch.is_some() {
        total_files += 1;
    }
    if fabric_api.is_some() {
        total_files += 1;
    }
    let initial_completed = 1 + lib_existing + asst_existing;
    let total_bytes = 1 + jar_total_bytes + lib_total_bytes + asst_total_bytes;
    let initial_bytes = 1 + lib_existing_bytes + asst_existing_bytes;
    set_total(&app, progress, total_files, total_bytes);
    set_completed(&app, progress, initial_completed, initial_bytes);

    // ── 阶段 B：client.jar / libraries / assets / natives / ModLoader / OptiFine / Fabric API 全并行 ──
    let jar_counter = Arc::new(AtomicU32::new(0));
    let jar_bytes_counter = Arc::new(AtomicU64::new(0));
    let jar_total_size = Arc::new(AtomicU64::new(0));
    let lib_counter = Arc::new(AtomicU32::new(0));
    let asst_counter = Arc::new(AtomicU32::new(0));
    let natives_counter = Arc::new(AtomicU32::new(0));
    let bytes_counter = Arc::<AtomicU64>::new(AtomicU64::new(0));
    let cfg_done = Arc::new(AtomicBool::new(false));
    let libs_done = Arc::new(AtomicBool::new(false));
    let optifine_done = Arc::new(AtomicBool::new(false));
    let fabric_done = Arc::new(AtomicBool::new(false));

    // 统一转 running（重建后列表；json 保持 done）——在 spawn EmitLoop 前完成，避免锁竞争
    for (i, s) in specs.iter().enumerate() {
        if s.kind != StepKind::VersionJson {
            set_phase(&app, progress, "", &s.name, i, "running");
        }
    }

    let _emit_handle = EmitLoop {
        app: app.clone(),
        cancel: Arc::clone(cancel),
        progress: Arc::clone(progress),
        jar_done: Arc::clone(&jar_counter),
        lib_done: Arc::clone(&lib_counter),
        asst_done: Arc::clone(&asst_counter),
        natives_done: Arc::clone(&natives_counter),
        jar_bytes: Arc::clone(&jar_bytes_counter),
        jar_total: Arc::clone(&jar_total_size),
        bytes: Arc::clone(&bytes_counter),
        jar_step: i_jar,
        lib_step: i_libs,
        asst_step: i_asst,
        natives_step: i_nat,
        cfg_done: Arc::clone(&cfg_done),
        libs_done: Arc::clone(&libs_done),
        optifine_done: Arc::clone(&optifine_done),
        fabric_done: Arc::clone(&fabric_done),
        lib_pending,
        asst_pending,
        natives_pending,
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
        if let Some(i) = i_jar {
            step_done(&app, progress, i);
        }
        Ok::<_, Vec<String>>(())
    };

    let libs = async {
        crate::downloader::library::download_libraries(
            &vj.libraries,
            &lib_dir,
            config.concurrency as usize,
            &src_refs,
            Arc::clone(&lib_counter),
            Arc::clone(&bytes_counter),
        )
        .await?;
        if let Some(i) = i_libs {
            step_done(&app, progress, i);
        }
        Ok::<_, Vec<String>>(())
    };

    let asst = async {
        crate::downloader::assets::download_assets(
            &vj.asset_index.url,
            &vj.asset_index.id,
            &vj.asset_index.sha1,
            &config,
            config.concurrency as usize,
            Arc::clone(&asst_counter),
            Arc::clone(&bytes_counter),
        )
        .await?;
        if let Some(i) = i_asst {
            step_done(&app, progress, i);
        }
        Ok::<_, Vec<String>>(())
    };

    let natives = async {
        crate::downloader::extract::process_natives(
            &vj.libraries,
            &lib_dir,
            &src_refs,
            Arc::clone(&natives_counter),
            Arc::clone(&bytes_counter),
        )
        .await?;
        if let Some(i) = i_nat {
            step_done(&app, progress, i);
        }
        Ok::<_, Vec<String>>(())
    };

    let loader_dl = async {
        if let Some(loader) = loader {
            // 阶段钩子：安装内部在对应事件点推进 LoaderConfig/LoaderLibs 两步
            let hook = |ev: crate::downloader::mod_loader::LoaderEvent| match ev {
                crate::downloader::mod_loader::LoaderEvent::ConfigDone => {
                    if let Some(i) = i_cfg {
                        step_done(&app, progress, i);
                    }
                    cfg_done.store(true, Ordering::Relaxed);
                }
                crate::downloader::mod_loader::LoaderEvent::LibsDone => {
                    if let Some(i) = i_lbs {
                        step_done(&app, progress, i);
                    }
                    libs_done.store(true, Ordering::Relaxed);
                }
            };
            let result = match loader.kind {
                crate::task::LoaderKind::Fabric => {
                    mod_loader::install_fabric(
                        mc_version,
                        &loader.version,
                        &parent_text,
                        &config,
                        &bytes_counter,
                        Some(&hook),
                    )
                    .await
                }
                crate::task::LoaderKind::Forge => {
                    mod_loader::install_forge(
                        mc_version,
                        &loader.version,
                        &parent_text,
                        &config,
                        &bytes_counter,
                        Some(&hook),
                    )
                    .await
                }
                crate::task::LoaderKind::NeoForge => {
                    mod_loader::install_neoforge(
                        mc_version,
                        &loader.version,
                        &parent_text,
                        &config,
                        &bytes_counter,
                        Some(&hook),
                    )
                    .await
                }
                crate::task::LoaderKind::Quilt => {
                    mod_loader::install_quilt(
                        mc_version,
                        &loader.version,
                        &parent_text,
                        &config,
                        &bytes_counter,
                        Some(&hook),
                    )
                    .await
                }
            };
            if let Err(e) = result {
                return Err(vec![e]);
            }
        }
        Ok::<_, Vec<String>>(())
    };

    let optifine_dl = async {
        if let Some(of) = optifine_patch {
            if let Err(e) = mod_loader::install_optifine(mc_version, of, "HD_U", &config).await {
                return Err(vec![e]);
            }
            if let Some(i) = i_of {
                step_done(&app, progress, i);
            }
            optifine_done.store(true, Ordering::Relaxed);
        }
        Ok::<_, Vec<String>>(())
    };

    let fabric_dl = async {
        if let Some(api) = fabric_api {
            let url = api.url.as_str();
            let ver = api.version.as_str();
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
            // 字节级进度：增量累加进 bytes_counter（EmitLoop 显示速度）
            let file_done = Arc::new(AtomicU64::new(0));
            let progress_cb = {
                let fd = file_done.clone();
                let bc = Arc::clone(&bytes_counter);
                move |downloaded: u64, _total: u64| {
                    let prev = fd.swap(downloaded, Ordering::Relaxed);
                    bc.fetch_add(downloaded.saturating_sub(prev), Ordering::Relaxed);
                }
            };
            match fetcher::fetch_bounded(&urls, &dest, Some(progress_cb)).await {
                Ok(_) => {
                    if let Some(i) = i_fab {
                        step_done(&app, progress, i);
                    }
                    fabric_done.store(true, Ordering::Relaxed);
                }
                Err(e) => return Err(vec![format!("下载 Fabric API 失败\n{}", e)]),
            }
        }
        Ok::<_, Vec<String>>(())
    };

    let parallel = tokio::try_join!(jar, libs, asst, natives, loader_dl, optifine_dl, fabric_dl);

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

    // 收尾（EmitLoop 已完成全部计数自然退出；此处显式设 100%）
    task::update_and_emit(&app, progress, |p| {
        p.status = "done".into();
        p.completed_files = p.total_files;
        p.progress = 100.0;
        p.current_phase = "完成".into();
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
