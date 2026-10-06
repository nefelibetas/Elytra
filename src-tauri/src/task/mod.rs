//! 任务域：TaskManager 单例、事件派发、任务注册与各类任务实现。
//!
//! 实现划分：
//! - [`manager`]：TaskManager 单例（注册表 / 终态收口 / 挂起决策）；
//! - [`emit`]：事件派发队列线程与进度锁外的 emit；
//! - [`spawn`]：任务注册与 spawn 入口；
//! - [`model`]：serde 任务配置与进度类型；
//! - [`steps`]：步骤分析；[`progress`]：进度上报；[`repair`]：缺失扫描与修复；
//! - [`files`]：批量文件下载；[`runner`]：完整下载主体；[`jdk`]：JDK 安装任务；
//! - [`modpack`]：整合包安装任务；[`store`]：SQLite 快照与恢复。

pub mod files;
pub mod jdk;
pub mod model;
pub mod modpack;
pub mod progress;
pub mod repair;
pub mod runner;
pub mod steps;
pub mod store;

mod ctx;
mod emit;
mod manager;
mod spawn;

pub use manager::{global, init, TaskManager, WaitingDecision};
pub use model::{
    DownloadFileItem, DownloadFileKind, DownloadFileTask, DownloadFilesTask, DownloadTask,
    FabricApi, LoaderChoice, LoaderKind, ModpackInstallSpec, StepInfo, StepStatus, TaskProgress,
    TaskStatus,
};
pub use spawn::{spawn_download, spawn_install_jdk};
pub use steps::{StepKind, StepSpec, TaskSpec};

pub(crate) use ctx::TaskCtx;
pub(crate) use emit::{emit, emit_event, update_and_emit};
pub(crate) use spawn::{spawn_task, steps_by_names};
