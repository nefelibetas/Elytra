//! ModLoader 引擎：按种类安装（profile 式 Fabric/Quilt、installer 式 Forge/NeoForge、
//! OptiFine 分场景）与版本列表查询。
//!
//! 实现划分：
//! - [`hook`]：安装阶段事件与钩子类型；
//! - [`ctx`]：安装参数结构（目标版本目录 + 安装上下文）；
//! - [`shared`]：profile 合并 / 继承展开 / 幂等判定 / 安装器进程管理；
//! - [`dispatch`]：按种类分派的统一入口；
//! - [`fabric`] / [`forge`] / [`neoforge`] / [`optifine`]：各安装器实现；
//! - [`processors`]：按 installer 声明的 install_profile.processors 执行（不写死工具/配方）；
//! - [`version_query`]：各 ModLoader 可用版本列表查询。

pub mod fabric;
pub mod forge;
pub mod neoforge;
pub mod optifine;
pub mod processors;
pub mod version_query;

mod ctx;
mod dispatch;
mod hook;
mod shared;

pub use ctx::{InstallCtx, VersionTarget};
pub use dispatch::install_loader;
pub use fabric::{install_fabric, install_quilt};
pub use forge::install_forge;
pub use hook::{LoaderEvent, LoaderHook};
pub use neoforge::install_neoforge;
pub use optifine::{install_optifine, OptifineMode};
pub(crate) use shared::{
    expand_inheritance, merge_loader_profile, processor_outputs_are_current, run_installer_command,
};
