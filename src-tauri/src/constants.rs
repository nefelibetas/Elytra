/// 离线用户名键
pub const LAST_OFFLINE_USERNAME: &str = "last_offline_username";
/// 启动器公开名称
pub const APP_NAME: &str = "Elytra Launcher";
/// 公开版本（单一事实源 `Cargo.toml`；发布时 tauri.conf.json / package.json 需与 Cargo.toml 一致）
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
/// Minecraft 启动器品牌名（-Dminecraft.launcher.brand）
pub const LAUNCHER_BRAND: &str = "Elytra";
/// Minecraft 客户端 ID（--clientId）
pub const CLIENT_ID: &str = "elytra";

/// 下载 HTTP 客户端 User-Agent
pub const UA_DOWNLOAD: &str = concat!("Elytra Launcher/", env!("CARGO_PKG_VERSION"));
/// Mojang API 调用 User-Agent
pub const UA_MOJANG_API: &str = concat!(
    "Elytra Launcher/",
    env!("CARGO_PKG_VERSION"),
    " (+https://github.com/nefelibetas/Elytra)"
);

/// 镜像源（BMCLAPI 等）并发上限：16。实测（2026-10）跨文件 32 并发全部 302 无 429、
/// 总耗时随并发线性下降；429 风险集中在**同文件**重复 Range 并发（节点侧按 hash 签名限流），
/// 跨文件高并发安全。单文件分段 ≤4 段，16 ≥ 4×4 文件在飞同文件不会触及节点限流。
/// 前端 Settings.vue 的 MIRROR_CONCURRENCY 需与此同步。
pub const MIRROR_CONCURRENCY: usize = 16;

/// CurseForge API Key（构建期从项目根 `.env` 注入，不提交 git）。
/// 所有 api.curseforge.com 与 forgecdn.net 请求（含文件下载）必须携带 x-api-key；
/// 运行时优先使用用户在设置中填写的 key，两者都没有时 CurseForge 源不可用。
pub const CURSEFORGE_API_KEY: Option<&str> = option_env!("ELYTRA_CURSEFORGE_API_KEY");

/// 微软 OAuth 公共客户端 ID（公开信息，不是密钥；回调固定 `http://localhost:27205`）。
/// 如需使用自己的 Azure 应用，改这里并同步注册回调地址。
pub const MS_CLIENT_ID: &str = "eef6b0c0-8cbb-4091-84d0-b59fa0600dba";

/// 配置目录名（exe 旁）
pub const CONFIG_DIR: &str = "Elytra";
/// 配置文件名
pub const SETTINGS_FILE: &str = "Settings.toml";

/// 系统钥匙串条目标识（macOS Keychain / Linux Secret Service 共用）
pub const KEYCHAIN_SERVICE: &str = "ElytraLauncher";
/// 主密钥条目 account（macOS）/ attribute（Linux）
pub const KEYCHAIN_MASTER_KEY_ACCOUNT: &str = "master-key";
/// 主密钥条目 label（用户可见）
pub const KEYCHAIN_MASTER_KEY_LABEL: &str = "Elytra Launcher master key";

/// 下载器配置
pub const SECT_DOWNLOADER: &str = "downloader";
/// 启动器配置
pub const SECT_LAUNCHER: &str = "launcher";
/// JDK 配置
pub const SECT_JDK: &str = "jdk";
/// JDK 扫描缓存 section（本机发现的 JDK 列表，后端维护）
pub const SECT_JDK_SCANNED: &str = "jdk_scanned";
/// 微软账号列表（secrets.bin 加密存储，按登录时间排序，末尾为当前）
pub const SECT_MS_ACCOUNTS: &str = "ms_accounts";
pub const SECT_EXTERNAL_ACCOUNTS: &str = "external_accounts";
pub const SECT_EXTERNAL: &str = "external";
/// 离线账号历史列表（最近使用排序，末尾为当前）
pub const SECT_OFFLINE_ACCOUNTS: &str = "offline_accounts";
/// 游戏目录
pub const SECT_GAME_DIRS: &str = "game_dirs";
/// 联机配置（协调节点等）
pub const SECT_ONLINE: &str = "online";
/// 本地皮肤收藏索引（PNG 原文存配置目录 `skins/<id>.png`）
pub const SECT_LOCAL_SKINS: &str = "local_skins";
/// 工具箱多线程下载器（保存目录 / 并发文件数 / 分段数）
pub const SECT_TOOL_DL: &str = "tooldl";

#[cfg(test)]
mod tests {
    /// CurseForge key 经 build.rs 从 .env 注入；None（CI/未克隆 .env）跳过，有则校验格式。
    #[test]
    fn curseforge_key_injected_via_env() {
        if let Some(key) = super::CURSEFORGE_API_KEY {
            assert!(
                key.starts_with("$2a$10$"),
                "CurseForge key 应为 bcrypt 格式（$2a$10$ 前缀），当前值异常: {key:?}"
            );
        }
    }
}
