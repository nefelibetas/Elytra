//! 下载任务命令（薄胶水层）：注册任务并 spawn 执行，编排逻辑在 task/runner.rs。

use tauri::AppHandle;

use crate::task::{
    self, files, repair, runner, DownloadFileItem, DownloadFileTask, DownloadFilesTask,
    DownloadTask, TaskSpec,
};

/// 提交完整下载/安装任务：注册到 TaskManager 并后台执行，返回任务 id。
/// 任务配置（含 ModLoader 选择）统一经 DownloadTask 传入，入口先做信任边界校验。
/// 任务名与步骤由 TaskSpec 分析生成（与 runner 执行顺序一致）。
#[tauri::command]
pub async fn start_download(app: AppHandle, task: DownloadTask) -> Result<String, String> {
    task.validate()?;
    spawn_task(
        app,
        task.name(),
        task.steps(),
        crate::task::store::JobSpec::Download(task.clone()),
        move |a, cancel, progress| {
            tauri::async_runtime::spawn(async move {
                runner::run_task(a, &cancel, &progress, &task).await;
            });
        },
    )
}

/// 注册任务并 spawn 执行（命令层共用样板）。
/// `spec`：恢复凭证（JobSpec），spawn 时写入快照（running）；
/// 终态（done/error/cancelled）由 TaskManager::fail/cancel 收口更新。
fn spawn_task(
    app: AppHandle,
    name: String,
    steps: Vec<crate::task::StepSpec>,
    spec: crate::task::store::JobSpec,
    run: impl FnOnce(AppHandle, std::sync::Arc<std::sync::atomic::AtomicBool>, std::sync::Arc<std::sync::Mutex<crate::task::TaskProgress>>) + Send + 'static,
) -> Result<String, String> {
    let step_names: Vec<&str> = steps.iter().map(|s| s.name.as_str()).collect();
    let tm = task::global();
    let (task_id, cancel, progress) = tm.create(&name, &step_names);
    // 持久化快照（崩溃/重启恢复依据；DB 故障静默）
    crate::task::store::save_snapshot(&task_id, &name, &spec, crate::task::TaskStatus::Running);
    run(app, cancel, progress);
    Ok(task_id)
}

/// 批量文件下载任务的注册 + spawn（单文件是批量特例，统一走这里）。
fn spawn_files_task(app: AppHandle, task: DownloadFilesTask) -> Result<String, String> {
    task.validate()?;
    spawn_task(
        app,
        task.name(),
        task.steps(),
        crate::task::store::JobSpec::Files(task.clone()),
        move |a, cancel, progress| {
            tauri::async_runtime::spawn(async move {
                files::run_files_task(a, &cancel, &progress, &task).await;
            });
        },
    )
}

/// 提交批量文件下载任务（多个 mod / 资源包 / 数据包 / 光影包）到下载管理。
/// 返回任务 id。
#[tauri::command]
pub async fn download_files(app: AppHandle, task: DownloadFilesTask) -> Result<String, String> {
    spawn_files_task(app, task)
}

/// 提交单文件下载任务（mod / 资源包 / 数据包 / 光影包）到下载管理。
/// 单文件是批量任务的特例：构造单元素批量任务，统一走批量 runner。
/// 返回任务 id。
#[tauri::command]
pub async fn download_single_file(
    app: AppHandle,
    task: DownloadFileTask,
) -> Result<String, String> {
    task.validate()?;
    let batch = DownloadFilesTask {
        version_dir_name: task.version_dir_name,
        files: vec![DownloadFileItem {
            kind: task.kind,
            url: task.url,
            filename: task.filename,
        }],
    };
    spawn_files_task(app, batch)
}

/// 扫描版本缺失文件（client.jar / 支持库 / 资源），不发起下载。
#[tauri::command]
pub async fn scan_missing_files(version_dir_name: String) -> Result<repair::MissingReport, String> {
    repair::scan_missing(&version_dir_name).await
}

/// 提交补全任务到下载管理：只下载实际缺失的部分，步骤按缺失情况动态生成。
/// 返回任务 id。
#[tauri::command]
pub async fn submit_missing_download(
    app: AppHandle,
    version_dir_name: String,
) -> Result<String, String> {
    let report = repair::scan_missing(&version_dir_name).await?;

    let steps = repair::repair_steps(&report);
    let tm = task::global();
    let name = format!("{} 版本修复", version_dir_name);
    let (task_id, cancel, progress) = tm.create(
        &name,
        &steps.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
    );

    // 持久化快照（崩溃恢复依据；DB 故障静默）
    crate::task::store::save_snapshot(
        &task_id,
        &name,
        &crate::task::store::JobSpec::Repair {
            version_dir_name: version_dir_name.clone(),
        },
        crate::task::TaskStatus::Running,
    );

    tauri::async_runtime::spawn(async move {
        repair::run_repair_task(app, &cancel, &progress, &version_dir_name, report).await;
    });

    Ok(task_id)
}

/// 列出上次进程退出时未完成的任务（running/interrupted 快照），供前端询问恢复。
#[tauri::command]
pub fn list_interrupted_tasks() -> Vec<crate::task::store::InterruptedJob> {
    crate::task::store::list_interrupted()
}

/// 恢复中断任务：读取快照 → 用恢复凭证重新提交（幂等重跑——
/// 已下载文件 sha1 命中自动跳过，等价从断点继续）→ 返回新任务 id。
#[tauri::command]
pub async fn resume_task(app: AppHandle, task_id: String) -> Result<String, String> {
    use crate::task::store::JobSpec;
    let (_name, spec) = crate::task::store::load_spec(&task_id).ok_or("任务快照不存在或已过期")?;
    // 恢复即重跑：旧快照删除，spawn 时写入新快照（新 task_id）
    crate::task::store::remove_snapshot(&task_id);
    match spec {
        JobSpec::Download(task) => start_download(app, task).await,
        JobSpec::Files(task) => download_files(app, task).await,
        JobSpec::Repair { version_dir_name } => {
            submit_missing_download(app, version_dir_name).await
        }
    }
}

/// 放弃恢复：删除任务快照（后续启动不再提示）。
#[tauri::command]
pub fn dismiss_interrupted_task(task_id: String) {
    crate::task::store::remove_snapshot(&task_id);
}

#[tauri::command]
pub async fn cancel_task(task_id: String) -> Result<(), String> {
    task::global().cancel(&task_id);
    Ok(())
}

/// 对挂起等待决策的任务作出选择（WaitingForUser）：skip=跳过该文件继续，retry=重试，cancel=取消任务。
/// 任务未处于等待状态时返回错误（前端弹窗已消失/竞态）。
#[tauri::command]
pub fn decide_waiting_task(task_id: String, action: String) -> Result<(), String> {
    use crate::task::WaitingDecision;
    let decision = match action.as_str() {
        "skip" => WaitingDecision::Skip,
        "retry" => WaitingDecision::Retry,
        "cancel" => WaitingDecision::Cancel,
        _ => return Err(format!("未知决策\n{action}")),
    };
    if task::global().decide(&task_id, decision) {
        Ok(())
    } else {
        Err("任务未处于等待状态".into())
    }
}

/// 任务是否仍注册（存在 = 运行中；终态已移除）。
/// 前端预注册任务后查询：查不到说明终态事件可能先于注册到达（竞态），应清理卡片。
#[tauri::command]
pub fn task_exists(task_id: String) -> bool {
    task::global().exists(&task_id)
}
