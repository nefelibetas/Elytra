//! host 侧：建虚拟网 + 信息服务器（6 个标准协议）+ 游戏端口发现与实例重建。

use std::sync::Arc;
use std::time::Duration;

use super::mc_discover::MinecraftScanner;
use super::model::{OnlineSnapshot, OnlineStatus, DEFAULT_SCAFFOLD_PORT, DISCOVER_POLL};
use super::scaffold::codec::{Request, Response};
use super::scaffold::identity::get_machine_id;
use super::scaffold::profiles::{Profile, ProfileKind, ProfileTable};
use super::scaffold::room::{create_room, RoomCredential};
use super::scaffold::server::{start, Handlers};
use super::service::{
    emit_snapshot, free_tcp_port, online_player_name, snapshot_inner, vendor_string, HostShared,
    OnlineServiceInner, SERVICE,
};
use super::session::{EasyTierSession, SessionConfig, SessionRole};
use crate::persist::online_config;

/// 开启联机（作为 host）：建虚拟网（hostname + 白名单 [scaffold_port]）+
/// 起信息服务器（OS 监听 scaffold_port，guest 经 proxy NAT 直连）。
/// 状态 `Waiting`；后台等游戏"对局域网开放"，检测到端口后**重建实例**
/// 白名单补游戏端口（proxy NAT 直连本机 MC 监听）→ `Hosting`。
pub async fn start_host(app: &tauri::AppHandle) -> Result<OnlineSnapshot, String> {
    let mut svc = SERVICE.lock().await;
    let (gen, scaffold_port, room, session) = start_host_session(&mut svc).await?;
    let (profiles, shared) = start_info_server(scaffold_port);
    let snap = publish_host_ready(app, &mut svc, session, room, scaffold_port, profiles);
    drop(svc);
    spawn_host_discovery(app, Arc::clone(&shared), gen);
    spawn_host_heartbeat(app, shared, gen);
    Ok(snap)
}

/// 阶段一：校验状态、启动扫描器、占信息服务器端口、建虚拟网。
/// 返回 (generation, scaffold_port, room, session)；锁由调用方持有。
async fn start_host_session(
    svc: &mut OnlineServiceInner,
) -> Result<(u64, u16, RoomCredential, EasyTierSession), String> {
    if svc.session.is_some() {
        return Err("联机已在运行中\n请先停止当前联机会话".to_string());
    }
    svc.generation += 1;
    let gen = svc.generation;
    // 扫描器提前启动：用户先点联机后开局域网也能收到广播（幂等复用）
    if svc.scanner.is_none() {
        svc.scanner = Some(MinecraftScanner::start());
    }
    // 信息服务器端口：优先 13448（参考 Terracotta），占用则随机
    let scaffold_port =
        match std::net::TcpListener::bind(("0.0.0.0", DEFAULT_SCAFFOLD_PORT)) {
            Ok(l) => {
                drop(l);
                DEFAULT_SCAFFOLD_PORT
            }
            Err(_) => free_tcp_port()?,
        };
    let room = create_room()?;
    let public_nodes = online_config().public_nodes;
    let cfg = SessionConfig {
        role: SessionRole::Host,
        room: room.clone(),
        local_port: None,
        mc_port: None,
        public_nodes,
        scaffold_port: Some(scaffold_port),
    };
    let s = EasyTierSession::start(cfg).await?;
    Ok((gen, scaffold_port, room, s))
}

/// 阶段二：构建房主玩家表 + 起信息服务器。
fn start_info_server(scaffold_port: u16) -> (ProfileTable, Arc<HostShared>) {
    // host 玩家表：房主
    let host_profile = Profile::new(
        online_player_name(),
        get_machine_id(),
        vendor_string(),
        ProfileKind::Host,
    );
    let profiles = ProfileTable::with_host(host_profile);
    let shared = Arc::new(HostShared {
        mc_port: parking_lot::RwLock::new(None),
        profiles: parking_lot::Mutex::new(profiles.clone()),
    });
    // 起信息服务器（OS 监听 0.0.0.0:scaffold_port，blocking 线程）
    let handlers = build_host_handlers(shared.clone());
    if let Err(e) = start(handlers, scaffold_port) {
        eprintln!("[online] Scaffolding 信息服务器启动失败: {e}");
    }
    (profiles, shared)
}

