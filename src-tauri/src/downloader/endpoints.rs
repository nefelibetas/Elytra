//! 下载源端点统一收口：全部程序内建网络拓扑（官方域名 / 镜像地址 / URL 模板）的
//! **单一事实源**——社区资源（Modrinth/CurseForge + MCIM 镜像）、JDK 下载源、
//! Mojang/BMCLAPI 文件源模板与镜像路由表。此前散落在 community/shared.rs、
//! translation.rs、util.rs、mod_loader/version_query.rs、jdk/download.rs、
//! config/downloader/model.rs（official()/china() 内联 URL）与 downloader/model.rs
//! （MIRROR_ROUTES）多处，改动需逐一同步；现全部集中于本模块，**一处一个**。
//!
//! 这些是程序内建的网络拓扑（官方域名 / 镜像地址），非用户偏好，
//! 故保持编译期常量、**不持久化、不进设置页**（Settings.toml 仅存模式三态，
//! `sources` 字段已 skip 序列化）；镜像失效/换域属运营事件，随版本发布更新即可。
//! 若未来要做用户可自定义镜像，应整体做成"自定义源编辑器"
//! （对齐 `DownloadSource.sources`），而非逐块开小口。
//!
//! 相关但**不在本模块**（各有归属，勿迁入）：
//! - 镜像/限流域名判定：`fetcher/core.rs::is_mirror_url` 与
//!   `fetcher/routes.rs::is_mirror_domain`（结构性规则，非 URL 拓扑）；
//! - 各调用点拼接的业务路径片段（如 `/mc/game/version_manifest_v2.json`）。

use crate::config::downloader::model::DownloadSource;

// ── Mojang 官方文件源（版本清单 / 资源 / 库 / ModLoader 官方端点）──

/// Mojang 版本清单 base（版本清单 / 版本 json / 资源索引 / java-runtime all.json）。
pub const MOJANG_MANIFEST: &str = "https://piston-meta.mojang.com";
/// Mojang 资源对象 CDN（assets 文件）。
pub const MOJANG_RESOURCES: &str = "https://resources.download.minecraft.net";
/// Mojang 库 maven（libraries.minecraft.net）。
pub const MOJANG_LIBRARIES: &str = "https://libraries.minecraft.net";
/// Forge 官方 maven。
pub const FORGE_MAVEN: &str = "https://maven.minecraftforge.net";
/// NeoForge 官方 maven。
pub const NEOFORGE_MAVEN: &str = "https://maven.neoforged.net";
/// Fabric 官方 maven（loader jar）。
pub const FABRIC_MAVEN: &str = "https://maven.fabricmc.net";
/// Fabric 官方 meta。
pub const FABRIC_META: &str = "https://meta.fabricmc.net";
/// Quilt 官方 meta。
pub const QUILT_META: &str = "https://meta.quiltmc.org";

// ── BMCLAPI 镜像（国内）──

/// BMCLAPI base（manifest 镜像）。
pub const BMCLAPI: &str = "https://bmclapi2.bangbang93.com";
/// assets 对象镜像前缀。
pub const BMCLAPI_ASSETS: &str = "https://bmclapi2.bangbang93.com/assets";
/// maven 镜像前缀（Mojang 库 / Forge / NeoForge / Fabric maven）。
pub const BMCLAPI_MAVEN: &str = "https://bmclapi2.bangbang93.com/maven";
/// libraries 镜像前缀。
pub const BMCLAPI_LIBRARIES: &str = "https://bmclapi2.bangbang93.com/libraries";
/// fabric meta 镜像前缀。
pub const BMCLAPI_FABRIC_META: &str = "https://bmclapi2.bangbang93.com/fabric-meta";

// ── URL 路径/模板片段（base 之上，每段唯一；官方与镜像共用处只定义一次）──

