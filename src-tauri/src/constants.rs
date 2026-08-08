/// 启动器公开名称
pub const APP_NAME: &str = "Elytra Launcher";
/// 公开版本（与 tauri.conf.json 保持一致）
pub const APP_VERSION: &str = "1.0.0";
/// Minecraft 启动器品牌名（-Dminecraft.launcher.brand）
pub const LAUNCHER_BRAND: &str = "Elytra";
/// Minecraft 客户端 ID（--clientId）
pub const CLIENT_ID: &str = "elytra";

/// 下载 HTTP 客户端 User-Agent
pub const UA_DOWNLOAD: &str = "Elytra Launcher/1.0.0";
/// Mojang API 调用 User-Agent
pub const UA_MOJANG_API: &str = "Elytra Launcher/1.0.0 (+https://github.com/nefelibetas/Elytra)";

/// 镜像源（BMCLAPI 等）并发上限。
/// 官方源实测 128 并发稳定无限制；BMCLAPI 30 并发同文件即触发 429
/// （`retry-after-short: 60` 窗口），8 为实测安全值，与主流启动器对镜像的保守并发一致。
/// 前端 Settings.vue 的 MIRROR_CONCURRENCY 需与此同步。
pub const MIRROR_CONCURRENCY: usize = 8;

/// 配置目录名（exe 旁）
pub const CONFIG_DIR: &str = "Elytra";
/// 配置文件名
pub const SETTINGS_FILE: &str = "Settings.toml";

/// persist section: 下载器配置
pub const SECT_DOWNLOADER: &str = "downloader";
/// persist section: 启动器配置
pub const SECT_LAUNCHER: &str = "launcher";
/// persist section: JDK 配置
pub const SECT_JDK: &str = "jdk";
/// persist section: 离线皮肤配置
pub const SECT_OFFLINE_SKIN: &str = "offline_skin";
/// persist section: 微软账号
pub const SECT_MS_ACCOUNT: &str = "ms_account";
/// persist section: 游戏目录
pub const SECT_GAME_DIRS: &str = "game_dirs";
