//! 下载任务命令（薄胶水层）：注册任务并 spawn 执行，编排逻辑在 task/runner.rs。

use std::path::Path;
use tauri::AppHandle;

use crate::config::downloader::model::DownloaderConfig;
use crate::constants;
use crate::downloader::fetcher;
use crate::downloader::modpack::{ModpackRequest, PackSource};
use crate::persist;
use crate::task::{
    self, files, modpack, repair, runner, DownloadFileItem, DownloadFileKind, DownloadFileTask,
    DownloadFilesTask, DownloadTask, TaskSpec, TaskStatus,
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
        task::store::JobSpec::Download(task.clone()),
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
    steps: Vec<task::StepSpec>,
    spec: task::store::JobSpec,
    run: impl FnOnce(
            AppHandle,
            std::sync::Arc<std::sync::atomic::AtomicBool>,
            std::sync::Arc<parking_lot::Mutex<task::TaskProgress>>,
        ) + Send
        + 'static,
) -> Result<String, String> {
    let step_names: Vec<&str> = steps.iter().map(|s| s.name.as_str()).collect();
    let tm = task::global();
    // 每次任务启动同步全局限速（用户改配置后不重启也生效）
    let cfg: DownloaderConfig =
        persist::read(constants::SECT_DOWNLOADER).unwrap_or_default();
    fetcher::set_speed_limit(cfg.max_speed);
    let (task_id, cancel, progress) = tm.create(&name, &step_names);
    // 持久化快照（崩溃/重启恢复依据；DB 故障静默）
    task::store::save_snapshot(&task_id, &name, &spec, TaskStatus::Running);
    run(app, cancel, progress);
    Ok(task_id)
}

