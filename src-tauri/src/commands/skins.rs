//! 本地皮肤收藏命令（披风侧沿用 account.rs 的 fetch_capes / set_active_cape / disable_cape）。

use crate::auth::skins::{self, LocalSkinView};

/// 本地皮肤列表（添加时间倒序）。条目含 PNG base64 与绝对路径，供前端一次渲染 3D 衣柜。
#[tauri::command]
pub async fn list_local_skins() -> Vec<LocalSkinView> {
    tauri::async_runtime::spawn_blocking(skins::list_views)
        .await
        .unwrap_or_default()
}

/// 保存皮肤到本地衣柜（b64 = PNG 原文 base64；variant = CLASSIC | SLIM；
/// source = microsoft / 站点名 / upload，空值后端回落 upload）。
#[tauri::command]
pub fn save_local_skin(
    name: String,
    b64: String,
    variant: String,
    source: String,
) -> Result<LocalSkinView, String> {
    skins::save(&name, &b64, &variant, &source)
}

/// 移除本地皮肤（幂等：id 不存在 / 文件已删均成功），返回删除后的列表。
#[tauri::command]
pub fn remove_local_skin(id: String) -> Result<Vec<LocalSkinView>, String> {
    skins::remove(&id)
}

#[tauri::command]
pub async fn read_local_skin(id: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || skins::read(&id))
        .await
        .map_err(common_err_ctx_key!("errors.file.read", "读取皮肤失败"))?
}

/// 导出皮肤 PNG 到选定路径（仅接受 .png）。
#[tauri::command]
pub fn export_skin_png(path: String, b64: String) -> Result<(), String> {
    skins::export_png(&path, &b64)
}

#[tauri::command]
pub fn import_skin_png(path: String) -> Result<String, String> {
    skins::import_png(&path)
}
