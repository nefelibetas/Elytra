//! 本地皮肤收藏命令（衣柜「皮肤」半边，薄胶水层）：逻辑在 auth/skins.rs。
//! 披风侧沿用 account.rs 现成命令（fetch_capes / set_active_cape / disable_cape）。

use crate::auth::skins::{self, LocalSkinView};

/// 本地皮肤列表（添加时间倒序）。条目含 PNG base64 与绝对路径，供前端一次渲染 3D 衣柜。
#[tauri::command]
pub fn list_local_skins() -> Vec<LocalSkinView> {
    skins::list_views()
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

/// 保存用户选中的本地 PNG 文件到衣柜（前端经 dialog 只拿到路径，WebView 无 fs 插件读不了字节）。
#[tauri::command]
pub fn save_local_skin_file(
    name: String,
    file_path: String,
    variant: String,
    source: String,
) -> Result<LocalSkinView, String> {
    skins::save_path(&name, &file_path, &variant, &source)
}

/// 移除本地皮肤（幂等：id 不存在 / 文件已删均成功），返回删除后的列表。
#[tauri::command]
pub fn remove_local_skin(id: String) -> Result<Vec<LocalSkinView>, String> {
    skins::remove(&id)
}

/// 读取本地皮肤 PNG（base64），供前端单独刷新预览/应用。
#[tauri::command]
pub fn read_local_skin(id: String) -> Result<String, String> {
    skins::read(&id)
}
