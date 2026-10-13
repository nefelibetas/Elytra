//! 自动任务执行上下文：内置动作（备份/记录/打包）只依赖这里暴露的信息，便于单测构造。

use std::path::{Path, PathBuf};

/// 实例配置文件（放在版本目录内，点开头避免被当成游戏文件）。
pub const INSTANCE_FILE: &str = ".elytra-instance.toml";

pub struct AutoTaskCtx<'a> {
    /// `.minecraft` 根（放全局日志 `logs/elytra-launch.log`）。
    pub mc_dir: &'a Path,
    /// 实例目录 `versions/{id}`（也是游戏 cwd）。
    pub game_dir: &'a Path,
    pub version_id: &'a str,
    pub config_dir: &'a Path,
    /// 子进程退出码（仅 `on_crash` / `post_exit` 有）。
    pub exit_code: Option<i32>,
    /// 最新崩溃报告路径（仅崩溃判定成立时有）。
    pub crash_report: Option<&'a str>,
}

impl AutoTaskCtx<'_> {
    /// 实例配置文件路径。
    pub fn instance_file(&self) -> PathBuf {
        self.game_dir.join(INSTANCE_FILE)
    }
}
