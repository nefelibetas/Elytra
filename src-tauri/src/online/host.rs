//! host 侧：建虚拟网 + 信息服务器（6 个标准协议）+ 游戏端口发现与实例重建。

use std::sync::Arc;
use std::time::Duration;

use super::mc_discover::MinecraftScanner;
use super::model::{OnlineSnapshot, OnlineStatus, DEFAULT_SCAFFOLD_PORT, DISCOVER_POLL};
use super::scaffold::codec::{Request, Response};
use super::scaffold::identity::get_machine_id;
use super::scaffold::profiles::{Profile, ProfileKind, ProfileTable};
use super::scaffold::room::{create_room, RoomCredential};
use super::scaffold::server::{start, Handlers, InfoServer};
use super::service::{
    emit_snapshot, free_tcp_port, online_player_name, snapshot_inner, vendor_string, HostShared,
    OnlineServiceInner, SERVICE,
};
use super::session::{EasyTierSession, SessionConfig, SessionRole};
use crate::persist::online_config;
use crate::task::EventSink;

/// 开启联机（host）：建虚拟网（hostname + 白名单 [scaffold_port]）+ 起信息服务器。
/// 状态 `Waiting`；后台等游戏开放局域网，检测到端口后**重建实例**补白名单 → `Hosting`。
pub async fn start_host(sink: Arc<dyn EventSink>) -> Result<OnlineSnapshot, String> {
    // 阶段一（锁内、短）：校验 + 代际递增 + 占端口/房间码，随即释放锁
    let (gen, scaffold_port, room, cfg) = prepare_host_session().await?;
    // 阶段二（锁外、慢）：建虚拟网（秒级）；持锁会冻住 status()/stop()
    let session = EasyTierSession::start(cfg).await?;
    // 阶段三（锁内、短）：代际校验——被 stop/重建抢先则丢弃刚建的会话
    let mut svc = SERVICE.lock().await;
    if svc.generation != gen {
        drop(svc);
        session.stop().await;
        bail_key!("errors.task.cancelled", "联机启动已取消");
    }
    let (profiles, shared, info_server) = start_info_server(scaffold_port);
    let snap = publish_host_ready(
        sink.as_ref(),
        &mut svc,
        session,
        room,
        scaffold_port,
        profiles,
        info_server,
    );
    drop(svc);
    spawn_host_discovery(Arc::clone(&sink), Arc::clone(&shared), gen);
    spawn_host_heartbeat(sink, shared, gen);
    Ok(snap)
}

