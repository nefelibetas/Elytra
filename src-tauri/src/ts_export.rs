//! TypeScript 类型导出入口：`cargo run --bin gen_ts --features gen-ts`。
//!
//! 需镜像到前端的 Rust 类型在此登记；漏登记时前端 import 会编译报错
//! （`src/types/<域>/<Type>.ts` 不存在）。独立为 bin 而非 `#[cfg(test)]`：
//! 跑测试会写文件、污染工作区，故只在显式调用时生成。

use ts_rs::TS;

use crate::auth::external::{ExternalAccount, ExternalConfig, ExternalProfile, ExternalServer};
use crate::auth::model::MsAccount;
use crate::commands::auto_tasks::InstanceAutoTasksState;
use crate::commands::map::MapBlockInfo;
use crate::config::downloader::{
    DownloadSource, DownloadSourceMode, DownloaderConfig, JdkSourceMode, NetworkMode,
};
use crate::config::game_dirs::{GameDir, GameDirsConfig};
use crate::config::online::OnlineConfig;
use crate::config::tooldl::ToolDlConfig;
use crate::game::community::CurseForgeKeySource;
use crate::game::version::dir::InstalledVersionInfo;
use crate::game::version::watch::{VersionDirChanged, VersionsChanged};
use crate::jdk::model::{JdkInfo, JdkOptions, JdkScanCache, JdkSelection};
use crate::launcher::auto_tasks::model::{
    AutoTaskAction, AutoTaskFailure, AutoTaskFailureInfo, AutoTaskPhase, AutoTaskProgress,
    AutoTaskStep, AutoTasks, AutoTasksFinished, BackupScope,
};
use crate::launcher::model::{GcType, LaunchOptions, MemoryMode, ProcessPriority, WindowSize};
use crate::map::folder::MapFolderEntry;
use crate::map::region_io::MapRegionInfo;
use crate::map::search::{MapBiomeHit, MapBiomeSummary};
use crate::map::world_info::MapWorldInfo;
use crate::seedmap::{
    ResolvedVersion, SeedMapBlock, SeedMapSlimeChunk, SeedMapStructure, SeedMapVersion,
};
use crate::task::files::WaitingUserPayload;
use crate::task::model::{
    DownloadFileKind, DownloadTask, FabricApi, LoaderChoice, LoaderKind, StepInfo, StepKind,
    StepStatus, TaskProgress, TaskStatus,
};
use crate::tooldl::{ToolDlSnapshot, ToolDlStatus};

/// 导出全部登记类型到 `TS_RS_EXPORT_DIR`（见 `src-tauri/.cargo/config.toml`）。
pub fn export_all() {
    DownloaderConfig::export().unwrap();
    CurseForgeKeySource::export().unwrap();
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
    StepKind::export().unwrap();
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
    BackupScope::export().unwrap();
    AutoTaskAction::export().unwrap();
    AutoTaskStep::export().unwrap();
    AutoTaskFailure::export().unwrap();
    AutoTaskPhase::export().unwrap();
    AutoTasks::export().unwrap();
    AutoTaskProgress::export().unwrap();
    AutoTaskFailureInfo::export().unwrap();
    AutoTasksFinished::export().unwrap();
    InstanceAutoTasksState::export().unwrap();
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
    InstalledVersionInfo::export().unwrap();
    VersionsChanged::export().unwrap();
    VersionDirChanged::export().unwrap();
    MapWorldInfo::export().unwrap();
    MapRegionInfo::export().unwrap();
    MapFolderEntry::export().unwrap();
    MapBlockInfo::export().unwrap();
    MapBiomeHit::export().unwrap();
    MapBiomeSummary::export().unwrap();
    SeedMapBlock::export().unwrap();
    SeedMapSlimeChunk::export().unwrap();
    SeedMapStructure::export().unwrap();
    SeedMapVersion::export().unwrap();
    ResolvedVersion::export().unwrap();
}
