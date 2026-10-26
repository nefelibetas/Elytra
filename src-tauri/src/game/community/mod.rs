//! 社区资源搜索 / 版本查询（Modrinth + CurseForge 官方 API，类别由 category 区分）。
//! 两平台 ID 体系/鉴权/返回结构不同，分开实现、统一出参：CurseForge 需 x-api-key
//! （用户 key 优先，其次 constants.rs 经 .env 注入），Modrinth 无需 key。简介翻译来自 MCIM 镜像，失败降级英文。
//! 模块布局：[`api`]/[`shared`]/[`modrinth`]/[`curseforge`]/[`translation`]/[`wiki_entry`]。

mod api;
mod curseforge;
mod dependencies;
mod modrinth;
mod shared;
mod translation;
pub(crate) mod wiki_entry;

pub use api::{
    get_community_details, get_community_versions, modrinth_version_files, search_community,
    CommunityDependency, CommunityDependencyItem, CommunitySearchPage, CommunitySearchResult,
    CommunityVersion, SearchQuery,
};
pub(crate) use dependencies::resolve_mod_dependencies;
pub use dependencies::{ModDependencyItem, ModDependencyPlan};

// ── 跨模块复用（平台实现与 updates / modpack / loader 依赖这些名字）──

/// JSON 查询通道（updates/identify.rs 复用）。
pub(crate) use shared::{api_routes, api_urls, get_json_multi, ApiPlatform};

/// CurseForge 带 key GET（loader/optifine.rs 复用）。
pub(crate) use curseforge::curseforge_get;
#[cfg(test)]
pub(crate) use curseforge::parse_fingerprint_matches;
/// CurseForge 用户 key 状态与设置（commands/community.rs 调用）。
pub use curseforge::{
    clear_curseforge_key, curseforge_key_source, set_curseforge_key, CurseForgeKeySource,
};
/// CurseForge 批量文件换链（modpack.rs 整合包安装复用）。
pub(crate) use curseforge::{curseforge_files, curseforge_fingerprint_matches};
/// Modrinth 项目原始 JSON 批量拉取（updates/identify.rs 复用）。
pub(crate) use modrinth::modrinth_projects_raw;
/// MCIM 批量翻译通道（updates/identify.rs 复用）。
pub(crate) use translation::fetch_translations;