/// 阶段三：回写状态为 `Waiting` 并广播快照。
fn publish_host_ready(
    app: &tauri::AppHandle,
    svc: &mut OnlineServiceInner,
    session: EasyTierSession,
    room: RoomCredential,
    scaffold_port: u16,
    profiles: ProfileTable,
) -> OnlineSnapshot {
    svc.session = Some(session);
    svc.status = OnlineStatus::Waiting;
    svc.room = Some(room);
    svc.scaffold_port = Some(scaffold_port);
    svc.profiles = Some(profiles);
    let snap = snapshot_inner(svc);
    emit_snapshot(app, &snap);
    snap
}

/// 阶段四：后台发现任务：Waiting → Hosting（游戏端口出现 → 重建实例补白名单）。
fn spawn_host_discovery(app: &tauri::AppHandle, shared: Arc<HostShared>, gen: u64) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        host_discovery_loop(&app, &shared, gen).await;
    });
}

/// 后台发现循环：轮询游戏端口，按需重建联机会话。
async fn host_discovery_loop(app: &tauri::AppHandle, shared: &HostShared, gen: u64) {
    loop {
        tokio::time::sleep(DISCOVER_POLL).await;
        let mut svc = SERVICE.lock().await;
        if svc.generation != gen {
            return;
        }
        if svc.role() != SessionRole::Host {
            return;
        }
        let mc_port = svc.scanner.as_ref().and_then(|sc| sc.get_port());
        // Waiting+有端口 → 重建实例补白名单（proxy NAT 直连本机 MC 监听）；
        // Hosting+端口消失 → 降回 Waiting（重建实例去布白名单）。
        let sync_target = match (svc.status, mc_port) {
            (OnlineStatus::Waiting, Some(port)) => Some(Some(port)),
            (OnlineStatus::Hosting, None) => Some(None),
            _ => None,
        };
        if let Some(port) = sync_target {
            if !sync_host_session(app, &mut svc, shared, port).await {
                return;
            }
        }
    }
}

/// 重建联机会话以匹配当前游戏端口（`None` = 去游戏端口白名单）。
/// 返回 `false` 表示调用方应结束后台任务。
async fn sync_host_session(
    app: &tauri::AppHandle,
    svc: &mut OnlineServiceInner,
    shared: &HostShared,
    mc_port: Option<u16>,
) -> bool {
    let Some(old) = svc.session.take() else {
        return false;
    };
    old.stop().await;
    let Some(room) = svc.room.clone() else {
        return false;
    };
    let cfg = SessionConfig {
        role: SessionRole::Host,
        room,
        local_port: None,
        mc_port,
        public_nodes: online_config().public_nodes,
        scaffold_port: Some(svc.scaffold_port.unwrap_or(0)),
    };
    match EasyTierSession::start(cfg).await {
        Ok(new_s) => {
            svc.session = Some(new_s);
            *shared.mc_port.write() = mc_port;
            svc.status = if mc_port.is_some() {
                OnlineStatus::Hosting
            } else {
                OnlineStatus::Waiting
            };
        }
        Err(e) => {
            eprintln!("[online] 重建联机会话失败: {e}");
            svc.status = OnlineStatus::Idle;
            svc.room = None;
            svc.scaffold_port = None;
            svc.scanner = None;
        }
    }
    let snap = snapshot_inner(svc);
    emit_snapshot(app, &snap);
    true
}

/// 阶段五：后台心跳维护任务：剔除超时 guest + 玩家表变化推前端。
fn spawn_host_heartbeat(app: &tauri::AppHandle, shared: Arc<HostShared>, gen: u64) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(2)).await;
            let svc = SERVICE.lock().await;
            if svc.generation != gen {
                return;
            }
            let changed = {
                let mut profiles = shared.profiles.lock();
                profiles.purge_expired()
            };
            if changed {
                let snap = snapshot_inner(&svc);
                emit_snapshot(&app, &snap);
            }
        }
    });
}

