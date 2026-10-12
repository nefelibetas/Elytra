//! TypeScript 类型导出入口（**非测试路径**）：`cargo run --bin gen_ts --features gen-ts`。
//!
//! 所有需要镜像到前端的 Rust 类型在此登记（新增类型加一行）。漏登记不会静默：
//! 前端 import 该类型时会编译报错（`src/types/<域>/<Type>.ts` 不存在）。
//!
//! 不在 `#[cfg(test)]` 里导出的原因：跑测试会写文件，容易误伤（CI/本地跑测试
//! 都会改工作区），故独立成 bin，只在显式调用时生成。

use ts_rs::TS;

use crate::config::downloader::{
    DownloadSource, DownloadSourceMode, DownloaderConfig, JdkSourceMode, NetworkMode,
};
use crate::config::game_dirs::{GameDir, GameDirsConfig};
use crate::auth::external::{ExternalAccount, ExternalConfig, ExternalProfile, ExternalServer};
use crate::auth::model::MsAccount;
use crate::config::online::OnlineConfig;
use crate::config::tooldl::ToolDlConfig;
use crate::jdk::model::{JdkInfo, JdkOptions, JdkScanCache, JdkSelection};
use crate::launcher::model::{GcType, LaunchOptions, MemoryMode, ProcessPriority, WindowSize};
use crate::task::files::WaitingUserPayload;
use crate::task::model::{
    DownloadFileKind, DownloadTask, FabricApi, LoaderChoice, LoaderKind, StepInfo, StepStatus,
    TaskProgress, TaskStatus,
};
use crate::tooldl::{ToolDlSnapshot, ToolDlStatus};

/// 导出全部登记类型到 `TS_RS_EXPORT_DIR`（见 `src-tauri/.cargo/config.toml`）。
pub fn export_all() {
    DownloaderConfig::export().unwrap();
    DownloadSource::export().unwrap();
    NetworkMode::export().unwrap();
    DownloadSourceMode::export().unwrap();
    JdkSourceMode::export().unwrap();
    GameDir::export().unwrap();
    GameDirsConfig::export().unwrap();
    OnlineConfig::export().unwrap();
    ToolDlConfig::export().unwrap();
    ToolDlStatus::export().unwrap();
    ToolDlSnapshot::export().unwrap();
    TaskStatus::export().unwrap();
    TaskProgress::export().unwrap();
    StepStatus::export().unwrap();
    StepInfo::export().unwrap();
    LoaderKind::export().unwrap();
    LoaderChoice::export().unwrap();
    FabricApi::export().unwrap();
    DownloadTask::export().unwrap();
    DownloadFileKind::export().unwrap();
    LaunchOptions::export().unwrap();
    MemoryMode::export().unwrap();
    ProcessPriority::export().unwrap();
    WindowSize::export().unwrap();
    GcType::export().unwrap();
    JdkInfo::export().unwrap();
    JdkOptions::export().unwrap();
    JdkScanCache::export().unwrap();
    JdkSelection::export().unwrap();
    MsAccount::export().unwrap();
    ExternalAccount::export().unwrap();
    ExternalProfile::export().unwrap();
    ExternalServer::export().unwrap();
    ExternalConfig::export().unwrap();
    WaitingUserPayload::export().unwrap();
}
