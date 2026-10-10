//! 联机会话单例：状态机共享状态、快照派生、会话终止与本地小工具。

use std::sync::LazyLock;
use tokio::sync::Mutex;

use super::mc_advertise::McAdvertiser;
use super::mc_discover::MinecraftScanner;
use super::model::{OnlineSnapshot, OnlineStatus};
use super::scaffold::profiles::ProfileTable;
use super::scaffold::room::RoomCredential;
use super::session::{EasyTierSession, SessionRole};
use crate::events::ONLINE_STATE;
use crate::persist::online_config;
use crate::task::emit_event;

/// host 共享状态（信息服务器 handler + 代理任务用）。
pub(super) struct HostShared {
    /// 当前游戏端口（None = 未检测到；c:server_port 据此）。
    pub(super) mc_port: parking_lot::RwLock<Option<u16>>,
    /// 玩家表（含 HOST + GUEST）。
    pub(super) profiles: parking_lot::Mutex<ProfileTable>,
}

/// OnlineService 单例（对齐 TaskManager：进程内唯一，模块级 LazyLock）。
pub(super) struct OnlineServiceInner {
    pub(super) session: Option<EasyTierSession>,
    pub(super) status: OnlineStatus,
    pub(super) room: Option<RoomCredential>,
    /// 组播扫描器（host 用；drop 即停线程）。
    pub(super) scanner: Option<MinecraftScanner>,
    /// 组播广播器（guest 用：模拟 MC 局域网服务器；drop 即停线程）。
    pub(super) advertiser: Option<McAdvertiser>,
    /// 会话代际：stop/start_host 递增，旧的后台任务据此自杀。
    pub(super) generation: u64,
    /// 信息服务器端口（host；guest 侧为空）。
    pub(super) scaffold_port: Option<u16>,
    /// host 玩家表（guest 侧为空）。
    pub(super) profiles: Option<ProfileTable>,
}

impl Default for OnlineServiceInner {
    fn default() -> Self {
        Self {
            session: None,
            status: OnlineStatus::Idle,
            room: None,
            scanner: None,
            advertiser: None,
            generation: 0,
            scaffold_port: None,
            profiles: None,
        }
    }
}

impl OnlineServiceInner {
    /// 当前会话角色（无会话/空闲 → Host，后台任务据此判断是否自管）。
    pub(super) fn role(&self) -> SessionRole {
        self.session
            .as_ref()
            .map(|s| s.role)
            .unwrap_or(SessionRole::Host)
    }
}

pub(super) static SERVICE: LazyLock<Mutex<OnlineServiceInner>> =
    LazyLock::new(|| Mutex::new(OnlineServiceInner::default()));

pub(super) fn snapshot_inner(svc: &OnlineServiceInner) -> OnlineSnapshot {
    OnlineSnapshot {
        status: svc.status.as_str(),
        room_code: svc.room.as_ref().map(|r| r.code.clone()),
        local_port: if svc.status == OnlineStatus::Joining {
            svc.session.as_ref().map(|s| s.local_port)
        } else {
            None
        },
        players: svc
            .profiles
            .as_ref()
            .map(|t| t.profiles())
            .unwrap_or_default(),
    }
}

pub(super) fn emit_snapshot(app: &tauri::AppHandle, snap: &OnlineSnapshot) {
    emit_event(app, ONLINE_STATE, snap.clone());
}

/// 停止联机（幂等）。
pub async fn stop(app: &tauri::AppHandle) -> Result<(), String> {
    let mut svc = SERVICE.lock().await;
    svc.generation += 1;
    if let Some(s) = svc.session.take() {
        s.stop().await;
    }
    svc.scanner = None; // Drop → 组播线程退出
    svc.advertiser = None; // Drop → 广播线程退出
    svc.status = OnlineStatus::Idle;
    svc.room = None;
    svc.scaffold_port = None;
    svc.profiles = None;
    let snap = snapshot_inner(&svc);
    emit_snapshot(app, &snap);
    Ok(())
}

/// 当前状态快照（纯读）。
pub async fn status() -> OnlineSnapshot {
    let svc = SERVICE.lock().await;
    snapshot_inner(&svc)
}

/// 探测一个空闲 TCP 端口并释放。
pub(super) fn free_tcp_port() -> Result<u16, String> {
    std::net::TcpListener::bind(("127.0.0.1", 0))
        .and_then(|l| l.local_addr())
        .map(|a| a.port())
        .map_err(common_err_ctx!("分配本地端口失败"))
}

/// 联机昵称：读配置（OnlineConfig.nickname），空则回退系统用户名。
pub(super) fn online_player_name() -> String {
    let cfg = online_config();
    if let Some(name) = cfg.nickname.as_ref().filter(|s| !s.trim().is_empty()) {
        name.clone()
    } else {
        std::env::var("USERNAME")
            .or_else(|_| std::env::var("USER"))
            .unwrap_or_else(|_| "Player".to_string())
    }
}

/// vendor 字符串：`Elytra {版本}`（协议 c:player_ping 的 vendor 字段）。
pub(super) fn vendor_string() -> String {
    format!("Elytra {}", crate::constants::APP_VERSION)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn player_name_fallback() {
        // 无配置 → 回退系统用户名（非空）
        let name = online_player_name();
        assert!(!name.is_empty());
    }
}
