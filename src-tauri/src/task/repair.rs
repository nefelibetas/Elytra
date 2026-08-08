// ── 缺失扫描与修复任务（补全垂直切片）──
// scan_missing 被命令层调用；run_repair_task 为补全任务主体。
// count_natives_missing / count_assets_all 为"缺失统计"共享助手，run_task 亦复用。

use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use tauri::AppHandle;

use crate::config::downloader::model::{DownloadSource, DownloaderConfig};
use crate::constants;
use crate::downloader::model::{Library, VersionJson};
use crate::persist;
use crate::task::{self, StepStatus, TaskProgress, TaskStatus};

use super::progress::{
    cancelled, set_cancelled, set_completed, set_error, set_phase, set_total, spawn_tail_loop,
    step_done, EmitLoop,
};

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
fn read_version_json(vdir: &Path) -> Result<VersionJson, String> {
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
pub fn count_natives_missing(libraries: &[Library], lib_dir: &Path) -> u32 {
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

/// 缺失 natives 的总字节（与 count_natives_missing 同过滤条件，取 artifact.size 求和）。
pub fn natives_missing_bytes(libraries: &[Library], lib_dir: &Path) -> u64 {
    use crate::downloader::extract::legacy_classifier_artifact;
    use crate::downloader::hash::file_sha1_matches;
    libraries
        .iter()
        .filter_map(|lib| {
            let artifact = legacy_classifier_artifact(lib)?;
            let dest = lib_dir.join(&artifact.path);
            if dest.exists() && file_sha1_matches(&dest, &artifact.sha1) {
                None
            } else {
                Some(artifact.size)
            }
        })
        .sum()
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
        task::update_and_emit(&app, progress, |p| {
            p.status = TaskStatus::Done;
            p.progress = 100.0;
            p.current_phase = "文件完整，无需修复".into();
        });
        // 终态统一移除后端任务注册
        if let Ok(p) = progress.try_lock() {
            task::global().fail(&p.task_id);
        }
        return;
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
        set_phase(&app, progress, Some(TaskStatus::Running), "修复中", i, StepStatus::Running);
    }

    let jar_counter = Arc::new(AtomicU32::new(0));
    let lib_counter = Arc::new(AtomicU32::new(0));
    let asst_counter = Arc::new(AtomicU32::new(0));
    // jar 字节独立计数（下载回调是覆盖式），libs/assets 字节为累加式
    let jar_bytes = Arc::new(AtomicU64::new(0));
    let jar_total = Arc::new(AtomicU64::new(0));
    let bytes_counter = Arc::<AtomicU64>::new(AtomicU64::new(0));

    // repair 无 ModLoader 安装步骤：安装步骤字节/总数字段全为占位（0 时公式回退，不显示进度）
    let emit_handle = EmitLoop {
        app: app.clone(),
        cancel: Arc::clone(cancel),
        progress: Arc::clone(progress),
        jar_done: Arc::clone(&jar_counter),
        lib_done: Arc::clone(&lib_counter),
        asst_done: Arc::clone(&asst_counter),
        // repair 的 natives 在 EmitLoop 生命周期之外单独执行，计数/标志全为占位
        natives_done: Arc::new(AtomicU32::new(0)),
        jar_bytes: Arc::clone(&jar_bytes),
        jar_total: Arc::clone(&jar_total),
        bytes: Arc::clone(&bytes_counter),
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
        jar_step: jar_idx,
        lib_step: libs_idx,
        asst_step: asst_idx,
        natives_step: None,
        loader_libs_step: None,
        optifine_step: None,
        fabric_step: None,
        cfg_done: Arc::new(AtomicBool::new(false)),
        libs_done: Arc::new(AtomicBool::new(false)),
        optifine_done: Arc::new(AtomicBool::new(false)),
        fabric_done: Arc::new(AtomicBool::new(false)),
        lib_pending,
        asst_pending,
        natives_pending: 0,
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
                // 逐源 sha1 校验下载：镜像 CDN 返回不完整/错误内容自动换源重试
                crate::downloader::fetcher::fetch_verified(&urls, &jar_path, Some(cb), &c.sha1)
                    .await
                    .map_err(|e| vec![e.to_string()])?;
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
        Arc::new(AtomicU64::new(0)), // 占位：repair 分步进度走文件数回退
    );

    let asst = crate::downloader::assets::download_assets(
        &vj.asset_index.url,
        &vj.asset_index.id,
        &vj.asset_index.sha1,
        &config,
        config.concurrency as usize,
        Arc::clone(&asst_counter),
        Arc::clone(&bytes_counter),
        Arc::new(AtomicU64::new(0)), // 占位
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

    // 收尾心跳：natives 等后续步骤期间保持速度/字节进度（EmitLoop 已 abort）
    let _tail_loop = spawn_tail_loop(&app, progress, Arc::clone(&bytes_counter));

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
        set_phase(&app, progress, None, "处理 native 库", i, StepStatus::Running);
        if let Err(errors) = crate::downloader::extract::process_natives(
            &vj.libraries,
            &lib_dir,
            &src_refs,
            Arc::clone(&lib_counter),
            Arc::clone(&bytes_counter),
            Arc::new(AtomicU64::new(0)), // 占位：repair 分步进度走文件数回退
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
        p.status = TaskStatus::Done;
        p.completed_files = p.total_files;
        p.progress = 100.0;
        p.current_phase = "修复完成".into();
    });
    // 终态统一移除后端任务注册
    if let Ok(p) = progress.try_lock() {
        task::global().fail(&p.task_id);
    }
}

/// Count total and pending assets (sha1-matched against local cache, multi-threaded).
/// 拉取/解析失败返回 Err（调用方显式报错，不再静默视为"完整"）。
/// `index_id`/`index_sha1` 非空时走 fetch_asset_index 的本地缓存（读缓存命中则免网络，否则拉取并写缓存）。
pub async fn count_assets_all(
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
