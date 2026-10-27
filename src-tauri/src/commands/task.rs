//! 下载任务命令：注册与 spawn 在 task/ 域，命令只传参。

use std::path::Path;
use tauri::AppHandle;

use crate::game::modpack::{ModpackRequest, PackSource};
use crate::task::{self, files, model, modpack, repair, store};

/// 提交完整下载/安装任务（注册 TaskManager 并后台执行），返回任务 id；注册/spawn/快照走 task::spawn_download。
#[tauri::command]
pub async fn start_download(app: AppHandle, task: model::DownloadTask) -> Result<String, String> {
    task::spawn_download(app, task)
}

#[tauri::command]
pub async fn download_files(
    app: AppHandle,
    task: model::DownloadFilesTask,
) -> Result<String, String> {
    files::spawn_files_task(app, task)
}

#[tauri::command]
pub async fn download_single_file(
    app: AppHandle,
    task: model::DownloadFileTask,
) -> Result<String, String> {
    task.validate()?;
    let batch = model::DownloadFilesTask {
        version_dir_name: task.version_dir_name,
        files: vec![model::DownloadFileItem {
            kind: task.kind,
            url: task.url,
            filename: task.filename,
            replace: Vec::new(),
        }],
    };
    files::spawn_files_task(app, batch)
}

#[tauri::command]
pub async fn scan_missing_files(version_dir_name: String) -> Result<repair::MissingReport, String> {
    repair::scan_missing(&version_dir_name).await
}

/// 提交补全任务（只下载实际缺失部分，步骤按缺失动态生成），返回任务 id。
#[tauri::command]
pub async fn submit_missing_download(
    app: AppHandle,
    version_dir_name: String,
) -> Result<String, String> {
    let report = repair::scan_missing(&version_dir_name).await?;

    let steps = repair::repair_step_specs(&report);
    crate::task::spawn_task(
        app,
        format!("{} 版本修复", version_dir_name),
        model::TaskText::new(
            "task.backend.name.repair",
            &[("version", version_dir_name.clone())],
        ),
        steps,
        task::store::JobSpec::Repair {
            version_dir_name: version_dir_name.clone(),
        },
        move |a, cancel, progress| {
            tauri::async_runtime::spawn(async move {
                repair::run_repair_task(a, &cancel, &progress, &version_dir_name, report).await;
            });
        },
    )
}

/// 提交 JDK 安装任务（下载缺失 Java 运行时）；完成后自动注册进 jdk 配置，返回任务 id。
#[tauri::command]
pub async fn submit_install_jdk(app: AppHandle, major: u32) -> Result<String, String> {
    task::spawn_install_jdk(app, major)
}

#[tauri::command]
pub fn list_interrupted_tasks() -> Vec<store::InterruptedJob> {
    store::list_interrupted()
}

/// 恢复中断任务：读快照 → 以恢复凭证重新提交（幂等重跑，已下载文件 sha1 命中自动跳过），返回新任务 id。
#[tauri::command]
pub async fn resume_task(app: AppHandle, task_id: String) -> Result<String, String> {
    let (_name, spec) = store::load_spec(&task_id)
        .ok_or_else(|| keyed_msg!("errors.task.snapshotMissing", "任务快照不存在或已过期"))?;
    // 恢复即重跑：旧快照删除，spawn 时写入新快照（新 task_id）
    task::store::remove_snapshot(&task_id);
    match spec {
        store::JobSpec::Download(task) => crate::task::spawn_download(app, task),
        store::JobSpec::Files(task) => {
            // 社区资源无校验数据，恢复 = 重新开始：先清掉上次中断残留的 .elytra-part
            // 半截临时文件（可能被占用/干扰校验）。
            files::cleanup_file_parts(&task);
            files::spawn_files_task(app, task)
        }
        store::JobSpec::Repair { version_dir_name } => {
            submit_missing_download(app, version_dir_name).await
        }
        store::JobSpec::InstallJdk { major } => crate::task::spawn_install_jdk(app, major),
        store::JobSpec::ModpackInstall(spec) => install_modpack(app, spec.request).await,
    }
}

