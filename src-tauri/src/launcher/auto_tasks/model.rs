//! 自动任务的数据模型（前端 TS 镜像经 ts-rs 生成到 `src/types/launcher/`）。

use serde::{Deserialize, Serialize};

/// 备份范围：全部世界 / 指定世界（`saves/` 下的目录名）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export_to = "launcher/")]
pub enum BackupScope {
    All,
    Selected { worlds: Vec<String> },
}

/// 单个自动任务动作：内置（Rust 实现）或自定义（系统命令行）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export_to = "launcher/")]
pub enum AutoTaskAction {
    /// 备份存档：排除 lock/tmp，按时间戳目录保留 `keep` 份。
    BackupSaves { scope: BackupScope, keep: u32 },
    /// 记录 mod 清单 JSON（复用 `game/updates` 的扫描能力）。
    RecordMods,
    /// 崩溃日志打包（含脱敏）。
    PackCrashLogs,
    /// 自定义命令行：Windows 走 `cmd /C`，其余走 `sh -c`。
    Command { command: String, timeout_secs: u32 },
}

impl AutoTaskAction {
    /// 事件/UI 用的动作种类标签（前端据此选图标与文案）。
    pub fn kind_label(&self) -> &'static str {
        match self {
            AutoTaskAction::BackupSaves { .. } => "backup_saves",
            AutoTaskAction::RecordMods => "record_mods",
            AutoTaskAction::PackCrashLogs => "pack_crash_logs",
            AutoTaskAction::Command { .. } => "command",
        }
    }

    /// 内置动作（排序时排在自定义命令之前）。
    pub fn is_builtin(&self) -> bool {
        !matches!(self, AutoTaskAction::Command { .. })
    }
}

/// 单个自动任务步骤（可单独停用而不删除）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export_to = "launcher/")]
pub struct AutoTaskStep {
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub action: AutoTaskAction,
}

/// `pre_launch` 失败时的行为。
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
#[ts(export_to = "launcher/")]
pub enum AutoTaskFailure {
    /// 终止启动（默认）。
    #[default]
    Abort,
    /// 忽略失败，继续启动。
    Continue,
}

/// 三个时点的自动任务集合（全局与实例同构）。默认 `on_crash` 打包崩溃日志、`post_exit` 记录 mod 清单开启，`pre_launch` 备份存档关闭（体积大）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[serde(default)]
#[ts(export_to = "launcher/")]
pub struct AutoTasks {
    pub pre_launch: Vec<AutoTaskStep>,
    pub on_crash: Vec<AutoTaskStep>,
    pub post_exit: Vec<AutoTaskStep>,
    pub pre_on_failure: AutoTaskFailure,
}

impl Default for AutoTasks {
    fn default() -> Self {
        Self {
            pre_launch: Vec::new(),
            on_crash: vec![AutoTaskStep {
                enabled: true,
                action: AutoTaskAction::PackCrashLogs,
            }],
            post_exit: vec![AutoTaskStep {
                enabled: true,
                action: AutoTaskAction::RecordMods,
            }],
            pre_on_failure: AutoTaskFailure::Abort,
        }
    }
}

/// 执行阶段（事件载荷与前端共用）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
#[ts(export_to = "launcher/")]
pub enum AutoTaskPhase {
    PreLaunch,
    OnCrash,
    PostExit,
}

impl AutoTaskPhase {
    /// 事件载荷/配置字段名（与 [`AutoTasks`] 的字段名一致）。
    pub fn as_str(self) -> &'static str {
        match self {
            AutoTaskPhase::PreLaunch => "pre_launch",
            AutoTaskPhase::OnCrash => "on_crash",
            AutoTaskPhase::PostExit => "post_exit",
        }
    }

    pub fn steps(self, auto_tasks: &AutoTasks) -> &[AutoTaskStep] {
        match self {
            AutoTaskPhase::PreLaunch => &auto_tasks.pre_launch,
            AutoTaskPhase::OnCrash => &auto_tasks.on_crash,
            AutoTaskPhase::PostExit => &auto_tasks.post_exit,
        }
    }

    /// 由配置字段名解析阶段（命令层入参校验）。
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "pre_launch" => Some(AutoTaskPhase::PreLaunch),
            "on_crash" => Some(AutoTaskPhase::OnCrash),
            "post_exit" => Some(AutoTaskPhase::PostExit),
            _ => None,
        }
    }
}

