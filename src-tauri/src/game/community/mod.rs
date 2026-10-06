//! 社区资源搜索 / 版本查询（Modrinth 官方 API + CurseForge 官方 API），
//! 覆盖 Mod / 光影 / 资源包 / 数据包 / 整合包 / 世界等类别（category 区分）。
//! 两平台 ID 体系、鉴权与返回结构完全不同，故分开实现、统一出参；
//! CurseForge 需要 x-api-key（constants.rs 经 .env 注入），Modrinth 无需 key。
//! 简介翻译由 MCIM 镜像提供（英文 → 中文），失败静默降级为英文。
//!
//! 模块布局：[`api`]（出参类型 + 分派入口）、[`shared`]（查询通道与版本比较）、
//! [`modrinth`] / [`curseforge`]（各平台实现）、[`translation`]（MCIM 翻译）、
//! [`wiki_entry`]（MC百科内置条目表：slug → 中文译名）。

mod api;
mod curseforge;
mod modrinth;
mod shared;
mod translation;
pub(crate) mod wiki_entry;

pub use api::{
    get_community_details, get_community_versions, modrinth_version_files, search_community,
    CommunityDependency, CommunityDependencyItem, CommunitySearchPage, CommunitySearchResult,
    CommunityVersion, SearchQuery,
};

// ── 跨模块复用（平台实现与 updates / modpack / loader 依赖这些名字）──

/// JSON 查询通道（updates/identify.rs 复用）。
pub(crate) use shared::{api_routes, api_urls, get_json_multi, ApiPlatform};

/// CurseForge 批量文件换链（modpack.rs 整合包安装复用）。
pub(crate) use curseforge::curseforge_files;
/// CurseForge 带 key GET（loader/optifine.rs 复用）。
pub(crate) use curseforge::curseforge_get;
/// Modrinth 项目原始 JSON 批量拉取（updates/identify.rs 复用）。
pub(crate) use modrinth::modrinth_projects_raw;
/// MCIM 批量翻译通道（updates/identify.rs 复用）。
pub(crate) use translation::fetch_translations;
