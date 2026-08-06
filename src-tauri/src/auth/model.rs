use crate::constants;
use serde::{Deserialize, Serialize};

/// Microsoft 正版账号（含长期 refresh_token 与短期 Minecraft access_token）。
/// 通过 persist 存到 Settings.toml 的 `ms_account` section。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MsAccount {
    /// Minecraft profile uuid（无连字符）
    pub uuid: String,
    /// 游戏名
    pub name: String,
    /// Minecraft access_token（短时有效，启动游戏用）
    pub access_token: String,
    /// 微软 refresh_token（长期凭证，用于静默刷新）
    pub refresh_token: String,
    /// Xbox User ID（启动参数 --xuid）
    pub xuid: String,
    /// access_token 过期时间（unix 秒）
    pub expires_at: u64,
}


pub fn read() -> Option<MsAccount> {
    crate::persist::read(constants::SECT_MS_ACCOUNT).ok()
}

pub fn save(account: &MsAccount) -> Result<(), String> {
    crate::persist::save(constants::SECT_MS_ACCOUNT, account).map_err(|e| e.to_string())
}

pub fn clear() -> Result<(), String> {
    crate::persist::remove(constants::SECT_MS_ACCOUNT).map_err(|e| e.to_string())
}
