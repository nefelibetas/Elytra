//! 任务注册与 spawn：把执行闭包挂到 TaskManager 并落 running 快照。

use parking_lot::Mutex;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use tauri::AppHandle;

use super::manager::global;
use super::model::{DownloadTask, TaskProgress, TaskStatus};
use super::steps::{StepKind, StepSpec, TaskSpec};
use super::store;
use super::{jdk, runner};
use crate::config::downloader::DownloaderConfig;
use crate::downloader;
use crate::jdk::SUPPORTED_JAVA_MAJORS;
use crate::persist;
use crate::task::EventSink;

/// 注册任务并 spawn 执行（任务域统一入口，命令层只传参数与执行闭包）。
/// `spec`：恢复凭证（JobSpec），spawn 时写入快照（running）；
/// 终态（done/error/cancelled）由 TaskManager::fail/cancel 收口更新。
pub(crate) fn spawn_task(
    app: AppHandle,
    name: String,
    steps: Vec<StepSpec>,
    spec: store::JobSpec,
    run: impl FnOnce(Arc<dyn EventSink>, Arc<AtomicBool>, Arc<Mutex<TaskProgress>>) + Send + 'static,
) -> Result<String, String> {
    let sink: Arc<dyn EventSink> = Arc::new(app);
    spawn_task_with_sink(sink, name, steps, spec, run)
}

/// 与 [`spawn_task`] 同逻辑，但事件出口由调用方注入。
///
/// 生产路径经 `spawn_task`（`AppHandle` 是 [`EventSink`] 的 blanket impl，包一次即得 sink）；
/// 集成测试直接注入记录型 sink，从而覆盖「注册 → running 快照 → 执行闭包」整条路径，
/// 而不必绕过入口直接调任务体。
pub fn spawn_task_with_sink(
    sink: Arc<dyn EventSink>,
    name: String,
    steps: Vec<StepSpec>,
    spec: store::JobSpec,
    run: impl FnOnce(Arc<dyn EventSink>, Arc<AtomicBool>, Arc<Mutex<TaskProgress>>) + Send + 'static,
) -> Result<String, String> {
    let step_names: Vec<&str> = steps.iter().map(|s| s.name.as_str()).collect();
    let tm = global();
    // 每次任务启动同步全局限速（用户改配置后不重启也生效）
    let cfg: DownloaderConfig = persist::downloader_config();
    downloader::fetcher::set_speed_limit(cfg.max_speed);
    let (task_id, cancel, progress) = tm.create(&name, &step_names);
    // 持久化快照（崩溃/重启恢复依据；DB 故障静默）
    store::save_snapshot(&task_id, &name, &spec, TaskStatus::Running);
    run(sink, cancel, progress);
    Ok(task_id)
}

/// 由步骤名构造 StepSpec（kind 取 File 仅占位——供修复/整合包等
/// 无 TaskSpec 驱动的 runner 使用，进度只按名称呈现）。
pub(crate) fn steps_by_names(names: Vec<String>) -> Vec<StepSpec> {
    names
        .into_iter()
        .map(|name| StepSpec {
            name,
            kind: StepKind::File,
        })
        .collect()
}

/// 完整下载/安装任务：校验 → 注册 → spawn（编排在 task/runner.rs）。
pub fn spawn_download(app: AppHandle, task: DownloadTask) -> Result<String, String> {
    task.validate()?;
    spawn_task(
        app,
        task.name(),
        task.steps(),
        store::JobSpec::Download(task.clone()),
        move |a, cancel, progress| {
            tauri::async_runtime::spawn(async move {
                runner::run_task(a, &cancel, &progress, &task).await;
            });
        },
    )
}

/// JDK 安装任务：支持列表校验 → 单步注册 → spawn（执行体在 task/jdk.rs）。
pub fn spawn_install_jdk(app: AppHandle, major: u32) -> Result<String, String> {
    if !SUPPORTED_JAVA_MAJORS.contains(&major) {
        bail!("暂不支持安装 Java {major}（仅支持 8/17/21/25）");
    }
    spawn_task(
        app,
        format!("下载 Java {major}"),
        vec![StepSpec {
            name: "下载 Java 运行时".into(),
            kind: StepKind::File,
        }],
        store::JobSpec::InstallJdk { major },
        move |a, cancel, progress| {
            tauri::async_runtime::spawn(async move {
                jdk::run_install_jdk(a, &cancel, &progress, major).await;
            });
        },
    )
}
