//! 离线账号历史 CRUD 命令。

use crate::constants;
use crate::persist;

/// 读取最后一次使用的离线用户名（历史列表末尾；兼容旧版单值键）。
#[tauri::command]
pub fn get_last_offline_username() -> String {
    let list: Vec<String> = persist::read(constants::SECT_OFFLINE_ACCOUNTS).unwrap_or_default();
    list.last().cloned().unwrap_or_else(|| {
        persist::read::<String>(constants::LAST_OFFLINE_USERNAME).unwrap_or_default()
    })
}

/// 保存最后一次使用的离线用户名（历史列表 upsert，置为末尾；空名忽略）。
#[tauri::command]
pub fn save_last_offline_username(username: String) -> Result<(), String> {
    save_offline_account(username)
}

/// 读取离线账号历史（最近使用升序，末尾为当前）。
#[tauri::command]
pub fn get_offline_accounts() -> Vec<String> {
    persist::read(constants::SECT_OFFLINE_ACCOUNTS).unwrap_or_default()
}

/// 保存/更新离线用户名到历史列表（输入即存；已存在则移到末尾，空名忽略）。
#[tauri::command]
pub fn save_offline_account(username: String) -> Result<(), String> {
    let name = username.trim().to_string();
    if name.is_empty() {
        return Ok(());
    }
    let mut list: Vec<String> = persist::read(constants::SECT_OFFLINE_ACCOUNTS).unwrap_or_default();
    list.retain(|n| n != &name);
    list.push(name);
    persist::save(constants::SECT_OFFLINE_ACCOUNTS, &list)
        .map_err(|e| format!("保存配置失败\n{e}"))?;
    let _ = persist::remove(constants::LAST_OFFLINE_USERNAME); // 旧单值键已并入列表
    Ok(())
}

/// 移除指定离线用户名。
#[tauri::command]
pub fn remove_offline_account(username: String) -> Result<(), String> {
    let mut list: Vec<String> = persist::read(constants::SECT_OFFLINE_ACCOUNTS).unwrap_or_default();
    list.retain(|n| n != &username);
    persist::save(constants::SECT_OFFLINE_ACCOUNTS, &list).map_err(|e| format!("保存配置失败\n{e}"))
}
