//! 自动任务执行上下文（参数收敛：路径 + 版本标识 + 退出信息）。
//!
//! 内置动作（备份 / 记录 / 打包）都只依赖这里暴露的信息，便于单测直接构造。

use std::path::{Path, PathBuf};

/// 实例配置文件（放在版本目录内，点开头避免被当成游戏文件）。
pub const INSTANCE_FILE: &str = ".elytra-instance.toml";

/// 单次自动任务执行的上下文。
pub struct AutoTaskCtx<'a> {
    /// `.minecraft` 根（放全局日志 `logs/elytra-launch.log`）。
    pub mc_dir: &'a Path,
    /// 实例目录 `versions/{id}`（也是游戏 cwd）。
    pub game_dir: &'a Path,
    /// 版本目录名（= 实例 id）。
    pub version_id: &'a str,
    /// 配置目录（清单 / 崩溃包输出根）。
    pub config_dir: &'a Path,
    /// 当前账号昵称（崩溃包脱敏用；离线/第三方账号同样有值）。
    pub username: &'a str,
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
