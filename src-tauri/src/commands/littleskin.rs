//! LittleSkin 第三方站皮肤/披风管理命令（薄胶水层）：逻辑在 auth/littleskin.rs。
//!
//! 能力边界：站点 API **没有** 材质上传端点（依据见 `auth::littleskin` 模块注释），
//! 因此不存在「上传本地皮肤到第三方站」的命令。`*_closet_item` 系列命令接收的 `tid`
//! 必须是站点皮肤库中**已存在**的材质 ID（用户自己在网页上传的材质，或任意公开材质）。

use crate::auth::littleskin;

/// 开始 LittleSkin 设备代码流授权：返回授权码、授权页 URL 与轮询句柄。
/// `device_code` 只留在后端内存，不返回前端。
#[tauri::command]
pub async fn start_littleskin_auth() -> Result<littleskin::DeviceAuthView, String> {
    littleskin::start_device_authorization().await
}

/// 轮询授权结果。前端按返回的 `interval` 秒调用，
/// 直到 `status` 不再是 `pending` / `slow_down`。
#[tauri::command]
pub async fn poll_littleskin_auth(handle: String) -> littleskin::PollOutcome {
    littleskin::poll_device_authorization(&handle).await
}

/// 本地授权状态（不联网；None = 未授权）。
#[tauri::command]
pub fn get_littleskin_account() -> Option<littleskin::AccountView> {
    littleskin::account_view()
}

/// 联网刷新账号信息（GET /api/user），顺带续期即将过期的访问令牌。
#[tauri::command]
pub async fn refresh_littleskin_account() -> Result<littleskin::AccountView, String> {
    littleskin::fetch_account().await
}

/// 退出授权：清除本地加密令牌（幂等）。
#[tauri::command]
pub fn logout_littleskin() -> Result<(), String> {
    littleskin::logout()
}

/// 角色列表（GET /api/players，含已应用的皮肤/披风 TID）。
#[tauri::command]
pub async fn list_littleskin_players() -> Result<Vec<littleskin::SitePlayer>, String> {
    littleskin::fetch_players().await
}

/// 衣柜列表（category = `skin` | `cape`，page 从 1 开始）。
#[tauri::command]
pub async fn list_littleskin_closet(
    category: String,
    page: Option<u32>,
) -> Result<littleskin::ClosetPage, String> {
    let category = littleskin::ClosetCategory::parse(&category)?;
    littleskin::fetch_closet(category, page.unwrap_or(1)).await
}

/// 把站点皮肤库中已存在的材质加入衣柜（站点无上传 API，tid 需由用户在站点获取）。
#[tauri::command]
pub async fn add_littleskin_closet_item(tid: i64, name: String) -> Result<(), String> {
    littleskin::add_closet_item(tid, &name).await
}

/// 重命名衣柜条目（PUT /api/closet/{tid}）。
#[tauri::command]
pub async fn rename_littleskin_closet_item(tid: i64, name: String) -> Result<(), String> {
    littleskin::rename_closet_item(tid, &name).await
}

/// 从衣柜移除条目（DELETE /api/closet/{tid}）。
#[tauri::command]
pub async fn remove_littleskin_closet_item(tid: i64) -> Result<(), String> {
    littleskin::remove_closet_item(tid).await
}

/// 应用材质到角色（PUT /api/players/{pid}/textures）。
/// skin / cape 至少传一个；TID 必须已在衣柜中。
#[tauri::command]
pub async fn apply_littleskin_textures(
    pid: i64,
    skin: Option<i64>,
    cape: Option<i64>,
) -> Result<(), String> {
    littleskin::apply_textures(pid, skin, cape).await
}

/// 重置角色材质（DELETE /api/players/{pid}/textures），skin / cape 至少一个为 true。
#[tauri::command]
pub async fn reset_littleskin_textures(pid: i64, skin: bool, cape: bool) -> Result<(), String> {
    littleskin::reset_textures(pid, skin, cape).await
}
