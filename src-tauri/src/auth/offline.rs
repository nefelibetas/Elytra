//! 离线账号历史域：本地用户名列表（最近使用升序，末尾为当前）。

use crate::constants;
use crate::persist;
use crate::persist::config::PersistError;

/// 离线账号历史（最近使用升序，末尾为当前）。未保存过 → 空列表。
pub fn accounts() -> Vec<String> {
    persist::read(constants::SECT_OFFLINE_ACCOUNTS).unwrap_or_default()
}

/// 当前（最后使用）离线用户名；列表为空时回退旧版单值键。
pub fn last_username() -> String {
    accounts().last().cloned().unwrap_or_else(|| {
        persist::read::<String>(constants::LAST_OFFLINE_USERNAME).unwrap_or_default()
    })
}

/// 保存/更新离线用户名到历史（输入即存；已存在移到末尾；空名忽略），并清理旧版单值键。
pub fn save(username: &str) -> Result<(), PersistError> {
    let name = username.trim();
    if name.is_empty() {
        return Ok(());
    }
    let mut list = accounts();
    upsert(&mut list, name);
    persist::save(constants::SECT_OFFLINE_ACCOUNTS, &list)?;
    let _ = persist::remove(constants::LAST_OFFLINE_USERNAME);
    Ok(())
}

/// 移除指定离线用户名（不存在则无操作）。
pub fn remove(username: &str) -> Result<(), PersistError> {
    let mut list = accounts();
    list.retain(|n| n != username);
    persist::save(constants::SECT_OFFLINE_ACCOUNTS, &list)
}

/// 纯列表插入：同名先去重再追加，维持“最近使用升序、末尾为当前”。
fn upsert(list: &mut Vec<String>, name: &str) {
    list.retain(|n| n != name);
    list.push(name.to_string());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upsert_moves_existing_to_tail() {
        let mut list: Vec<String> = vec!["a".into(), "b".into(), "c".into()];
        upsert(&mut list, "b");
        assert_eq!(list, ["a", "c", "b"]);

        upsert(&mut list, "d");
        assert_eq!(list, ["a", "c", "b", "d"]);
    }

    #[test]
    fn upsert_on_empty_list_appends() {
        let mut list: Vec<String> = Vec::new();
        upsert(&mut list, "solo");
        assert_eq!(list, ["solo"]);
    }
}
