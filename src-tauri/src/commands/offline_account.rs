use crate::auth;

/// 读取最后一次使用的离线用户名（历史列表末尾；兼容旧版单值键）。
#[tauri::command]
pub fn get_last_offline_username() -> String {
    auth::offline::last_username()
}

#[tauri::command]
pub fn save_last_offline_username(username: String) -> Result<(), String> {
    save_offline_account(username)
}

/// 读取离线账号历史（最近使用升序，末尾为当前）。
#[tauri::command]
pub fn get_offline_accounts() -> Vec<String> {
    auth::offline::accounts()
}

/// 保存/更新离线用户名到历史列表（输入即存；已存在则移到末尾，空名忽略）。
#[tauri::command]
pub fn save_offline_account(username: String) -> Result<(), String> {
    auth::offline::save(&username)
        .map_err(common_err_ctx_key!("errors.config.write", "保存配置失败"))
}

#[tauri::command]
pub fn remove_offline_account(username: String) -> Result<(), String> {
    auth::offline::remove(&username)
        .map_err(common_err_ctx_key!("errors.config.write", "保存配置失败"))
}