/// 阶段一：校验状态、启动扫描器、占信息服务器端口、组装建网配置。
/// 返回 (generation, scaffold_port, room, cfg)；**建网由调用方在锁外做**。
async fn prepare_host_session() -> Result<(u64, u16, RoomCredential, SessionConfig), String> {
    let mut svc = SERVICE.lock().await;
    if svc.session.is_some() {
        return Err(keyed_msg!(
            "errors.online.alreadyRunning",
            "联机已在运行中\n请先停止当前联机会话"
        ));
    }
    svc.generation += 1;
    let gen = svc.generation;
    // 扫描器提前启动：先点联机后开局域网也能收到广播（幂等复用）
    if svc.scanner.is_none() {
        svc.scanner = Some(MinecraftScanner::start());
    }
    // 信息服务器端口：优先 13448，占用则随机
    let scaffold_port = match std::net::TcpListener::bind(("0.0.0.0", DEFAULT_SCAFFOLD_PORT)) {
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
    Ok((gen, scaffold_port, room, cfg))
}

/// 阶段二：构建房主玩家表 + 起信息服务器。
/// 返回服务器句柄（None = 启动失败），调用方须存进单例以便停止时释放端口。
fn start_info_server(scaffold_port: u16) -> (ProfileTable, Arc<HostShared>, Option<InfoServer>) {
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
    let info_server = match start(handlers, scaffold_port) {
        Ok(server) => {
            eprintln!(
                "[online] Scaffolding 信息服务器监听 0.0.0.0:{}",
                server.port()
            );
            Some(server)
        }
        Err(e) => {
            eprintln!("[online] Scaffolding 信息服务器启动失败: {e}");
            None
        }
    };
    (profiles, shared, info_server)
}

fn publish_host_ready(
    sink: &dyn EventSink,
    svc: &mut OnlineServiceInner,
    session: EasyTierSession,
    room: RoomCredential,
    scaffold_port: u16,
    profiles: ProfileTable,
    info_server: Option<InfoServer>,
) -> OnlineSnapshot {
    svc.session = Some(session);
    svc.status = OnlineStatus::Waiting;
    svc.room = Some(room);
    svc.scaffold_port = Some(scaffold_port);
    svc.profiles = Some(profiles);
    svc.info_server = info_server;
    let snap = snapshot_inner(svc);
    emit_snapshot(sink, &snap);
    snap
}

/// 端口重建判定：外层 `None` = 无需动作，内层值 = 重建到的游戏端口（`None` = 去掉游戏端口）；
/// `Waiting`+有端口 → 升 `Hosting`，`Hosting`+端口消失 → 降 `Waiting`，`Hosting`+端口变化 → 重建。
/// 端口变化须显式比对：MC 关掉再 5s 内重开时旧端口仍在扫描器 TTL 内，仅靠"消失/出现"会漏掉新端口，白名单与 `shared.mc_port` 将一直指向旧端口（guest 加入必失败）。
fn host_sync_target(
    status: OnlineStatus,
    mc_port: Option<u16>,
    current: Option<u16>,
) -> Option<Option<u16>> {
    match (status, mc_port) {
        (OnlineStatus::Waiting, Some(port)) => Some(Some(port)),
        (OnlineStatus::Hosting, None) => Some(None),
        (OnlineStatus::Hosting, Some(port)) if current != Some(port) => Some(Some(port)),
        _ => None,
    }
}

fn spawn_host_discovery(sink: Arc<dyn EventSink>, shared: Arc<HostShared>, gen: u64) {
    tauri::async_runtime::spawn(async move {
        host_discovery_loop(sink.as_ref(), &shared, gen).await;
    });
}

async fn host_discovery_loop(sink: &dyn EventSink, shared: &HostShared, gen: u64) {
    loop {
        tokio::time::sleep(DISCOVER_POLL).await;
        // 锁内只读状态、判定是否需要重建；重建（秒级）在锁外做
        let sync_target = {
            let svc = SERVICE.lock().await;
            if svc.generation != gen {
                return;
            }
            if svc.role() != SessionRole::Host {
                return;
            }
            let mc_port = svc.scanner.as_ref().and_then(|sc| sc.get_port());
            host_sync_target(svc.status, mc_port, *shared.mc_port.read())
        };
        if let Some(mc_port) = sync_target {
            if !sync_host_session(sink, shared, gen, mc_port).await {
                return;
            }
        }
    }
}

/// 重建联机会话以匹配当前游戏端口（`None` = 去游戏端口白名单）；返回 `false` = 调用方应结束。
/// `old.stop()` 与 `start()` 均为秒级，放在**锁外**（持锁会卡住 status()/stop()）；期间被抢先由代际校验兜住。
async fn sync_host_session(
    sink: &dyn EventSink,
    shared: &HostShared,
    gen: u64,
    mc_port: Option<u16>,
) -> bool {
    let (old, cfg) = {
        let mut svc = SERVICE.lock().await;
        if svc.generation != gen {
            return false;
        }
        let Some(old) = svc.session.take() else {
            return false;
        };
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
        (old, cfg)
    };

    old.stop().await;
    let started = EasyTierSession::start(cfg).await;

    let mut stale_info: Option<InfoServer> = None;
    let snap = {
        let mut svc = SERVICE.lock().await;
        if svc.generation != gen {
            drop(svc);
            // 期间被 stop / 新会话抢占 → 丢弃刚建的会话
            if let Ok(s) = started {
                s.stop().await;
            }
            return false;
        }
        match started {
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
                // 会话已不可用：信息服务器一并关闭
                stale_info = svc.info_server.take();
            }
        }
        snapshot_inner(&svc)
    };
    // 锁外 drop：join accept 线程，避免阻塞 status()/stop()
    drop(stale_info);
    emit_snapshot(sink, &snap);
    true
}

/// 后台心跳任务：剔除超时 guest + 玩家表变化推前端。
fn spawn_host_heartbeat(sink: Arc<dyn EventSink>, shared: Arc<HostShared>, gen: u64) {
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
                emit_snapshot(sink.as_ref(), &snap);
            }
        }
    });
}