/// 批量文件下载任务的注册 + spawn（单文件是批量特例，统一走这里）。
fn spawn_files_task(app: AppHandle, task: DownloadFilesTask) -> Result<String, String> {
    // 前端传入的 version_dir_name 是已安装版本的实际目录名（get_installed_versions
    // 返回目录名，含冲突追加的 _{n}），直接校验目录存在即可；
    // mod / 资源包 / 数据包 / 光影包严格落盘到版本隔离目录 versions/{目录}/{kind_dir}/{filename}。
    let config: DownloaderConfig = persist::read(constants::SECT_DOWNLOADER).unwrap_or_default();
    let dir = config
        .minecraft_dir_abs()
        .join("versions")
        .join(&task.version_dir_name);
    if !dir.is_dir() {
        return Err(format!(
            "目标版本目录不存在（请先在版本选择页选择已安装的版本）\n版本目录: {}",
            dir.display()
        ));
    }
    task.validate()?;
    spawn_task(
        app,
        task.name(),
        task.steps(),
        task::store::JobSpec::Files(task.clone()),
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
    let (task_id, cancel, progress) =
        tm.create(&name, &steps.iter().map(|s| s.as_str()).collect::<Vec<_>>());

    // 持久化快照（崩溃恢复依据；DB 故障静默）
    task::store::save_snapshot(
        &task_id,
        &name,
        &task::store::JobSpec::Repair {
            version_dir_name: version_dir_name.clone(),
        },
        TaskStatus::Running,
    );

    tauri::async_runtime::spawn(async move {
        repair::run_repair_task(app, &cancel, &progress, &version_dir_name, report).await;
    });

    Ok(task_id)
}

/// 提交 JDK 安装任务（下载缺失 Java 运行时）到任务中心。
/// 单步任务；完成后自动注册进 jdk 配置，前端完成监听自动选中。
/// 返回任务 id。
#[tauri::command]
pub async fn submit_install_jdk(app: AppHandle, major: u32) -> Result<String, String> {
    if !crate::jdk::SUPPORTED_JAVA_MAJORS.contains(&major) {
        return Err(format!("暂不支持安装 Java {major}（仅支持 8/17/21/25）"));
    }
    let tm = task::global();
    let name = format!("下载 Java {major}");
    let steps = ["下载 Java 运行时"];
    let (task_id, cancel, progress) = tm.create(&name, &steps);

    // 持久化快照（崩溃恢复依据；DB 故障静默）
    task::store::save_snapshot(
        &task_id,
        &name,
        &task::store::JobSpec::InstallJdk { major },
        TaskStatus::Running,
    );

    tauri::async_runtime::spawn(async move {
        runner::run_install_jdk(app, &cancel, &progress, major).await;
    });

    Ok(task_id)
}

/// 列出上次进程退出时未完成的任务（running/interrupted 快照），供前端询问恢复。
#[tauri::command]
pub fn list_interrupted_tasks() -> Vec<task::store::InterruptedJob> {
    task::store::list_interrupted()
}

/// 恢复中断任务：读取快照 → 用恢复凭证重新提交（幂等重跑——
/// 已下载文件 sha1 命中自动跳过，等价从断点继续）→ 返回新任务 id。
#[tauri::command]
pub async fn resume_task(app: AppHandle, task_id: String) -> Result<String, String> {
    use crate::task::store::JobSpec;
    let (_name, spec) = task::store::load_spec(&task_id).ok_or("任务快照不存在或已过期")?;
    // 恢复即重跑：旧快照删除，spawn 时写入新快照（新 task_id）
    task::store::remove_snapshot(&task_id);
    match spec {
        JobSpec::Download(task) => start_download(app, task).await,
        JobSpec::Files(task) => {
            // 社区资源无校验数据，恢复 = 重新开始：先清掉上次中断残留的
            // .elytra-part 半截临时文件（陈旧残留可能被占用/校验干扰），
            // 确保重新下载干净起步
            cleanup_file_parts(&task);
            download_files(app, task).await
        }
        JobSpec::Repair { version_dir_name } => {
            submit_missing_download(app, version_dir_name).await
        }
        JobSpec::InstallJdk { major } => submit_install_jdk(app, major).await,
        JobSpec::ModpackInstall(spec) => install_modpack(app, spec.request).await,
    }
}

/// 删除批量文件任务的全部 .elytra-part 残留（恢复社区资源下载前调用）。
/// worlds 任务清理 `saves/.elytra-worlds/` 临时区（含半截 zip/unpack），替代按 kind_dir 的清理。
fn cleanup_file_parts(task: &DownloadFilesTask) {
    let config: DownloaderConfig = persist::read(constants::SECT_DOWNLOADER).unwrap_or_default();
    let mc = config.minecraft_dir_abs();
    for f in &task.files {
        if f.kind == DownloadFileKind::World {
            let stage = mc
                .join("versions")
                .join(&task.version_dir_name)
                .join("saves")
                .join(".elytra-worlds");
            let _ = std::fs::remove_dir_all(&stage);
            continue;
        }
        let dest = mc
            .join("versions")
            .join(&task.version_dir_name)
            .join(f.kind.kind_dir())
            .join(&f.filename);
        let tmp = dest.with_file_name(format!("{}.elytra-part", f.filename));
        let _ = std::fs::remove_file(tmp);
    }
}

/// 放弃恢复：删除任务快照（后续启动不再提示）。
#[tauri::command]
pub fn dismiss_interrupted_task(task_id: String) {
    task::store::remove_snapshot(&task_id);
}

/// 暂停任务：停下载、保留已下载目录与快照（status=paused），
/// 供前端显示"继续"（resume_task 重跑快照，已完成文件 sha1 跳过）。
#[tauri::command]
pub fn pause_task(task_id: String) -> Result<(), String> {
    if !task::global().pause(&task_id) {
        return Err(format!(
            "任务不存在或已结束
{task_id}"
        ));
    }
    Ok(())
}

#[tauri::command]
pub async fn cancel_task(app: AppHandle, task_id: String) -> Result<(), String> {
    task::global().cancel(&task_id);
    // 立即广播取消态：cancel() 只改内存不发事件，此前要等任务侧下个检查点
    // 才反映，期间心跳事件会把刚取消的卡片重置回运行中（"复活"观感）
    if let Some(p) = task::global()
        .list()
        .into_iter()
        .find(|p| p.task_id == task_id && p.status == TaskStatus::Cancelled)
    {
        task::emit(&app, &p);
    }
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

// ── 整合包安装（单任务卡片：解析 → 本体 → 内容，步骤表动态增长）──

/// 安装整合包（Modrinth .mrpack / CurseForge zip）。
/// 单一任务全程承载：包体下载/解析 → 游戏本体+ModLoader → 内容文件+覆盖解压。
/// 恢复 = 重跑整卡（各环节 sha1/扫描幂等，已就位内容零重下）。返回任务 id。
#[tauri::command]
pub async fn install_modpack(app: AppHandle, request: ModpackRequest) -> Result<String, String> {
    if request.name.trim().is_empty() {
        return Err("整合包名称不能为空".into());
    }
    match &request.source {
        PackSource::Local { path } if path.trim().is_empty() => {
            return Err("整合包文件路径不能为空".into());
        }
        PackSource::Online { url, .. } if url.trim().is_empty() => {
            return Err("整合包下载地址不能为空".into());
        }
        _ => {}
    }
    if let PackSource::Local { path } = &request.source {
        if !Path::new(path).is_file() {
            return Err(format!("整合包文件不存在\n{path}"));
        }
    }

    let tm = task::global();
    let name = format!("整合包：{}", request.name);
    let steps = modpack::initial_steps(&request);
    let (task_id, cancel, progress) = tm.create(&name, &steps);
    task::store::save_snapshot(
        &task_id,
        &name,
        &task::store::JobSpec::ModpackInstall(task::model::ModpackInstallSpec {
            request: request.clone(),
        }),
        TaskStatus::Running,
    );
    tauri::async_runtime::spawn(async move {
        modpack::run_install(&app, &cancel, &progress, &request).await;
    });
    Ok(task_id)
}
