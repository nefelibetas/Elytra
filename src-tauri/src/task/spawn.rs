//! 任务注册与 spawn：把执行闭包挂到 TaskManager 并落 running 快照。

use parking_lot::Mutex;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use tauri::AppHandle;

use super::manager::global;
use super::model::{DownloadTask, StepKind, TaskProgress, TaskStatus, TaskText};
use super::steps::{StepSpec, TaskSpec};
use super::store;
use super::{jdk, runner};
use crate::config::downloader::DownloaderConfig;
use crate::downloader;
use crate::jdk::SUPPORTED_JAVA_MAJORS;
use crate::persist;
use crate::task::EventSink;

/// 注册任务并 spawn 执行（任务域统一入口）。`spec` 为恢复凭证，spawn 时写入 running 快照；终态由 TaskManager::fail/cancel 收口。
pub(crate) fn spawn_task(
    app: AppHandle,
    name: String,
    text: TaskText,
    steps: Vec<StepSpec>,
    spec: store::JobSpec,
    run: impl FnOnce(Arc<dyn EventSink>, Arc<AtomicBool>, Arc<Mutex<TaskProgress>>) + Send + 'static,
) -> Result<String, String> {
    let sink: Arc<dyn EventSink> = Arc::new(app);
    spawn_task_with_sink(sink, name, text, steps, spec, run)
}

/// 同 [`spawn_task`]，事件出口由调用方注入：生产用 `AppHandle`，集成测试注入记录型 sink。
pub fn spawn_task_with_sink(
    sink: Arc<dyn EventSink>,
    name: String,
    text: TaskText,
    steps: Vec<StepSpec>,
    spec: store::JobSpec,
    run: impl FnOnce(Arc<dyn EventSink>, Arc<AtomicBool>, Arc<Mutex<TaskProgress>>) + Send + 'static,
) -> Result<String, String> {
    let tm = global();
    // 每次任务启动同步全局限速（用户改配置后不重启也生效）
    let cfg: DownloaderConfig = persist::downloader_config();
    downloader::fetcher::set_speed_limit(cfg.max_speed);
    let (task_id, cancel, progress) = tm.create(&name, &text, &steps);
    // 持久化快照（崩溃/重启恢复依据；DB 故障静默）
    store::save_snapshot(&task_id, &name, &spec, TaskStatus::Running);
    run(sink, cancel, progress);
    Ok(task_id)
}

/// 完整下载/安装任务：校验 → 注册 → spawn（编排在 task/runner.rs）。
pub fn spawn_download(app: AppHandle, task: DownloadTask) -> Result<String, String> {
    task.validate()?;
    let text = match &task.loader {
        Some(loader) => TaskText::new(
            "task.backend.name.install",
            &[
                ("mc", task.mc_version.clone()),
                ("loader", loader.kind.display_name().to_string()),
                ("version", loader.version.clone()),
            ],
        ),
        None => TaskText::new(
            "task.backend.name.download",
            &[("mc", task.version_id.clone())],
        ),
    };
    spawn_task(
        app,
        task.name(),
        text,
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
        bail_key!("errors.jdk.install", "暂不支持安装 Java {major}（仅支持 8/17/21/25）");
    }
    spawn_task(
        app,
        format!("下载 Java {major}"),
        TaskText::new(
            "task.backend.name.jdk",
            &[("major", major.to_string())],
        ),
        vec![StepSpec {
            name: "下载 Java 运行时".into(),
            kind: StepKind::JavaRuntime,
        }],
        store::JobSpec::InstallJdk { major },
        move |a, cancel, progress| {
            tauri::async_runtime::spawn(async move {
                jdk::run_install_jdk(a, &cancel, &progress, major).await;
            });
        },
    )
}
