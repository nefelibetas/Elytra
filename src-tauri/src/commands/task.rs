//! 下载任务命令（薄胶水层）：注册任务并 spawn 执行，编排逻辑在 task/runner.rs。

use tauri::AppHandle;

use crate::task::{self, runner};

/// 提交完整下载/安装任务：注册到 TaskManager 并后台执行，返回任务 id。
#[allow(clippy::too_many_arguments)]
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
    fabric_api_url: Option<String>,
    fabric_api_version: Option<String>,
) -> Result<String, String> {
    let task_name = match (&main_loader, &main_loader_version) {
        (Some(ld), Some(lv)) => format!("{} {}-{} 安装", mc_version, ld, lv),
        _ => format!("{} 下载", version_id),
    };

    let mut steps = vec!["下载原版json文件", "下载原版客户端文件", "下载原版支持库文件", "下载原版资源文件"];
    if main_loader.is_some() { steps.push("安装 ModLoader"); }
    if optifine_patch.is_some() { steps.push("安装 OptiFine"); }
    if fabric_api_version.is_some() { steps.push("安装 Fabric API"); }

    let tm = task::global();
    let (task_id, cancel, progress) = tm.create(&task_name, &steps);

    tauri::async_runtime::spawn(async move {
        runner::run_task(app, &cancel, &progress, &version_id, &version_url, &mc_version,
                         main_loader.as_deref(), main_loader_version.as_deref(),
                         optifine_patch.as_deref(), version_dir_name,
                         fabric_api_url.as_deref(), fabric_api_version.as_deref()).await;
    });

    Ok(task_id)
}

/// 扫描版本缺失文件（client.jar / 支持库 / 资源），不发起下载。
#[tauri::command]
pub async fn scan_missing_files(version_dir_name: String) -> Result<runner::MissingReport, String> {
    runner::scan_missing(&version_dir_name).await
}

/// 提交补全任务到下载管理：只下载实际缺失的部分，步骤按缺失情况动态生成。
/// 返回任务 id。
#[tauri::command]
pub async fn submit_missing_download(
    app: AppHandle,
    version_dir_name: String,
) -> Result<String, String> {
    let report = runner::scan_missing(&version_dir_name).await?;

    let steps = runner::repair_steps(&report);
    let tm = task::global();
    let (task_id, cancel, progress) = tm.create(
        &format!("{} 版本修复", version_dir_name),
        &steps.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
    );

    tauri::async_runtime::spawn(async move {
        runner::run_repair_task(app, &cancel, &progress, &version_dir_name, report).await;
    });

    Ok(task_id)
}

#[tauri::command]
pub async fn cancel_task(task_id: String) -> Result<(), String> {
    task::global().cancel(&task_id);
    Ok(())
}
