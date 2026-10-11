//! 联机命令（薄胶水）：host 开房间 / guest 加入 / 停止 / 状态查询 / 配置读写。
//! 业务逻辑在 online/ 领域层，此处仅转发 + 错误展平。
//! 顺序自由：host 可先开联机再在游戏里"对局域网开放"（后台自动检测端口），
//! guest 从房间码解析端口，均无需手动填写。
use crate::config::online::OnlineConfig;
use crate::online::{self, OnlineSnapshot};
use crate::persist;
use crate::task::EventSink;
use std::sync::Arc;

/// 开启联机（作为 host）。立即返回 Waiting 快照（房间码为身份段）；
/// 检测到游戏端口后经 `online-state` 事件推送 Hosting + 完整房间码。
#[tauri::command]
pub async fn online_start_host(app: tauri::AppHandle) -> Result<OnlineSnapshot, String> {
    let sink: Arc<dyn EventSink> = Arc::new(app.clone());
    online::start_host(sink).await
}

/// 加入联机（作为 guest）。返回本地转发端口（游戏内填 127.0.0.1:此端口）。
#[tauri::command]
pub async fn online_join(app: tauri::AppHandle, room_code: String) -> Result<u16, String> {
    let sink: Arc<dyn EventSink> = Arc::new(app.clone());
    online::join(sink, &room_code).await
}

/// 停止联机。
#[tauri::command]
pub async fn online_stop(app: tauri::AppHandle) -> Result<(), String> {
    let sink: Arc<dyn EventSink> = Arc::new(app.clone());
    online::stop(sink.as_ref()).await
}

/// 当前联机状态快照（纯读）。
#[tauri::command]
pub async fn online_status() -> OnlineSnapshot {
    online::status().await
}

/// 读取联机配置（协调节点列表）。
#[tauri::command]
pub fn get_online_config() -> OnlineConfig {
    persist::online_config()
}

/// 保存联机配置（协调节点列表）。
#[tauri::command]
pub fn save_online_config(config: OnlineConfig) -> Result<(), String> {
    persist::save_online_config(&config).map_err(common_err_ctx!("保存联机配置失败"))
}