/// 进度事件载荷（`auto-task-progress`；高频，按 version_id 合并最新帧）。
#[derive(Debug, Clone, Serialize, ts_rs::TS)]
#[ts(export_to = "launcher/")]
pub struct AutoTaskProgress {
    pub version_id: String,
    pub phase: AutoTaskPhase,
    /// 当前步骤下标（0-based，指向**排序后**的步骤列表）。
    pub step_index: u32,
    pub step_total: u32,
    pub action: String,
    pub message: String,
    /// 该步骤是否已完成（true = 收尾帧）。
    pub done: bool,
}

#[derive(Debug, Clone, Serialize, ts_rs::TS)]
#[ts(export_to = "launcher/")]
pub struct AutoTaskFailureInfo {
    pub action: String,
    pub message: String,
}

/// 终态汇总事件载荷（`auto-tasks-finished`；低频 FIFO，前端据此弹 dialog）。
#[derive(Debug, Clone, Serialize, ts_rs::TS)]
#[ts(export_to = "launcher/")]
pub struct AutoTasksFinished {
    pub version_id: String,
    pub phase: AutoTaskPhase,
    pub failures: Vec<AutoTaskFailureInfo>,
}

/// 备份默认保留份数。
pub fn default_keep() -> u32 {
    5
}

/// 自定义命令默认超时（秒）。
pub fn default_timeout_secs() -> u32 {
    120
}

fn default_true() -> bool {
    true
}