/// forge 版本元数据（两条源同走官方 maven，下载由路由/重写层镜像兜底）。
pub const FORGE_METADATA_PATH: &str = "/net/minecraftforge/forge/maven-metadata.xml";
/// neoforge 版本元数据（官方：maven.neoforged.net/releases 下）。
pub const NEOFORGE_METADATA_PATH: &str = "/releases/net/neoforged/neoforge/maven-metadata.xml";
/// neoforge 版本元数据（镜像：BMCLAPI /maven 下无 /releases 段）。
pub const NEOFORGE_METADATA_MIRROR_PATH: &str = "/net/neoforged/neoforge/maven-metadata.xml";
/// fabric loader 版本列表。
pub const FABRIC_LOADER_LIST_PATH: &str = "/v2/versions/loader/{mc}";
/// quilt loader 版本列表（无镜像，官方直连）。
pub const QUILT_LOADER_LIST_PATH: &str = "/v3/versions/loader/{mc}";
/// OptiFine 版本列表（仅 BMCLAPI 有源，两条源同走）。
pub const OPTIFINE_LIST_PATH: &str = "/optifine/{mc}";
/// forge 安装器（官方路径段，镜像经 BMCLAPI /maven 前缀）。
pub const FORGE_INSTALLER_PATH: &str =
    "/net/minecraftforge/forge/{mc}-{ver}/forge-{mc}-{ver}-installer.jar";
/// neoforge 安装器（官方：/releases 下）。
pub const NEOFORGE_INSTALLER_PATH: &str =
    "/releases/net/neoforged/neoforge/{ver}/neoforge-{ver}-installer.jar";
/// neoforge 安装器（镜像：/maven 下无 /releases 段）。
pub const NEOFORGE_INSTALLER_MIRROR_PATH: &str =
    "/net/neoforged/neoforge/{ver}/neoforge-{ver}-installer.jar";
/// fabric loader jar（maven 路径段，官方与镜像共用）。
pub const FABRIC_LOADER_PATH: &str = "/net/fabricmc/fabric-loader/{ver}/fabric-loader-{ver}.jar";
/// fabric profile json。
pub const FABRIC_PROFILE_PATH: &str = "/v2/versions/loader/{mc}/{loaderVer}/profile/json";
/// quilt profile json（无镜像，官方直连）。
pub const QUILT_PROFILE_PATH: &str = "/v3/versions/loader/{mc}/{loaderVer}/profile/json";
/// OptiFine 下载路径模板。
pub const OPTIFINE_DOWNLOAD_PATH: &str = "/optifine/{mc}/{type}/{patch}";

/// 官方文件源模板（Mojang 系直连；OptiFine 无官方源，走 BMCLAPI）。
pub fn official_source() -> DownloadSource {
    DownloadSource {
        name: "官方".into(),
        manifest_url: MOJANG_MANIFEST.into(),
        resource_url: MOJANG_RESOURCES.into(),
        maven_url: MOJANG_LIBRARIES.into(),
        libraries_url: MOJANG_LIBRARIES.into(),
        forge_list_url: format!("{FORGE_MAVEN}{FORGE_METADATA_PATH}"),
        fabric_list_url: format!("{FABRIC_META}{FABRIC_LOADER_LIST_PATH}"),
        neoforge_list_url: format!("{NEOFORGE_MAVEN}{NEOFORGE_METADATA_PATH}"),
        quilt_list_url: format!("{QUILT_META}{QUILT_LOADER_LIST_PATH}"),
        optifine_list_url: format!("{BMCLAPI}{OPTIFINE_LIST_PATH}"),
        forge_installer_url: format!("{FORGE_MAVEN}{FORGE_INSTALLER_PATH}"),
        neoforge_installer_url: format!("{NEOFORGE_MAVEN}{NEOFORGE_INSTALLER_PATH}"),
        fabric_loader_url: format!("{FABRIC_MAVEN}{FABRIC_LOADER_PATH}"),
        fabric_profile_url: format!("{FABRIC_META}{FABRIC_PROFILE_PATH}"),
        quilt_profile_url: format!("{QUILT_META}{QUILT_PROFILE_PATH}"),
        optifine_download_url: format!("{BMCLAPI}{OPTIFINE_DOWNLOAD_PATH}"),
    }
}

