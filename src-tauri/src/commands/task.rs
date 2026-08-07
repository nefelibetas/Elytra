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
        move |a, cancel, progress| {
            tauri::async_runtime::spawn(async move {
                runner::run_task(a, &cancel, &progress, &task).await;
            });
        },
    )
}

/// 注册任务并 spawn 执行（命令层共用样板）。
fn spawn_task(
    app: AppHandle,
    name: String,
    steps: Vec<crate::task::StepSpec>,
    run: impl FnOnce(AppHandle, std::sync::Arc<std::sync::atomic::AtomicBool>, std::sync::Arc<std::sync::Mutex<crate::task::TaskProgress>>) + Send + 'static,
) -> Result<String, String> {
    let step_names: Vec<&str> = steps.iter().map(|s| s.name.as_str()).collect();
    let tm = task::global();
    let (task_id, cancel, progress) = tm.create(&name, &step_names);
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
    let (task_id, cancel, progress) = tm.create(
        &format!("{} 版本修复", version_dir_name),
        &steps.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
    );

    tauri::async_runtime::spawn(async move {
        repair::run_repair_task(app, &cancel, &progress, &version_dir_name, report).await;
    });

    Ok(task_id)
}

#[tauri::command]
pub async fn cancel_task(task_id: String) -> Result<(), String> {
    task::global().cancel(&task_id);
    Ok(())
}
