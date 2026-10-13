use crate::constants;
use crate::persist;
use serde::{Deserialize, Serialize};

/// Microsoft 正版账号（含长期 refresh_token 与短期 Minecraft access_token）。
/// 敏感数据安全：经 persist::secrets 存到 `Elytra/secrets.bin`（Windows DPAPI 加密，
/// 绑定当前用户账户），**不进 Settings.toml 明文**。
#[derive(Debug, Clone, Serialize, Deserialize, ts_rs::TS)]
#[ts(export_to = "auth/")]
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
    #[ts(type = "number")]
    pub expires_at: u64,
}

/// 读取全部正版账号（登录时间升序，末尾 = 当前账号）。无账号返回空列表。
pub fn read_all() -> Vec<MsAccount> {
    persist::secrets::read_json_list(constants::SECT_MS_ACCOUNTS)
}

/// 读取当前正版账号（列表最后一个，即最近登录/切换的）。无则 None。
pub fn read() -> Option<MsAccount> {
    read_all().into_iter().last()
}

/// 保存正版账号：按 uuid upsert 并置为当前（移到列表末尾）。
/// 走原子读-改-写：并发（静默刷新的 save 与登录/删除重叠）下用陈旧列表整体写回会丢更新。
pub fn save(account: &MsAccount) -> Result<(), String> {
    let account = account.clone();
    persist::secrets::update_json_list(constants::SECT_MS_ACCOUNTS, move |list: &mut Vec<MsAccount>| {
        list.retain(|a| a.uuid != account.uuid);
        list.push(account);
        Ok(())
    })
}

/// 移除指定 uuid 的正版账号（连同其长期 refresh_token）。列表清空时删除存储键。
pub fn remove(uuid: &str) -> Result<(), String> {
    let uuid = uuid.to_string();
    persist::secrets::update_json_list(constants::SECT_MS_ACCOUNTS, move |list: &mut Vec<MsAccount>| {
        list.retain(|a| a.uuid != uuid);
        Ok(())
    })
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
        persist::secrets::update_json_list::<MsAccount, _, ()>(constants::SECT_MS_ACCOUNTS, |list| {
            list.clear();
            Ok(())
        })
        .ok();
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
}