/// 构建 host 信息服务器 handlers（6 个标准协议）。
fn build_host_handlers(shared: Arc<HostShared>) -> Handlers {
    let handlers = Handlers::new();
    // c:ping：回显请求体
    handlers.register(
        "c",
        "ping",
        Arc::new(move |req: &Request| Ok(Response::ok(req.body.clone()))),
    );
    // c:protocols：返回支持的全部标准协议（\0 分割）
    handlers.register(
        "c",
        "protocols",
        Arc::new(move |_req: &Request| {
            let list = [
                "c:ping",
                "c:protocols",
                "c:server_port",
                "c:player_ping",
                "c:player_profiles_list",
                "c:player_easytier_id",
            ];
            Ok(Response::ok(list.join("\0").into_bytes()))
        }),
    );
    // c:server_port：游戏端口（proxy NAT 直连本机 MC 端口，大端 u16）；
    // 未检测到游戏端口 → 错误 32（服务器未启动）
    {
        let shared = shared.clone();
        handlers.register(
            "c",
            "server_port",
            Arc::new(move |_req: &Request| {
                match *shared.mc_port.read() {
                    Some(port) => Ok(Response::ok(port.to_be_bytes().to_vec())),
                    None => Ok(Response::fail(32, Vec::new())),
                }
            }),
        );
    }
    // c:player_ping：心跳（JSON），更新玩家表
    {
        let shared = shared.clone();
        handlers.register(
            "c",
            "player_ping",
            Arc::new(move |req: &Request| {
                let value: serde_json::Value = serde_json::from_slice(&req.body)
                    .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
                let name = value
                    .get("name")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidData, "缺 name"))?
                    .to_string();
                let machine_id = value
                    .get("machine_id")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        std::io::Error::new(std::io::ErrorKind::InvalidData, "缺 machine_id")
                    })?
                    .to_string();
                let vendor = value
                    .get("vendor")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let guest = Profile::new(name, machine_id, vendor, ProfileKind::Guest);
                shared.profiles.lock().upsert_guest(guest);
                Ok(Response::ok(Vec::new()))
            }),
        );
    }
    // c:player_profiles_list：玩家列表（JSON 数组）
    {
        let shared = shared.clone();
        handlers.register(
            "c",
            "player_profiles_list",
            Arc::new(move |_req: &Request| {
                let profiles = shared.profiles.lock().profiles();
                let body = serde_json::to_vec(&profiles)
                    .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
                Ok(Response::ok(body))
            }),
        );
    }
    handlers
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 生产 handler 行为：c:ping 回显 / c:server_port 按游戏端口 / 玩家表维护。
    #[test]
    fn host_handlers_behavior() {
        let shared = Arc::new(HostShared {
            mc_port: parking_lot::RwLock::new(None),
            profiles: parking_lot::Mutex::new(ProfileTable::with_host(Profile::new(
                "host".into(),
                "m0".into(),
                "test".into(),
                ProfileKind::Host,
            ))),
        });
        let handlers = build_host_handlers(shared.clone());

        // c:ping 回显
        let req = Request { namespace: "c".into(), value: "ping".into(), body: b"abc".to_vec() };
        let resp = handlers.dispatch(&req).unwrap();
        assert_eq!(resp.status, 0);
        assert_eq!(resp.body, b"abc");

        // c:protocols 含全部标准协议
        let req = Request { namespace: "c".into(), value: "protocols".into(), body: vec![] };
        let resp = handlers.dispatch(&req).unwrap();
        let list = String::from_utf8(resp.body).unwrap();
        for p in ["c:ping", "c:protocols", "c:server_port", "c:player_ping", "c:player_profiles_list"] {
            assert!(list.split('\0').any(|x| x == p), "缺协议 {p}");
        }

        // c:server_port：无游戏端口 → 32；有 → 返回端口
        let req = Request { namespace: "c".into(), value: "server_port".into(), body: vec![] };
        let resp = handlers.dispatch(&req).unwrap();
        assert_eq!(resp.status, 32, "未检测到游戏端口应报错");
        *shared.mc_port.write() = Some(25565);
        let resp = handlers.dispatch(&req).unwrap();
        assert_eq!(resp.status, 0);
        assert_eq!(resp.body, 25565u16.to_be_bytes().to_vec());

        // c:player_ping：JSON 更新玩家表
        let req = Request {
            namespace: "c".into(),
            value: "player_ping".into(),
            body: br#"{"name":"alice","machine_id":"m1","vendor":"test"}"#.to_vec(),
        };
        let resp = handlers.dispatch(&req).unwrap();
        assert_eq!(resp.status, 0);

        // c:player_profiles_list：host + alice
        let req = Request { namespace: "c".into(), value: "player_profiles_list".into(), body: vec![] };
        let resp = handlers.dispatch(&req).unwrap();
        let profiles: serde_json::Value = serde_json::from_slice(&resp.body).unwrap();
        let arr = profiles.as_array().unwrap();
        assert_eq!(arr.len(), 2, "host + 1 guest");
        assert_eq!(arr[0]["kind"], "HOST");
        assert_eq!(arr[1]["name"], "alice");

        // 未知协议 → 255
        let req = Request { namespace: "c".into(), value: "nope".into(), body: vec![] };
        let resp = handlers.dispatch(&req).unwrap();
        assert_eq!(resp.status, 255);
    }
}
