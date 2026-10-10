//! 工具箱「多线程下载器」配置（纯数据，无逻辑）。

use serde::{Deserialize, Serialize};

/// 默认同时下载的文件数（与 [`ToolDlConfig::default`] 一致；调度侧缓存初值同此）。
pub const DEFAULT_MAX_FILES: u32 = 3;
/// 默认单文件分段数。
pub const DEFAULT_SEGMENTS: u32 = 8;

/// 工具箱下载器配置（persist section `tooldl`）。
#[derive(Debug, Clone, Serialize, Deserialize, ts_rs::TS)]
#[ts(export_to = "config/")]
#[serde(default)]
pub struct ToolDlConfig {
    /// 保存目录（空 = 尚未选择，此时不允许开始下载）
    pub save_dir: String,
    /// 同时下载的文件数上限
    pub max_files: u32,
    /// 单文件分段数（1 = 单连接）
    pub segments: u32,
}

impl Default for ToolDlConfig {
    fn default() -> Self {
        Self {
            save_dir: String::new(),
            max_files: DEFAULT_MAX_FILES,
            segments: DEFAULT_SEGMENTS,
        }
    }
}
