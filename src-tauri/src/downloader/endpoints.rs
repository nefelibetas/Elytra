//! 外部端点统一收口：社区资源（Modrinth/CurseForge + MCIM 镜像）与 JDK 下载源的
//! 全部 URL 常量。此前散落在 community/shared.rs、community/translation.rs、
//! util.rs、mod_loader/version_query.rs、jdk/download.rs 五处，改动需逐一同步；
//! 现集中于本模块，**单一事实源**。
//!
//! 说明：这些是程序内建的网络拓扑（官方域名 / 镜像地址），非用户偏好，
//! 故保持编译期常量、**不持久化、不进设置页**；镜像失效/换域属运营事件，
//! 随版本发布更新即可。若未来要做用户可自定义镜像，应整体做成
//! "自定义源编辑器"（对齐 `DownloadSource.sources`），而非逐块开小口。
//!
//! 相关但**不在本模块**（各有归属，勿迁入）：
//! - Mojang 文件源 / BMCLAPI 镜像：`config/downloader/model.rs` 的
//!   `DownloadSource`（official()/china() 模板）与 `downloader/model.rs` 的
//!   `MIRROR_ROUTES`（官方域 → BMCLAPI 前缀，单一表）；
//! - 镜像/限流域名判定：`fetcher/core.rs::is_mirror_url` 与
//!   `fetcher/routes.rs::is_mirror_domain`（结构性规则）。

/// 社区资源端点（Modrinth / CurseForge 官方与 MCIM 镜像）。
pub struct CommunityEndpoints;

impl CommunityEndpoints {
    /// Modrinth 官方 API 基址（`api_routes` / `fabric_api_versions` 共用）。
    pub const MODRINTH_API_BASE: &'static str = "https://api.modrinth.com";
    /// MCIM 镜像 Modrinth API 基址（docs.mcimirror.top，"可直接替换 URL"）。
    pub const MODRINTH_MIRROR_BASE: &'static str = "https://mod.mcimirror.top/modrinth";
    /// CurseForge 官方 API 基址（含 /v1 版本前缀）。
    pub const CURSEFORGE_API_BASE: &'static str = "https://api.curseforge.com/v1";
    /// MCIM 镜像 CurseForge API 基址（/v1 前缀保留）。
    pub const CURSEFORGE_MIRROR_BASE: &'static str = "https://mod.mcimirror.top/curseforge/v1";
    /// Modrinth 官方文件 CDN 基址（`community_download_urls` 换域名镜像的源）。
    pub const MODRINTH_CDN_BASE: &'static str = "https://cdn.modrinth.com";
    /// MCIM 镜像文件 CDN 基址（Modrinth 文件换域名即达）。
    pub const MODRINTH_CDN_MIRROR_BASE: &'static str = "https://mod.mcimirror.top";
    /// MCIM 中文简介翻译 API 基址（`/translate/{source}`；仅 MCIM 提供，无官方对照）。
    pub const TRANSLATION_API_BASE: &'static str = "https://mod.mcimirror.top";
}

/// JDK（managed runtime）下载源端点。
pub struct JdkEndpoints;

impl JdkEndpoints {
    /// Mojang java-runtime all.json 固定资源地址（官方启动器 product 版本）。
    pub const RUNTIME_ALL_JSON: &'static str =
        "https://piston-meta.mojang.com/v1/products/java-runtime/2ec0cc96c44e5a76b9c8b7c39df7210883d12871/all.json";
    /// Adoptium 官方 API 基址（`/v3/assets/latest/...`，元数据含 sha256 checksum）。
    pub const ADOPTIUM_API_BASE: &'static str = "https://api.adoptium.net";
    /// TUNA 清华镜像 Adoptium 文件镜像基址（与官方同文件、文件名一致，仅同步文件——
    /// 无 API、无 checksum，元数据必须走官方 API）。大文件下载优先用它省国内带宽。
    pub const TUNA_ADOPTIUM_BASE: &'static str = "https://mirrors.tuna.tsinghua.edu.cn/Adoptium";
    /// Azul Zulu 官方 API 基址（`/metadata/v1/zulu/packages/...`，含 download_url + sha256）。
    pub const AZUL_API_BASE: &'static str = "https://api.azul.com";
}