#[tauri::command]
pub fn dismiss_interrupted_task(task_id: String) {
    store::remove_snapshot(&task_id);
}

/// 暂停任务：停下载、保留已下载目录与快照（status=paused），前端可"继续"（resume_task 重跑）。
#[tauri::command]
pub fn pause_task(task_id: String) -> Result<(), String> {
    if !task::global().pause(&task_id) {
        bail_key!("errors.task.notFound", "任务不存在或已结束{task_id}");
    }
    Ok(())
}

#[tauri::command]
pub async fn cancel_task(app: AppHandle, task_id: String) -> Result<(), String> {
    task::global().cancel(&task_id);
    // 立即广播取消态：cancel() 只改内存不发事件，否则心跳事件会把卡片重置回运行中。
    if let Some(p) = task::global()
        .list()
        .into_iter()
        .find(|p| p.task_id == task_id && p.status == model::TaskStatus::Cancelled)
    {
        task::emit(&app, &p);
    }
    Ok(())
}

/// 对挂起等待决策的任务（WaitingForUser）作出选择：skip=跳过该文件继续，retry=重试，cancel=取消任务；非等待状态返回错误。
#[tauri::command]
pub fn decide_waiting_task(task_id: String, action: String) -> Result<(), String> {
    let decision = match action.as_str() {
        "skip" => task::WaitingDecision::Skip,
        "retry" => task::WaitingDecision::Retry,
        "cancel" => task::WaitingDecision::Cancel,
        _ => bail_key!("errors.task.unknownDecision", "未知决策\n{action}"),
    };
    if task::global().decide(&task_id, decision) {
        Ok(())
    } else {
        Err(keyed_msg!("errors.task.notWaiting", "任务未处于等待状态"))
    }
}

/// 任务是否仍注册（存在 = 运行中；终态已移除）；前端预注册后查询，查不到 = 竞态应清理卡片。
#[tauri::command]
pub fn task_exists(task_id: String) -> bool {
    task::global().exists(&task_id)
}

// ── 整合包安装（单任务卡片，步骤表动态增长）──

/// 安装整合包（自动嗅探 Modrinth / CurseForge / MultiMC·Prism / HMCL / MCBBS / PCL / 完整目录，不按扩展名判断）：单任务承载 包体下载/解析 → 本体+ModLoader → 内容+覆盖解压；恢复 = 重跑整卡（各环节幂等），返回任务 id。
#[tauri::command]
pub async fn install_modpack(app: AppHandle, request: ModpackRequest) -> Result<String, String> {
    if request.name.trim().is_empty() {
        return Err(keyed_msg!(
            "errors.task.modpackNameEmpty",
            "整合包名称不能为空"
        ));
    }
    match &request.source {
        PackSource::Local { path } if path.trim().is_empty() => {
            return Err(keyed_msg!(
                "errors.task.modpackPathEmpty",
                "整合包文件路径不能为空"
            ));
        }
        PackSource::Online { url, .. } if url.trim().is_empty() => {
            return Err(keyed_msg!(
                "errors.task.modpackUrlEmpty",
                "整合包下载地址不能为空"
            ));
        }
        _ => {}
    }
    if let PackSource::Local { path } = &request.source {
        if !Path::new(path).is_file() {
            bail_key!("errors.task.modpackFileMissing", "整合包文件不存在\n{path}");
        }
    }

    let steps = task::modpack::initial_step_specs(&request);
    crate::task::spawn_task(
        app,
        format!("整合包：{}", request.name),
        model::TaskText::new(
            "task.backend.name.modpack",
            &[("name", request.name.clone())],
        ),
        steps,
        task::store::JobSpec::ModpackInstall(task::model::ModpackInstallSpec {
            request: request.clone(),
        }),
        move |a, cancel, progress| {
            tauri::async_runtime::spawn(async move {
                modpack::run_install(a, &cancel, &progress, &request).await;
            });
        },
    )
}
