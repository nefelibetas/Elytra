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

/// 读取全部正版账号（登录时间升序，末尾 = 当前账号）。无账号返回空列表。
pub fn read_all() -> Vec<MsAccount> {
    migrate_legacy();
    match crate::persist::secrets::read_secret(constants::SECT_MS_ACCOUNTS) {
        Some(bytes) => serde_json::from_slice(&bytes).unwrap_or_default(),
        None => vec![],
    }
}

/// 读取当前正版账号（列表最后一个，即最近登录/切换的）。无则 None。
pub fn read() -> Option<MsAccount> {
    read_all().into_iter().last()
}

/// 保存正版账号：按 uuid upsert 并置为当前（移到列表末尾）。
/// 写入 secrets 的同时清掉旧 TOML section（迁移完成后不再有明文残留）。
pub fn save(account: &MsAccount) -> Result<(), String> {
    let mut list = read_all();
    list.retain(|a| a.uuid != account.uuid);
    list.push(account.clone());
    let bytes = serde_json::to_vec(&list).map_err(|e| e.to_string())?;
    crate::persist::secrets::save_secret(constants::SECT_MS_ACCOUNTS, &bytes)?;
    let _ = crate::persist::remove(constants::SECT_MS_ACCOUNT);
    Ok(())
}

/// 移除指定 uuid 的正版账号（连同其长期 refresh_token）。列表清空时删除存储键。
pub fn remove(uuid: &str) -> Result<(), String> {
    let mut list = read_all();
    list.retain(|a| a.uuid != uuid);
    if list.is_empty() {
        return crate::persist::secrets::remove_secret(constants::SECT_MS_ACCOUNTS);
    }
    let bytes = serde_json::to_vec(&list).map_err(|e| e.to_string())?;
    crate::persist::secrets::save_secret(constants::SECT_MS_ACCOUNTS, &bytes)
}

/// 旧版本迁移（幂等，两级）：
/// ① Settings.toml 明文 ms_account → secrets 单键（老用户不丢账号，明文不再残留）；
/// ② secrets 单键 ms_account → ms_accounts 列表（本轮多账号改造）。
fn migrate_legacy() {
    // ① TOML 明文 → secrets 单键
    if crate::persist::secrets::read_secret(constants::SECT_MS_ACCOUNT).is_none() {
        if let Ok(old) = crate::persist::read::<MsAccount>(constants::SECT_MS_ACCOUNT) {
            if let Ok(bytes) = serde_json::to_vec(&old) {
                if crate::persist::secrets::save_secret(constants::SECT_MS_ACCOUNT, &bytes).is_ok()
                {
                    let _ = crate::persist::remove(constants::SECT_MS_ACCOUNT);
                }
            }
        }
    }
    // ② secrets 单键 → 列表
    if crate::persist::secrets::read_secret(constants::SECT_MS_ACCOUNTS).is_none() {
        if let Some(bytes) = crate::persist::secrets::read_secret(constants::SECT_MS_ACCOUNT) {
            if let Ok(account) = serde_json::from_slice::<MsAccount>(&bytes) {
                let list = vec![account];
                if let Ok(list_bytes) = serde_json::to_vec(&list) {
                    if crate::persist::secrets::save_secret(constants::SECT_MS_ACCOUNTS, &list_bytes)
                        .is_ok()
                    {
                        let _ = crate::persist::secrets::remove_secret(constants::SECT_MS_ACCOUNT);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(uuid: &str, name: &str) -> MsAccount {
        MsAccount {
            uuid: uuid.into(),
            name: name.into(),
            access_token: "at".into(),
            refresh_token: "rt".into(),
            xuid: "12345".into(),
            expires_at: 999_999,
        }
    }

    fn cleanup() {
        crate::persist::secrets::remove_secret(constants::SECT_MS_ACCOUNTS).ok();
        crate::persist::secrets::remove_secret(constants::SECT_MS_ACCOUNT).ok();
        let _ = crate::persist::remove(constants::SECT_MS_ACCOUNT);
    }

    /// 多账号：save 追加新账号，upsert 不重复，末尾 = 当前。
    #[test]
    fn save_appends_and_upserts_by_uuid() {
        cleanup();

        save(&sample("uuid-a", "Alice")).unwrap();
        save(&sample("uuid-b", "Bob")).unwrap();
        assert_eq!(read_all().len(), 2);
        assert_eq!(read().unwrap().uuid, "uuid-b", "最近保存的应为当前");

        // upsert：更新已有 uuid 并置为当前，不新增条目
        save(&sample("uuid-a", "Alice2")).unwrap();
        let all = read_all();
        assert_eq!(all.len(), 2, "同 uuid 保存应更新而非追加");
        assert_eq!(all.last().unwrap().uuid, "uuid-a", "upsert 后应为当前");
        assert_eq!(all.last().unwrap().name, "Alice2");
        assert_eq!(all[0].name, "Bob");

        cleanup();
    }

    /// remove：按 uuid 删除，删除当前后回退到前一账号；清空后 read 为 None。
    #[test]
    fn remove_drops_account_and_falls_back() {
        cleanup();
        save(&sample("uuid-a", "Alice")).unwrap();
        save(&sample("uuid-b", "Bob")).unwrap();

        remove("uuid-b").unwrap();
        assert_eq!(read().unwrap().uuid, "uuid-a", "删除当前后回退到上一账号");

        remove("uuid-a").unwrap();
        assert!(read().is_none(), "列表清空后无当前账号");
        assert!(
            crate::persist::secrets::read_secret(constants::SECT_MS_ACCOUNTS).is_none(),
            "列表清空后存储键应删除"
        );

        cleanup();
    }

    /// 旧版本迁移：Settings.toml 明文 ms_account → secrets 列表 → 明文与单键清除。
    /// 测试进程配置落在 target/debug/deps/Elytra/，与正式运行隔离（AGENTS.md 约定）。
    #[test]
    fn migrate_legacy_moves_toml_to_accounts_list() {
        let sec = constants::SECT_MS_ACCOUNT;
        cleanup();

        // 模拟旧版本：TOML 明文 ms_account
        crate::persist::save(sec, &sample("test-uuid", "tester")).expect("写旧格式应成功");
        assert!(
            crate::persist::read::<MsAccount>(sec).is_ok(),
            "前置：TOML 明文应存在"
        );

        // read_all() 触发两级迁移
        let all = read_all();
        assert_eq!(all.len(), 1, "迁移后列表应含原账号");
        assert_eq!(all[0].uuid, "test-uuid");
        assert_eq!(read().unwrap().refresh_token, "rt");

        // TOML 明文已清除；secrets 单键已清除；列表键持有加密副本
        assert!(
            crate::persist::read::<MsAccount>(sec).is_err(),
            "迁移后 Settings.toml 不应残留 ms_account 明文"
        );
        assert!(
            crate::persist::secrets::read_secret(sec).is_none(),
            "迁移后 secrets 单键应已并入列表"
        );
        assert!(
            crate::persist::secrets::read_secret(constants::SECT_MS_ACCOUNTS).is_some(),
            "secrets 应持有加密账号列表"
        );

        cleanup();
    }

    /// 幂等：重复调用迁移不破坏已有列表（同 uuid 不重复）。
    #[test]
    fn migrate_is_idempotent() {
        cleanup();
        save(&sample("uuid-a", "Alice")).unwrap();

        read_all(); // 触发迁移（应无动作）
        read_all();
        assert_eq!(read_all().len(), 1, "幂等迁移不应重复账号");

        cleanup();
    }
}
