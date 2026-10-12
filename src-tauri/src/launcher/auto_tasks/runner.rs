//! 自动任务执行器：按阶段跑动作列表，逐动作推进度事件，单个失败不中断后续。
//!
//! 是否因失败中止由**调用方**决策（`pre_launch` 看 [`AutoTasks::pre_on_failure`]），
//! 这里只负责执行与汇总，保持纯粹、可测。

use std::time::{Duration, Instant};

use super::ctx::AutoTaskCtx;
use super::model::{
    ordered_steps, AutoTaskAction, AutoTaskFailureInfo, AutoTasksFinished, AutoTaskPhase, AutoTaskProgress, AutoTasks,
};
use super::{backup, crashpack, record};
use crate::events;
use crate::task::EventSink;

/// 命令超时的轮询间隔。
const POLL_MS: u64 = 50;

/// 单次执行的进度上报器（把 sink/ctx/phase/total 收拢成一个上下文，
/// 避免逐帧透传 8 个参数）。
struct Progress<'a> {
    sink: &'a dyn EventSink,
    ctx: &'a AutoTaskCtx<'a>,
    phase: AutoTaskPhase,
    total: u32,
}

impl Progress<'_> {
    /// 推进度帧（高频；派发端按「实例 + 阶段」合并为最新一帧）。
    fn emit(&self, step_index: u32, action: &str, message: &str, done: bool) {
        crate::task::emit_event(
            self.sink,
            events::AUTO_TASK_PROGRESS,
            AutoTaskProgress {
                version_id: self.ctx.version_id.to_string(),
                phase: self.phase,
                step_index,
                step_total: self.total,
                action: action.to_string(),
                message: message.to_string(),
                done,
            },
        );
    }
}

/// 执行一个阶段的全部动作，返回失败列表（空 = 全部成功）。
pub fn run_auto_tasks(
    sink: &dyn EventSink,
    ctx: &AutoTaskCtx<'_>,
    phase: AutoTaskPhase,
    auto_tasks: &AutoTasks,
) -> Vec<AutoTaskFailureInfo> {
    let steps = ordered_steps(phase.steps(auto_tasks));
    if steps.is_empty() {
        return Vec::new();
    }
    let progress = Progress {
        sink,
        ctx,
        phase,
        total: steps.len() as u32,
    };
    let mut failures = Vec::new();
    for (idx, step) in steps.iter().enumerate() {
        let idx = idx as u32;
        let label = step.action.kind_label();
        progress.emit(idx, label, "正在执行…", false);
        match run_one(&progress, idx, &step.action) {
            Ok(msg) => progress.emit(idx, label, &msg, true),
            Err(e) => {
                progress.emit(idx, label, &e, true);
                failures.push(AutoTaskFailureInfo {
                    action: label.to_string(),
                    message: e,
                });
            }
        }
    }
    crate::task::emit_event(
        sink,
        events::AUTO_TASKS_FINISHED,
        AutoTasksFinished {
            version_id: ctx.version_id.to_string(),
            phase,
            failures: failures.clone(),
        },
    );
    failures
}

/// 执行单个动作。
fn run_one(progress: &Progress<'_>, idx: u32, action: &AutoTaskAction) -> Result<String, String> {
    match action {
        AutoTaskAction::BackupSaves { scope, keep } => {
            let mut on_step = |done: usize, all: usize| {
                let msg = format!("已复制 {done}/{all} 个文件");
                progress.emit(idx, "backup_saves", &msg, false);
            };
            let out = backup::backup_saves(progress.ctx.game_dir, scope, *keep, &mut on_step)?;
            Ok(format!(
                "已备份 {} 个世界 / {} 个文件（{}）",
                out.worlds,
                out.files,
                out.dir.display()
            ))
        }
        AutoTaskAction::RecordMods => {
            let path = record::record_mods(
                progress.ctx.config_dir,
                progress.ctx.version_id,
                progress.ctx.game_dir,
            )?;
            Ok(format!("已记录 mod 清单（{}）", path.display()))
        }
        AutoTaskAction::PackCrashLogs => {
            let path = crashpack::pack_crash_logs(progress.ctx)?;
            Ok(format!("已打包崩溃日志（{}）", path.display()))
        }
        AutoTaskAction::Command {
            command,
            timeout_secs,
        } => run_command(command, *timeout_secs),
    }
}