fn build_host_handlers(shared: Arc<HostShared>) -> Handlers {
    let handlers = Handlers::new();
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
    // c:server_port：游戏端口（大端 u16）；未检测到 → 错误 32
    {
        let shared = shared.clone();
        handlers.register(
            "c",
            "server_port",
            Arc::new(move |_req: &Request| match *shared.mc_port.read() {
                Some(port) => Ok(Response::ok(port.to_be_bytes().to_vec())),
                None => Ok(Response::fail(32, Vec::new())),
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
                    .ok_or_else(|| {
                        std::io::Error::new(
                            std::io::ErrorKind::InvalidData,
                            keyed_msg!("errors.online.profileNameMissing", "缺 name"),
                        )
                    })?
                    .to_string();
                let machine_id = value
                    .get("machine_id")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        std::io::Error::new(
                            std::io::ErrorKind::InvalidData,
                            keyed_msg!("errors.online.profileMachineIdMissing", "缺 machine_id"),
                        )
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

        let req = Request {
            namespace: "c".into(),
            value: "ping".into(),
            body: b"abc".to_vec(),
        };
        let resp = handlers.dispatch(&req).unwrap();
        assert_eq!(resp.status, 0);
        assert_eq!(resp.body, b"abc");

        let req = Request {
            namespace: "c".into(),
            value: "protocols".into(),
            body: vec![],
        };
        let resp = handlers.dispatch(&req).unwrap();
        let list = String::from_utf8(resp.body).unwrap();
        for p in [
            "c:ping",
            "c:protocols",
            "c:server_port",
            "c:player_ping",
            "c:player_profiles_list",
        ] {
            assert!(list.split('\0').any(|x| x == p), "缺协议 {p}");
        }

        let req = Request {
            namespace: "c".into(),
            value: "server_port".into(),
            body: vec![],
        };
        let resp = handlers.dispatch(&req).unwrap();
        assert_eq!(resp.status, 32, "未检测到游戏端口应报错");
        *shared.mc_port.write() = Some(25565);
        let resp = handlers.dispatch(&req).unwrap();
        assert_eq!(resp.status, 0);
        assert_eq!(resp.body, 25565u16.to_be_bytes().to_vec());

        let req = Request {
            namespace: "c".into(),
            value: "player_ping".into(),
            body: br#"{"name":"alice","machine_id":"m1","vendor":"test"}"#.to_vec(),
        };
        let resp = handlers.dispatch(&req).unwrap();
        assert_eq!(resp.status, 0);

        let req = Request {
            namespace: "c".into(),
            value: "player_profiles_list".into(),
            body: vec![],
        };
        let resp = handlers.dispatch(&req).unwrap();
        let profiles: serde_json::Value = serde_json::from_slice(&resp.body).unwrap();
        let arr = profiles.as_array().unwrap();
        assert_eq!(arr.len(), 2, "host + 1 guest");
        assert_eq!(arr[0]["kind"], "HOST");
        assert_eq!(arr[1]["name"], "alice");

        let req = Request {
            namespace: "c".into(),
            value: "nope".into(),
            body: vec![],
        };
        let resp = handlers.dispatch(&req).unwrap();
        assert_eq!(resp.status, 255);
    }

    /// Hosting 期间游戏端口变化必须触发重建（否则白名单/端口停在旧值，guest 加入失败）；
    /// 端口未变则不得重复重建。
    #[test]
    fn host_sync_target_handles_port_change() {
        assert_eq!(
            host_sync_target(OnlineStatus::Waiting, Some(25565), None),
            Some(Some(25565))
        );
        assert_eq!(
            host_sync_target(OnlineStatus::Hosting, None, Some(25565)),
            Some(None)
        );
        assert_eq!(
            host_sync_target(OnlineStatus::Hosting, Some(25566), Some(25565)),
            Some(Some(25566))
        );
        assert_eq!(
            host_sync_target(OnlineStatus::Hosting, Some(25565), Some(25565)),
            None
        );
        assert_eq!(
            host_sync_target(OnlineStatus::Idle, Some(25565), None),
            None
        );
        assert_eq!(
            host_sync_target(OnlineStatus::Joining, Some(25565), None),
            None
        );
    }
}
