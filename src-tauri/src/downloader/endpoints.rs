//! 内建网络拓扑（官方域名/镜像地址/URL 模板）单一事实源，编译期常量不持久化；镜像失效随版本更新；镜像/限流域名判定见 `fetcher/core.rs::is_mirror_url`、`fetcher/routes.rs::is_mirror_domain`。

use crate::config::downloader::DownloadSource;

// ── Mojang 官方文件源（版本清单 / 资源 / 库 / ModLoader 官方端点）──

pub const MOJANG_MANIFEST: &str = "https://piston-meta.mojang.com";
pub const MOJANG_RESOURCES: &str = "https://resources.download.minecraft.net";
pub const MOJANG_LIBRARIES: &str = "https://libraries.minecraft.net";
pub const FORGE_MAVEN: &str = "https://maven.minecraftforge.net";
pub const NEOFORGE_MAVEN: &str = "https://maven.neoforged.net";
pub const FABRIC_MAVEN: &str = "https://maven.fabricmc.net";
pub const FABRIC_META: &str = "https://meta.fabricmc.net";
pub const QUILT_META: &str = "https://meta.quiltmc.org";

// ── BMCLAPI 镜像（国内）──

pub const BMCLAPI: &str = "https://bmclapi2.bangbang93.com";
pub const BMCLAPI_ASSETS: &str = "https://bmclapi2.bangbang93.com/assets";
pub const BMCLAPI_MAVEN: &str = "https://bmclapi2.bangbang93.com/maven";
pub const BMCLAPI_LIBRARIES: &str = "https://bmclapi2.bangbang93.com/libraries";
pub const BMCLAPI_FABRIC_META: &str = "https://bmclapi2.bangbang93.com/fabric-meta";

// ── URL 路径/模板片段（base 之上，每段唯一；官方与镜像共用处只定义一次）──

/// forge 版本元数据（两源同走官方 maven）。
pub const FORGE_METADATA_PATH: &str = "/net/minecraftforge/forge/maven-metadata.xml";
/// neoforge 版本元数据（官方含 /releases 段，镜像无）。
pub const NEOFORGE_METADATA_PATH: &str = "/releases/net/neoforged/neoforge/maven-metadata.xml";
pub const NEOFORGE_METADATA_MIRROR_PATH: &str = "/net/neoforged/neoforge/maven-metadata.xml";
pub const FABRIC_LOADER_LIST_PATH: &str = "/v2/versions/loader/{mc}";
/// quilt loader 版本列表（无镜像）。
pub const QUILT_LOADER_LIST_PATH: &str = "/v3/versions/loader/{mc}";
/// OptiFine 版本列表（仅 BMCLAPI 有源）。
pub const OPTIFINE_LIST_PATH: &str = "/optifine/{mc}";
/// forge 安装器（镜像经 /maven 前缀）。
pub const FORGE_INSTALLER_PATH: &str =
    "/net/minecraftforge/forge/{mc}-{ver}/forge-{mc}-{ver}-installer.jar";
/// neoforge 安装器（官方含 `/releases` 段；镜像无）。
/// 镜像 canonical 路径为 `/maven/net/neoforged/...`（即 `/neoforge/list/:mcversion` 的 `installerPath`，依据 BMCLAPI apiDoc）；镜像是「已同步才有」。
pub const NEOFORGE_INSTALLER_PATH: &str =
    "/releases/net/neoforged/neoforge/{ver}/neoforge-{ver}-installer.jar";
pub const NEOFORGE_INSTALLER_MIRROR_PATH: &str =
    "/net/neoforged/neoforge/{ver}/neoforge-{ver}-installer.jar";
/// fabric loader jar（maven 段，两源共用）。
pub const FABRIC_LOADER_PATH: &str = "/net/fabricmc/fabric-loader/{ver}/fabric-loader-{ver}.jar";
pub const FABRIC_PROFILE_PATH: &str = "/v2/versions/loader/{mc}/{loaderVer}/profile/json";
/// quilt profile json（无镜像）。
pub const QUILT_PROFILE_PATH: &str = "/v3/versions/loader/{mc}/{loaderVer}/profile/json";
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

/// 镜像文件源模板（BMCLAPI；无镜像端点保持官方直连，与 MIRROR_ROUTES 一致）。
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

/// BMCLAPI 镜像路由表：官方域 → 镜像前缀；行序即匹配优先序，供路由层候选生成与 mirror_url 重写共用。
pub(crate) const MIRROR_ROUTES: &[(&str, &str)] = &[
    ("resources.download.minecraft.net", BMCLAPI_ASSETS),
    ("libraries.minecraft.net", BMCLAPI_LIBRARIES),
    ("piston-meta.mojang.com", BMCLAPI),
    ("piston-data.mojang.com", BMCLAPI),
    ("launcher.mojang.com", BMCLAPI),
    ("launchermeta.mojang.com", BMCLAPI),
    // ModLoader 官方 maven 高并发不稳（20 并发即失败），BMCLAPI 仅作兜底（官方优先）。
    // NeoForge 构件 URL 含 `/releases`，而 BMCLAPI `/maven` 已指向仓库根，保留会 404：更具体的 `/releases` 条目须排在通用条目之前。
    ("maven.neoforged.net/releases", BMCLAPI_MAVEN),
    ("maven.neoforged.net", BMCLAPI_MAVEN),
    ("maven.minecraftforge.net", BMCLAPI_MAVEN),
    ("maven.fabricmc.net", BMCLAPI_MAVEN),
    // fabric 版本清单镜像（quilt 无镜像，直连官方）
    ("meta.fabricmc.net", BMCLAPI_FABRIC_META),
];

/// 社区资源端点（Modrinth / CurseForge 官方与 MCIM 镜像）。
pub struct CommunityEndpoints;

impl CommunityEndpoints {
    pub const MODRINTH_API_BASE: &'static str = "https://api.modrinth.com";
    pub const MODRINTH_MIRROR_BASE: &'static str = "https://mod.mcimirror.top/modrinth";
    /// CurseForge 官方 API（含 /v1 前缀）。
    pub const CURSEFORGE_API_BASE: &'static str = "https://api.curseforge.com/v1";
    pub const CURSEFORGE_MIRROR_BASE: &'static str = "https://mod.mcimirror.top/curseforge/v1";
    pub const MODRINTH_CDN_BASE: &'static str = "https://cdn.modrinth.com";
    pub const MODRINTH_CDN_MIRROR_BASE: &'static str = "https://mod.mcimirror.top";
    /// MCIM 中文简介翻译 API（仅 MCIM 提供）。
    pub const TRANSLATION_API_BASE: &'static str = "https://mod.mcimirror.top";
}

/// JDK（managed runtime）下载源端点。
pub struct JdkEndpoints;

impl JdkEndpoints {
    pub const RUNTIME_ALL_JSON: &'static str =
        "https://piston-meta.mojang.com/v1/products/java-runtime/2ec0cc96c44e5a76b9c8b7c39df7210883d12871/all.json";
    /// Adoptium 官方 API（元数据含 sha256）。
    pub const ADOPTIUM_API_BASE: &'static str = "https://api.adoptium.net";
    /// TUNA 清华 Adoptium 文件镜像（无 API/checksum，元数据走官方）；大文件优先。
    pub const TUNA_ADOPTIUM_BASE: &'static str = "https://mirrors.tuna.tsinghua.edu.cn/Adoptium";
    /// Azul Zulu 官方 API（含 download_url + sha256）。
    pub const AZUL_API_BASE: &'static str = "https://api.azul.com";
}
