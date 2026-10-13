//! 自动任务命令（薄胶水）：实例自动任务读写 + 立即执行一次。

use std::path::PathBuf;

use tauri::AppHandle;

use crate::launcher::auto_tasks::ctx::AutoTaskCtx;
use crate::launcher::auto_tasks::model::{AutoTaskPhase, AutoTasks};
use crate::launcher::auto_tasks::runner;
use crate::persist;
use crate::persist::instance;
use crate::util;

/// 实例自动任务状态：`global` 作为默认值与"自定义"的预填内容；
/// `instance` 为 `Some` 时该实例完全采用自己的自动任务。
#[derive(serde::Serialize, ts_rs::TS)]
#[ts(export_to = "launcher/")]
pub struct InstanceAutoTasksState {
    pub global: AutoTasks,
    pub instance: Option<AutoTasks>,
}

/// 当前生效的 Minecraft 目录（下载配置里的 minecraft_dir，绝对路径）。
fn mc_dir() -> PathBuf {
    persist::downloader_config().minecraft_dir_abs()
}

/// 读取实例自动任务状态（全局 + 实例）。
#[tauri::command]
pub fn get_instance_auto_tasks(version_id: String) -> InstanceAutoTasksState {
    InstanceAutoTasksState {
        global: persist::launch_options().auto_tasks,
        instance: instance::read_auto_tasks(&mc_dir(), &version_id),
    }
}

/// 保存实例自动任务；`tasks = None` 表示删除实例配置（回退全局）。
#[tauri::command]
pub fn save_instance_auto_tasks(version_id: String, tasks: Option<AutoTasks>) -> Result<(), String> {
    instance::write_auto_tasks(&mc_dir(), &version_id, tasks.as_ref())
}

/// 保存全局自动任务（读改写整个 `launcher` section，避免前端回传整份 LaunchOptions）。
#[tauri::command]
pub fn save_global_auto_tasks(tasks: AutoTasks) -> Result<(), String> {
    let mut options = persist::launch_options();
    options.auto_tasks = tasks;
    persist::save_launch_options(&options).map_err(common_err_ctx!("保存配置失败"))
}

/// 立即执行一次某阶段的自动任务（跳过启动流程，便于调试与应急）。
/// 在阻塞线程执行（备份/打包可能数十秒），不占用 UI 线程。
#[tauri::command]
pub async fn run_auto_tasks_now(
    app: AppHandle,
    version_id: String,
    phase: String,
) -> Result<(), String> {
    let phase = AutoTaskPhase::parse(&phase).ok_or_else(|| format!("未知自动任务阶段\n{phase}"))?;
    let root = mc_dir();
    let game_dir = root.join("versions").join(&version_id);
    if !game_dir.is_dir() {
        bail!("版本目录不存在\n{}", game_dir.display());
    }
    let auto_tasks = instance::read_auto_tasks(&root, &version_id)
        .unwrap_or_else(|| persist::launch_options().auto_tasks);

    tauri::async_runtime::spawn_blocking(move || {
        let config_dir = util::config_dir();
        let ctx = AutoTaskCtx {
            mc_dir: &root,
            game_dir: &game_dir,
            version_id: &version_id,
            config_dir: &config_dir,
            exit_code: None,
            crash_report: None,
        };
        let failures = runner::run_auto_tasks(&app, &ctx, phase, &auto_tasks);
        if failures.is_empty() {
            return Ok(());
        }
        let detail = failures
            .iter()
            .map(|f| format!("[{}] {}", f.action, f.message))
            .collect::<Vec<_>>()
            .join("\n");
        Err(format!("自动任务执行失败\n{detail}"))
    })
    .await
    .map_err(|e| format!("自动任务执行线程失败\n{e}"))?
}
