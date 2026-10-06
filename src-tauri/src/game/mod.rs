//! 游戏内容域：版本元数据、资源装配、ModLoader 安装、社区资源、整合包、Mod 更新。
//! 传输能力统一取自 `crate::downloader`（纯下载层）。

pub mod community;
pub mod loader;
pub mod modpack;
pub mod resource;
pub mod updates;
pub mod version;