/// 镜像文件源模板（BMCLAPI；无镜像的端点保持官方直连，与 MIRROR_ROUTES 行语义一致）。
pub fn mirror_source() -> DownloadSource {
    DownloadSource {
        name: "国内镜像".into(),
        manifest_url: BMCLAPI.into(),
        resource_url: BMCLAPI_ASSETS.into(),
        maven_url: BMCLAPI_MAVEN.into(),
        libraries_url: BMCLAPI_LIBRARIES.into(),
        forge_list_url: format!("{FORGE_MAVEN}{FORGE_METADATA_PATH}"),
        fabric_list_url: format!("{BMCLAPI_FABRIC_META}{FABRIC_LOADER_LIST_PATH}"),
        neoforge_list_url: format!("{BMCLAPI_MAVEN}{NEOFORGE_METADATA_MIRROR_PATH}"),
        quilt_list_url: format!("{QUILT_META}{QUILT_LOADER_LIST_PATH}"),
        optifine_list_url: format!("{BMCLAPI}{OPTIFINE_LIST_PATH}"),
        forge_installer_url: format!("{BMCLAPI_MAVEN}{FORGE_INSTALLER_PATH}"),
        neoforge_installer_url: format!("{BMCLAPI_MAVEN}{NEOFORGE_INSTALLER_MIRROR_PATH}"),
        fabric_loader_url: format!("{BMCLAPI_MAVEN}{FABRIC_LOADER_PATH}"),
        fabric_profile_url: format!("{BMCLAPI_FABRIC_META}{FABRIC_PROFILE_PATH}"),
        quilt_profile_url: format!("{QUILT_META}{QUILT_PROFILE_PATH}"),
        optifine_download_url: format!("{BMCLAPI}{OPTIFINE_DOWNLOAD_PATH}"),
    }
}

/// BMCLAPI 镜像路由表：官方域 → BMCLAPI 镜像前缀。
///
/// 此前为两张同构表：`fetcher.rs` 的 MIRROR_ROUTES（路由层镜像候选生成，
/// 全量 10 条）与 `source.rs` 的 REWRITE_RULES（`DownloadSource::mirror_url`
/// 重写，前 8 条子集）。现合并为本单一表，两个消费方均引用此处；
/// 行序即匹配优先序（各域互不重叠，first-match 与全量收集语义一致）。
/// 替换前缀引用本模块 BMCLAPI_* 常量，与 mirror_source() 模板同一事实源。
///
/// 相比原 REWRITE_RULES 的差异行逐条注明：
/// - `maven.fabricmc.net`：REWRITE_RULES 原无。Fabric 库构件此前仅经
///   fetcher 路由层生成镜像候选；合并后 `mirror_url` 同步获得该重写
///   （现有调用方均不传 fabric 库 URL，纯超集保全集）。
/// - `meta.fabricmc.net` → `/fabric-meta`：REWRITE_RULES 原无。fabric 版本
///   清单镜像（quilt 无镜像，保持直连官方）；同上，仅路由层原先可用。
pub(crate) const MIRROR_ROUTES: &[(&str, &str)] = &[
    ("resources.download.minecraft.net", BMCLAPI_ASSETS),
    ("libraries.minecraft.net", BMCLAPI_LIBRARIES),
    ("piston-meta.mojang.com", BMCLAPI),
    ("piston-data.mojang.com", BMCLAPI),
    ("launcher.mojang.com", BMCLAPI),
    ("launchermeta.mojang.com", BMCLAPI),
    // ModLoader 官方 maven 对高并发不友好（实测 20 并发即有连接失败），
    // BMCLAPI 完整镜像 maven.neoforged.net / maven.minecraftforge.net（302 实测可用）。
    // 仅做备用：官方源仍优先，镜像源兜底，避免整批库下载失败。
    ("maven.neoforged.net", BMCLAPI_MAVEN),
    ("maven.minecraftforge.net", BMCLAPI_MAVEN),
    ("maven.fabricmc.net", BMCLAPI_MAVEN),
    // fabric 版本清单镜像（quilt 无镜像，保持直连官方）
    ("meta.fabricmc.net", BMCLAPI_FABRIC_META),
];

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
