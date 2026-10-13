//! 统一 ModLoader 安装入口：按种类分派四个安装器。

use crate::task::LoaderKind;

use super::ctx::{InstallCtx, VersionTarget};
use super::fabric::{install_fabric, install_quilt};
use super::forge::install_forge;
use super::neoforge::install_neoforge;

/// runner（完整下载）与 task/modpack（整合包）共用入口。
/// `ctx.cancel` 仅 Forge/NeoForge 使用（installer 式需 kill 子进程），Fabric/Quilt 忽略。
pub async fn install_loader(
    kind: LoaderKind,
    mc_version: &str,
    loader_version: &str,
    target: VersionTarget<'_>,
    parent_text: &str,
    ctx: InstallCtx<'_>,
) -> Result<(), String> {
    match kind {
        LoaderKind::Fabric => {
            install_fabric(mc_version, loader_version, target, parent_text, ctx).await
        }
        LoaderKind::Quilt => {
            install_quilt(mc_version, loader_version, target, parent_text, ctx).await
        }
        LoaderKind::Forge => {
            install_forge(mc_version, loader_version, target, parent_text, ctx).await
        }
        LoaderKind::NeoForge => install_neoforge(loader_version, target, parent_text, ctx).await,
    }
}