/// 跑一条自定义命令行（Windows `cmd /C`，其余 `sh -c`），带超时；超时即杀进程。
fn run_command(command: &str, timeout_secs: u32) -> Result<String, String> {
    let mut cmd = command_shell(command);

    let mut child = cmd
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(common_err_ctx!("启动命令失败"))?;
    let timeout = Duration::from_secs(u64::from(timeout_secs.max(1)));
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => return Ok("命令执行成功".to_string()),
            Ok(Some(status)) => {
                return Err(format!(
                    "命令退出码 {}",
                    status.code().unwrap_or(-1)
                ));
            }
            Ok(None) => {}
            Err(e) => return Err(format!("等待命令失败\n{e}")),
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!("命令超时（{} 秒）", timeout_secs));
        }
        std::thread::sleep(Duration::from_millis(POLL_MS));
    }
}

#[cfg(target_os = "windows")]
fn command_shell(command: &str) -> std::process::Command {
    use std::os::windows::process::CommandExt as _;
    let mut cmd = std::process::Command::new("cmd");
    cmd.args(["/C", command]);
    // CREATE_NO_WINDOW：自动任务命令不应弹出控制台窗口
    cmd.creation_flags(0x0800_0000);
    cmd
}

#[cfg(not(target_os = "windows"))]
fn command_shell(command: &str) -> std::process::Command {
    let mut cmd = std::process::Command::new("sh");
    cmd.args(["-c", command]);
    cmd
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::launcher::auto_tasks::model::{AutoTaskStep, AutoTasks};
    use parking_lot::Mutex;
    use std::path::PathBuf;

    /// 记录型事件出口（不依赖 tauri）。
    #[derive(Default)]
    struct RecordSink {
        events: Mutex<Vec<(String, serde_json::Value)>>,
    }

    impl EventSink for RecordSink {
        fn send(&self, event: &'static str, payload: serde_json::Value) {
            self.events.lock().push((event.to_string(), payload));
        }
    }

    fn tmp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("elytra-autotasks-runner-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn ok_cmd() -> String {
        if cfg!(windows) {
            "exit /b 0".into()
        } else {
            "exit 0".into()
        }
    }

    fn fail_cmd(code: i32) -> String {
        if cfg!(windows) {
            format!("exit /b {code}")
        } else {
            format!("exit {code}")
        }
    }

    /// 进度事件里 (step_index, action) 序列（去重连续同帧，只看每步首次出现）。
    fn step_actions(sink: &RecordSink) -> Vec<(u64, String)> {
        let events = sink.events.lock();
        let mut out: Vec<(u64, String)> = Vec::new();
        for (name, payload) in events.iter() {
            if name != "auto-task-progress" {
                continue;
            }
            let idx = payload["step_index"].as_u64().unwrap_or(0);
            let action = payload["action"].as_str().unwrap_or("").to_string();
            if out.last().map(|(i, a)| (*i != idx || *a != action)).unwrap_or(true) {
                out.push((idx, action));
            }
        }
        out
    }

    #[test]
    fn runs_builtin_before_custom_command() {
        let root = tmp_dir("order");
        let config = root.join("cfg");
        let game = root.join("game");
        std::fs::create_dir_all(game.join("mods")).unwrap();
        std::fs::write(game.join("mods/a.jar"), b"x").unwrap();
        let auto_tasks = AutoTasks {
            pre_launch: vec![
                AutoTaskStep {
                    enabled: true,
                    action: AutoTaskAction::Command {
                        command: ok_cmd(),
                        timeout_secs: 30,
                    },
                },
                AutoTaskStep {
                    enabled: true,
                    action: AutoTaskAction::RecordMods,
                },
            ],
            ..Default::default()
        };
        let sink = RecordSink::default();
        let ctx = AutoTaskCtx {
            mc_dir: &root,
            game_dir: &game,
            version_id: "inst",
            config_dir: &config,
            username: "",
            exit_code: None,
            crash_report: None,
        };
        let failures = run_auto_tasks(&sink, &ctx, AutoTaskPhase::PreLaunch, &auto_tasks);
        assert!(failures.is_empty(), "都应成功: {failures:?}");
        assert!(config.join("records/inst/mods.json").exists(), "内置动作产物应存在");
        let steps = step_actions(&sink);
        assert_eq!(
            steps.first().map(|(_, a)| a.as_str()),
            Some("record_mods"),
            "内置动作必须排在自定义命令之前，实际: {steps:?}"
        );
    }

    #[test]
    fn failure_does_not_stop_remaining_actions() {
        let root = tmp_dir("continue");
        let game = root.join("game");
        std::fs::create_dir_all(&game).unwrap();
        let auto_tasks = AutoTasks {
            post_exit: vec![
                AutoTaskStep {
                    enabled: true,
                    action: AutoTaskAction::Command {
                        command: fail_cmd(3),
                        timeout_secs: 30,
                    },
                },
                AutoTaskStep {
                    enabled: true,
                    action: AutoTaskAction::Command {
                        command: ok_cmd(),
                        timeout_secs: 30,
                    },
                },
            ],
            ..Default::default()
        };
        let sink = RecordSink::default();
        let ctx = AutoTaskCtx {
            mc_dir: &root,
            game_dir: &game,
            version_id: "inst",
            config_dir: &root,
            username: "",
            exit_code: Some(1),
            crash_report: None,
        };
        let failures = run_auto_tasks(&sink, &ctx, AutoTaskPhase::PostExit, &auto_tasks);
        assert_eq!(failures.len(), 1, "只有第一条失败: {failures:?}");
        assert!(
            failures[0].message.contains('3'),
            "错误应含退出码: {}",
            failures[0].message
        );
        let steps = step_actions(&sink);
        assert!(
            steps.iter().any(|(idx, _)| *idx == 1),
            "失败后仍须执行第二步，事件: {steps:?}"
        );
    }

    #[test]
    fn emits_finished_event_with_failures() {
        let root = tmp_dir("finished");
        let game = root.join("game");
        std::fs::create_dir_all(&game).unwrap();
        let bad = if cfg!(windows) { "exit /b 1" } else { "exit 1" };
        let auto_tasks = AutoTasks {
            on_crash: vec![AutoTaskStep {
                enabled: true,
                action: AutoTaskAction::Command {
                    command: bad.to_string(),
                    timeout_secs: 30,
                },
            }],
            ..Default::default()
        };
        let sink = RecordSink::default();
        let ctx = AutoTaskCtx {
            mc_dir: &root,
            game_dir: &game,
            version_id: "inst",
            config_dir: &root,
            username: "",
            exit_code: Some(1),
            crash_report: None,
        };
        run_auto_tasks(&sink, &ctx, AutoTaskPhase::OnCrash, &auto_tasks);
        let events = sink.events.lock();
        let finished = events
            .iter()
            .find(|(name, _)| name == "auto-tasks-finished")
            .expect("应发出 auto-tasks-finished");
        assert_eq!(finished.1["failures"].as_array().unwrap().len(), 1);
        assert_eq!(finished.1["phase"], "on_crash");
    }

    #[test]
    fn no_steps_emits_nothing() {
        let root = tmp_dir("empty");
        let sink = RecordSink::default();
        let ctx = AutoTaskCtx {
            mc_dir: &root,
            game_dir: &root,
            version_id: "inst",
            config_dir: &root,
            username: "",
            exit_code: None,
            crash_report: None,
        };
        let failures = run_auto_tasks(&sink, &ctx, AutoTaskPhase::PreLaunch, &AutoTasks::default());
        assert!(failures.is_empty());
        assert!(sink.events.lock().is_empty(), "无步骤不应产生事件");
    }

    #[test]
    fn command_timeout_kills_process() {
        // 用 1 秒超时跑一条更久的命令（Windows ping 4 次 ≈ 3s；Unix sleep 3）
        let slow = if cfg!(windows) {
            "ping -n 4 127.0.0.1"
        } else {
            "sleep 3"
        };
        let started = Instant::now();
        let err = run_command(slow, 1).unwrap_err();
        assert!(err.contains("超时"), "应报超时: {err}");
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "超时应及时返回，实际 {:?}",
            started.elapsed()
        );
    }

    #[test]
    fn command_success_reported() {
        let ok = if cfg!(windows) { "exit /b 0" } else { "exit 0" };
        assert!(run_command(ok, 30).is_ok());
    }

    #[test]
    fn command_failure_reports_exit_code() {
        let bad = if cfg!(windows) { "exit /b 7" } else { "exit 7" };
        let err = run_command(bad, 30).unwrap_err();
        assert!(err.contains('7'), "应含退出码: {err}");
    }
}
