//! ModLoader 安装的公共参数结构。

use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use super::LoaderHook;
use crate::config::downloader::DownloaderConfig;
use crate::downloader::progress::ProgressCounters;

/// 安装目标版本目录（由 runner / 整合包解析，含冲突重命名 `{id}_n` 语义）。
#[derive(Clone, Copy)]
pub struct VersionTarget<'a> {
    pub dir_name: &'a str,
    pub vdir: &'a Path,
}

/// 安装上下文：下载配置、进度计数、取消令牌、阶段钩子。
#[derive(Clone, Copy)]
pub struct InstallCtx<'a> {
    pub config: &'a DownloaderConfig,
    pub pc: &'a ProgressCounters,
    pub cancel: &'a Arc<AtomicBool>,
    pub hook: Option<LoaderHook<'a>>,
}
