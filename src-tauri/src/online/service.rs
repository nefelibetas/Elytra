//! 联机会话单例：状态机共享状态、快照派生、会话终止与本地小工具。

use super::mc_advertise::McAdvertiser;
use super::mc_discover::MinecraftScanner;
use super::model::{OnlineSnapshot, OnlineStatus};
use super::scaffold::profiles::ProfileTable;
use super::scaffold::room::RoomCredential;
use super::scaffold::server::InfoServer;
use super::session::{EasyTierSession, SessionRole};
use crate::events::ONLINE_STATE;
use crate::persist::online_config;
use crate::task::{emit_event, EventSink};
use std::future::Future;
use std::sync::LazyLock;
use tokio::sync::Mutex;

pub(super) struct HostShared {
    /// 当前游戏端口（None = 未检测到；c:server_port 据此）。
    pub(super) mc_port: parking_lot::RwLock<Option<u16>>,
    /// 玩家表（含 HOST + GUEST）。
    pub(super) profiles: parking_lot::Mutex<ProfileTable>,
}

/// guest 本地游戏代理句柄：drop/停止时中止后台任务并释放本地监听端口（丢弃 `JoinHandle` 会让代理一直 LISTEN）。
pub(super) struct GuestProxy {
    handle: Option<tauri::async_runtime::JoinHandle<()>>,
}

impl GuestProxy {
    pub(super) fn spawn<F>(task: F) -> Self
    where
        F: Future<Output = ()> + Send + 'static,
    {
        Self {
            handle: Some(tauri::async_runtime::spawn(task)),
        }
    }

    /// 中止代理任务并等待其真正结束（保证监听端口已释放）。
    pub(super) async fn shutdown(mut self) {
        if let Some(handle) = self.handle.take() {
            handle.abort();
            let _ = handle.await;
        }
    }
}

impl Drop for GuestProxy {
    fn drop(&mut self) {
        if let Some(handle) = self.handle.take() {
            handle.abort();
        }
    }
}

/// OnlineService 单例（对齐 TaskManager：进程内唯一，模块级 LazyLock）。
pub(super) struct OnlineServiceInner {
    pub(super) session: Option<EasyTierSession>,
    pub(super) status: OnlineStatus,
    pub(super) room: Option<RoomCredential>,
    /// 组播扫描器（host 用；drop 即停线程）。
    pub(super) scanner: Option<MinecraftScanner>,
    /// 组播广播器（guest 用；drop 即停线程）。
    pub(super) advertiser: Option<McAdvertiser>,
    /// guest 本地游戏代理任务。
    pub(super) proxy: Option<GuestProxy>,
    /// host 信息服务器（drop 即停 accept 线程并释放端口）。
    pub(super) info_server: Option<InfoServer>,
    /// 会话代际：stop/start 递增，旧后台任务据此退出。
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
            proxy: None,
            info_server: None,
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

pub(super) fn emit_snapshot(sink: &dyn EventSink, snap: &OnlineSnapshot) {
    emit_event(sink, ONLINE_STATE, snap.clone());
}

/// 停止联机（幂等）。
pub async fn stop(sink: &dyn EventSink) -> Result<(), String> {
    // 锁内（短）：先递增代际（在途 start/重建据此认输并丢弃自己刚建的会话），
    // 再摘出会话与资源、复位状态。
    let (session, proxy, info_server, snap) = {
        let mut svc = SERVICE.lock().await;
        svc.generation += 1;
        let session = svc.session.take();
        svc.scanner = None; // Drop → 组播线程退出（只发退出信号，非阻塞）
        svc.advertiser = None; // Drop → 广播线程退出
        let proxy = svc.proxy.take(); // 中止代理任务 → 释放 guest 本地监听端口
        let info_server = svc.info_server.take(); // 停 accept 线程 → 释放 host 信息端口
        svc.status = OnlineStatus::Idle;
        svc.room = None;
        svc.scaffold_port = None;
        svc.profiles = None;
        (session, proxy, info_server, snapshot_inner(&svc))
    };
    if let Some(p) = proxy {
        p.shutdown().await;
    }
    // InfoServer::drop 会 join accept 线程：放锁外，避免阻塞 status()/stop()
    drop(info_server);
    // 会话停止是秒级；放最后，端口/线程先释放
    if let Some(s) = session {
        s.stop().await;
    }
    emit_snapshot(sink, &snap);
    Ok(())
}

pub async fn status() -> OnlineSnapshot {
    let svc = SERVICE.lock().await;
    snapshot_inner(&svc)
}

pub(super) fn free_tcp_port() -> Result<u16, String> {
    std::net::TcpListener::bind(("127.0.0.1", 0))
        .and_then(|l| l.local_addr())
        .map(|a| a.port())
        .map_err(common_err_ctx!("分配本地端口失败"))
}

/// 联机昵称：配置为空则回退系统用户名。
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

/// vendor 字符串：`Elytra {版本}`。
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
