use crate::constants;
use serde::{Deserialize, Serialize};

/// Microsoft 正版账号（含长期 refresh_token 与短期 Minecraft access_token）。
/// 敏感数据安全：经 persist::secrets 存到 `Elytra/secrets.bin`（Windows DPAPI 加密，
/// 绑定当前用户账户），**不再进 Settings.toml 明文**（旧版本迁移见 migrate_legacy）。
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
    migrate_legacy();
    let bytes = crate::persist::secrets::read_secret(constants::SECT_MS_ACCOUNT)?;
    serde_json::from_slice(&bytes).ok()
}

pub fn save(account: &MsAccount) -> Result<(), String> {
    // 写入 secrets 的同时清掉旧 TOML section（迁移完成后不再有明文残留）
    let bytes = serde_json::to_vec(account).map_err(|e| e.to_string())?;
    crate::persist::secrets::save_secret(constants::SECT_MS_ACCOUNT, &bytes)?;
    let _ = crate::persist::remove(constants::SECT_MS_ACCOUNT);
    Ok(())
}

pub fn clear() -> Result<(), String> {
    crate::persist::secrets::remove_secret(constants::SECT_MS_ACCOUNT)?;
    let _ = crate::persist::remove(constants::SECT_MS_ACCOUNT);
    Ok(())
}

/// 旧版本迁移（幂等）：secrets 已有则不动作；Settings.toml 存在 ms_account 明文时
/// 读取 → 写入加密存储 → 删除 TOML section（老用户不丢账号，明文不再残留）。
fn migrate_legacy() {
    if crate::persist::secrets::read_secret(constants::SECT_MS_ACCOUNT).is_some() {
        return;
    }
    let Ok(old) = crate::persist::read::<MsAccount>(constants::SECT_MS_ACCOUNT) else {
        return;
    };
    let Ok(bytes) = serde_json::to_vec(&old) else {
        return;
    };
    if crate::persist::secrets::save_secret(constants::SECT_MS_ACCOUNT, &bytes).is_ok() {
        let _ = crate::persist::remove(constants::SECT_MS_ACCOUNT);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 旧版本迁移：Settings.toml 明文 ms_account → secrets（DPAPI）→ TOML section 清除。
    /// 测试进程配置落在 target/debug/deps/Elytra/，与正式运行隔离（AGENTS.md 约定）。
    #[test]
    fn migrate_legacy_moves_toml_to_secrets() {
        let sec = constants::SECT_MS_ACCOUNT;
        // 清理现场
        crate::persist::secrets::remove_secret(sec).ok();
        let _ = crate::persist::remove(sec);

        // 模拟旧版本：TOML 明文 ms_account
        let old = MsAccount {
            uuid: "test-uuid".into(),
            name: "tester".into(),
            access_token: "legacy-at".into(),
            refresh_token: "legacy-rt".into(),
            xuid: "12345".into(),
            expires_at: 999_999,
        };
        crate::persist::save(sec, &old).expect("写旧格式应成功");
        assert!(
            crate::persist::read::<MsAccount>(sec).is_ok(),
            "前置：TOML 明文应存在"
        );

        // read() 触发迁移
        let migrated = read().expect("迁移后应可读到账号");
        assert_eq!(migrated.uuid, "test-uuid");
        assert_eq!(migrated.refresh_token, "legacy-rt");

        // TOML section 已清除（明文不再残留）
        assert!(
            crate::persist::read::<MsAccount>(sec).is_err(),
            "迁移后 Settings.toml 不应残留 ms_account 明文"
        );
        // secrets 中有加密副本
        assert!(
            crate::persist::secrets::read_secret(sec).is_some(),
            "secrets 应持有加密副本"
        );

        // 清理现场
        crate::persist::secrets::remove_secret(sec).ok();
    }
}