/// 执行序：内置动作在前、自定义命令在后；同类内保持用户顺序（稳定排序）。
pub fn ordered_steps(steps: &[AutoTaskStep]) -> Vec<AutoTaskStep> {
    let mut out: Vec<AutoTaskStep> = steps.iter().filter(|s| s.enabled).cloned().collect();
    out.sort_by_key(|s| !s.action.is_builtin());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_tasks_toml_roundtrip() {
        let auto_tasks = AutoTasks {
            pre_launch: vec![
                AutoTaskStep {
                    enabled: true,
                    action: AutoTaskAction::BackupSaves {
                        scope: BackupScope::Selected {
                            worlds: vec!["生存".into(), "红石".into()],
                        },
                        keep: 5,
                    },
                },
                AutoTaskStep {
                    enabled: false,
                    action: AutoTaskAction::Command {
                        command: "echo hi".into(),
                        timeout_secs: 30,
                    },
                },
            ],
            post_exit: vec![AutoTaskStep {
                enabled: true,
                action: AutoTaskAction::RecordMods,
            }],
            pre_on_failure: AutoTaskFailure::Continue,
            on_crash: vec![],
        };
        let text = toml::to_string(&auto_tasks).unwrap();
        let back: AutoTasks = toml::from_str(&text).unwrap();
        assert_eq!(back, auto_tasks, "AutoTasks 序列化应可无损往返");
    }

    #[test]
    fn auto_tasks_default_when_section_missing() {
        // 旧配置无 [auto_tasks] 字段（或整段缺失）必须解析为默认值
        #[derive(Deserialize)]
        struct Wrapper {
            #[serde(default)]
            auto_tasks: AutoTasks,
        }
        let w: Wrapper = toml::from_str("").unwrap();
        assert_eq!(w.auto_tasks, AutoTasks::default());
        assert_eq!(w.auto_tasks.pre_on_failure, AutoTaskFailure::Abort);
    }

    #[test]
    fn step_enabled_defaults_true() {
        // enabled 省略时默认启用
        let step: AutoTaskStep = toml::from_str("action = \"RecordMods\"").unwrap();
        assert!(step.enabled);
    }

    #[test]
    fn auto_tasks_default_enables_records_but_not_backup() {
        let auto_tasks = AutoTasks::default();
        assert!(auto_tasks.pre_launch.is_empty(), "备份默认不开");
        assert_eq!(auto_tasks.on_crash.len(), 1);
        assert_eq!(auto_tasks.on_crash[0].action, AutoTaskAction::PackCrashLogs);
        assert_eq!(auto_tasks.post_exit.len(), 1);
        assert_eq!(auto_tasks.post_exit[0].action, AutoTaskAction::RecordMods);
    }

    #[test]
    fn ordered_steps_puts_builtin_first() {
        let steps = vec![
            AutoTaskStep {
                enabled: true,
                action: AutoTaskAction::Command {
                    command: "a".into(),
                    timeout_secs: 1,
                },
            },
            AutoTaskStep {
                enabled: true,
                action: AutoTaskAction::RecordMods,
            },
            AutoTaskStep {
                enabled: true,
                action: AutoTaskAction::Command {
                    command: "b".into(),
                    timeout_secs: 1,
                },
            },
            AutoTaskStep {
                enabled: true,
                action: AutoTaskAction::PackCrashLogs,
            },
        ];
        let ordered = ordered_steps(&steps);
        assert_eq!(
            ordered
                .iter()
                .map(|s| s.action.kind_label())
                .collect::<Vec<_>>(),
            vec!["record_mods", "pack_crash_logs", "command", "command"],
            "内置应整体排在自定义之前，且同类内保持用户序"
        );
    }

    #[test]
    fn ordered_steps_skips_disabled() {
        let steps = vec![
            AutoTaskStep {
                enabled: false,
                action: AutoTaskAction::RecordMods,
            },
            AutoTaskStep {
                enabled: true,
                action: AutoTaskAction::PackCrashLogs,
            },
        ];
        let ordered = ordered_steps(&steps);
        assert_eq!(ordered.len(), 1);
        assert_eq!(ordered[0].action.kind_label(), "pack_crash_logs");
    }

    #[test]
    fn phase_maps_to_field() {
        let auto_tasks = AutoTasks {
            pre_launch: vec![AutoTaskStep {
                enabled: true,
                action: AutoTaskAction::RecordMods,
            }],
            on_crash: vec![],
            post_exit: vec![],
            pre_on_failure: AutoTaskFailure::Abort,
        };
        assert_eq!(AutoTaskPhase::PreLaunch.steps(&auto_tasks).len(), 1);
        assert_eq!(AutoTaskPhase::OnCrash.steps(&auto_tasks).len(), 0);
        assert_eq!(AutoTaskPhase::PostExit.steps(&auto_tasks).len(), 0);
        assert_eq!(AutoTaskPhase::PostExit.as_str(), "post_exit");
        assert_eq!(AutoTaskPhase::parse("on_crash"), Some(AutoTaskPhase::OnCrash));
        assert_eq!(AutoTaskPhase::parse("bogus"), None);
    }

    #[test]
    fn kind_labels_stable() {
        // 前端据标签选图标，标签是契约的一部分
        assert_eq!(AutoTaskAction::RecordMods.kind_label(), "record_mods");
        assert_eq!(AutoTaskAction::PackCrashLogs.kind_label(), "pack_crash_logs");
        assert_eq!(
            AutoTaskAction::BackupSaves {
                scope: BackupScope::All,
                keep: 1
            }
            .kind_label(),
            "backup_saves"
        );
        assert_eq!(
            AutoTaskAction::Command {
                command: String::new(),
                timeout_secs: 1
            }
            .kind_label(),
            "command"
        );
    }

    #[test]
    fn defaults_match_frontend_contract() {
        assert_eq!(default_keep(), 5);
        assert_eq!(default_timeout_secs(), 120);
    }
}
