//! guest 侧：加入房间 → 发现中心 → 心跳协商 → 建本地游戏代理 + MC 局域网广播模拟。

use std::net::{Ipv4Addr, SocketAddr};
use std::time::Duration;

use super::mc_advertise::McAdvertiser;
use super::model::{OnlineStatus, SCAFFOLD_HOSTNAME_PREFIX};
use super::scaffold::codec::{read_response_async, write_request_async};
use super::scaffold::identity::get_machine_id;
use super::scaffold::profiles::{Profile, ProfileKind, ProfileTable};
use super::scaffold::room::parse_room_code;
use super::service::{
    emit_snapshot, free_tcp_port, online_player_name, snapshot_inner, vendor_string,
    OnlineServiceInner, SERVICE,
};
use super::session::{EasyTierSession, SessionConfig, SessionRole};
use crate::persist::online_config;
use easytier_core::gateway::DataPlaneTcpStream;

/// 加入联机（作为 guest）：解析房间码 → 建网 → 发现中心 → 心跳 → 协商 →
/// server_port → 建游戏代理（本地 → 中心游戏端口）→ MC 广播。返回本地端口。
pub async fn join(app: &tauri::AppHandle, room_code: &str) -> Result<u16, String> {
    let (session, gen) = start_join_session(room_code).await?;
    let (stream, center_addr, center_ip) = connect_center(app, &session, gen).await?;
    let (local_profile, supports_easytier_id, game_port) =
        negotiate_guest_session(app, stream).await?;
    let local_port =
        activate_guest_join(app, &session, center_ip, game_port, &local_profile).await?;
    spawn_guest_sync(
        app,
        session,
        center_addr,
        gen,
        local_profile,
        supports_easytier_id,
    );
    Ok(local_port)
}

/// 阶段一：校验状态、解析房间码、建虚拟网并写入初始状态。
/// 返回会话与 generation；返回前释放锁（探测期间不持锁）。
async fn start_join_session(room_code: &str) -> Result<(EasyTierSession, u64), String> {
    let mut svc = SERVICE.lock().await;
    if svc.session.is_some() {
        return Err("联机已在运行中\n请先停止当前联机会话".to_string());
    }
    let room = parse_room_code(room_code)
        .ok_or_else(|| "房间码无效\n请检查格式（U/XXXX-XXXX-XXXX-XXXX）".to_string())?;
    svc.generation += 1;
    let gen = svc.generation;
    let public_nodes = online_config().public_nodes;
    let cfg = SessionConfig {
        role: SessionRole::Guest,
        room: room.clone(),
        local_port: None,
        mc_port: None,
        public_nodes,
        scaffold_port: None,
    };
    let s = EasyTierSession::start(cfg).await?;
    svc.session = Some(s.clone());
    svc.status = OnlineStatus::Joining;
    svc.room = Some(room);
    drop(svc); // 探测期间不持锁（stop 需要）
    Ok((s, gen))
}

/// 阶段二：发现联机中心并连上信息服务器（数据面）。
/// 失败时回滚会话、广播快照并返回错误文案。
async fn connect_center(
    app: &tauri::AppHandle,
    session: &EasyTierSession,
    gen: u64,
) -> Result<(DataPlaneTcpStream, SocketAddr, Ipv4Addr), String> {
    // 发现中心：轮询 route_snapshots 匹配 hostname 前缀
    let (center_ip, center_port) = match discover_center(session, gen).await {
        Some(v) => v,
        None => {
            let mut svc = SERVICE.lock().await;
            rollback_join(&mut svc);
            let snap = snapshot_inner(&svc);
            emit_snapshot(app, &snap);
            return Err("联机中心发现失败\n请确认主机已开启联机".to_string());
        }
    };

    // 连信息服务器（数据面）
    let center_addr = SocketAddr::from((center_ip, center_port));
    let stream = match session
        .data_plane_tcp_connect(center_addr, Duration::from_secs(10))
        .await
    {
        Ok(st) => st,
        Err(e) => {
            let mut svc = SERVICE.lock().await;
            rollback_join(&mut svc);
            let snap = snapshot_inner(&svc);
            emit_snapshot(app, &snap);
            bail!("连接联机中心失败\n{e}");
        }
    };
    Ok((stream, center_addr, center_ip))
}

/// 阶段三：信息协议握手（立即心跳 → 协议协商 → server_port）。
/// 返回本地玩家资料、是否支持 easytier_id、游戏端口。
async fn negotiate_guest_session(
    app: &tauri::AppHandle,
    mut stream: DataPlaneTcpStream,
) -> Result<(Profile, bool, u16), String> {
    // 立即心跳（协商前也要发，协议要求）
    let local_profile = Profile::new(
        online_player_name(),
        get_machine_id(),
        vendor_string(),
        ProfileKind::Local,
    );
    let ping_body = serde_json::json!({
        "name": local_profile.name,
        "machine_id": local_profile.machine_id,
        "vendor": local_profile.vendor,
    });
    write_request_async(&mut stream, "c", "player_ping", ping_body.to_string().as_bytes())
        .await
        .map_err(common_err_ctx!("发送心跳失败"))?;
    let _ = read_response_async(&mut stream).await;

    // 协议协商：c:protocols
    write_request_async(&mut stream, "c", "protocols", b"")
        .await
        .map_err(common_err_ctx!("协议协商失败"))?;
    let protocols_resp = read_response_async(&mut stream)
        .await
        .map_err(common_err_ctx!("协议协商失败"))?;
    let supports_easytier_id = protocols_resp.status == 0 && {
        let server = String::from_utf8_lossy(&protocols_resp.body);
        server.split('\0').any(|p| p == "c:player_easytier_id")
    };

    // c:server_port（返回游戏端口；proxy NAT 直连 host 本机 MC 监听）
    write_request_async(&mut stream, "c", "server_port", b"")
        .await
        .map_err(common_err_ctx!("获取服务器端口失败"))?;
    let port_resp = read_response_async(&mut stream)
        .await
        .map_err(common_err_ctx!("获取服务器端口失败"))?;
    if port_resp.status != 0 || port_resp.body.len() != 2 {
        let mut svc = SERVICE.lock().await;
        rollback_join(&mut svc);
        let snap = snapshot_inner(&svc);
        emit_snapshot(app, &snap);
        return Err("服务器未启动\n请让主机先在游戏里开启局域网世界".to_string());
    }
    let game_port = u16::from_be_bytes([port_resp.body[0], port_resp.body[1]]);
    drop(stream);
    Ok((local_profile, supports_easytier_id, game_port))
}

/// 阶段四：建本地游戏代理 + MC 局域网广播，回写状态并广播快照。返回本地端口。
async fn activate_guest_join(
    app: &tauri::AppHandle,
    session: &EasyTierSession,
    center_ip: Ipv4Addr,
    game_port: u16,
    local_profile: &Profile,
) -> Result<u16, String> {
    // 建游戏代理：本地 OS 监听 local_port → 数据面连 centerIP:game_port
    let local_port = free_tcp_port()?;
    let proxy_session = session.clone();
    tauri::async_runtime::spawn(async move {
        run_guest_proxy(proxy_session, local_port, center_ip, game_port).await;
    });

    // MC 广播模拟（guest 端局域网标签页看到服务器）
    let advertise_motd = online_config()
        .advertise_motd
        .filter(|s| !s.trim().is_empty());
    let motd = advertise_motd.unwrap_or_else(|| "Elytra 联机".to_string());
    let advertiser = McAdvertiser::start_with_motd(local_port, &motd);

    // 回写状态
    let mut svc = SERVICE.lock().await;
    svc.advertiser = Some(advertiser);
    if let Some(s) = svc.session.as_mut() {
        s.local_port = local_port;
    }
    svc.profiles = Some(ProfileTable::with_host(local_profile.clone()));
    let snap = snapshot_inner(&svc);
    emit_snapshot(app, &snap);
    drop(svc);
    Ok(local_port)
}

/// 阶段五：起后台心跳 + 玩家列表同步任务（每 5s）。
fn spawn_guest_sync(
    app: &tauri::AppHandle,
    session: EasyTierSession,
    center_addr: SocketAddr,
    gen: u64,
    local_profile: Profile,
    supports_easytier_id: bool,
) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let mut stream = match session
            .data_plane_tcp_connect(center_addr, Duration::from_secs(10))
            .await
        {
            Ok(st) => st,
            Err(_) => return,
        };
        guest_sync_loop(&app, &mut stream, local_profile, supports_easytier_id, gen).await;
    });
}

/// 后台心跳 + 玩家列表同步循环。
async fn guest_sync_loop(
    app: &tauri::AppHandle,
    stream: &mut DataPlaneTcpStream,
    local_profile: Profile,
    supports_easytier_id: bool,
    gen: u64,
) {
    loop {
        tokio::time::sleep(Duration::from_secs(5)).await;
        let svc = SERVICE.lock().await;
        if svc.generation != gen {
            return;
        }
        drop(svc);
        // 心跳（协商支持 easytier_id 时带字段）
        let mut ping = serde_json::json!({
            "name": local_profile.name,
            "machine_id": local_profile.machine_id,
            "vendor": local_profile.vendor,
        });
        if supports_easytier_id {
            ping["easytier_id"] = serde_json::Value::String("elytra".into());
        }
        if write_request_async(stream, "c", "player_ping", ping.to_string().as_bytes())
            .await
            .is_err()
            || read_response_async(stream).await.is_err()
        {
            return;
        }
        // 拉玩家列表
        if write_request_async(stream, "c", "player_profiles_list", b"")
            .await
            .is_ok()
        {
            let resp = match read_response_async(stream).await {
                Ok(resp) if resp.status == 0 => resp,
                _ => continue,
            };
            if let Ok(server_players) = serde_json::from_slice::<Vec<Profile>>(&resp.body) {
                let profiles = build_guest_profiles(server_players, &local_profile);
                let mut svc = SERVICE.lock().await;
                if svc.generation != gen {
                    return;
                }
                svc.profiles = Some(profiles);
                let snap = snapshot_inner(&svc);
                emit_snapshot(app, &snap);
            }
        }
    }
}

/// 由中心返回的玩家列表构建玩家表（保留本机 Local 身份）。
fn build_guest_profiles(server_players: Vec<Profile>, local_profile: &Profile) -> ProfileTable {
    let mut profiles = ProfileTable::default();
    let mut saw_local = false;
    for p in server_players {
        if p.machine_id == local_profile.machine_id {
            saw_local = true;
        }
        let kind = if p.machine_id == local_profile.machine_id {
            ProfileKind::Local
        } else {
            p.kind
        };
        let profile = Profile::new(p.name, p.machine_id, p.vendor, kind);
        if kind == ProfileKind::Host {
            profiles = ProfileTable::with_host(profile);
        } else if kind != ProfileKind::Local {
            profiles.upsert_guest(profile);
        }
    }
    if !saw_local {
        profiles.upsert_guest(local_profile.clone());
    }
    profiles
}

/// guest 游戏代理：本地 OS 监听 → 数据面连 centerIP:game_port。
async fn run_guest_proxy(
    session: EasyTierSession,
    local_port: u16,
    center_ip: Ipv4Addr,
    game_port: u16,
) {
    let listener = match tokio::net::TcpListener::bind(("0.0.0.0", local_port)).await {
        Ok(l) => l,
        Err(e) => {
            eprintln!("[online] guest 本地代理绑定失败: {e}");
            return;
        }
    };
    let dst = SocketAddr::from((center_ip, game_port));
    loop {
        match listener.accept().await {
            Ok((client, _)) => {
                let session = session.clone();
                tauri::async_runtime::spawn(async move {
                    let Ok(remote) = session
                        .data_plane_tcp_connect(dst, Duration::from_secs(10))
                        .await
                    else {
                        return;
                    };
                    let (mut cr, mut cw) = tokio::io::split(client);
                    let (mut rr, mut rw) = tokio::io::split(remote);
                    let c2r = tokio::io::copy(&mut cr, &mut rw);
                    let r2c = tokio::io::copy(&mut rr, &mut cw);
                    let _ = tokio::try_join!(c2r, r2c);
                });
            }
            Err(e) => {
                eprintln!("[online] guest 本地代理 accept 失败: {e}");
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }
    }
}

/// 发现联机中心：轮询路由表匹配 hostname 前缀（最多 15s）。
async fn discover_center(session: &EasyTierSession, gen: u64) -> Option<(Ipv4Addr, u16)> {
    for _ in 0..15 {
        tokio::time::sleep(Duration::from_secs(1)).await;
        let svc = SERVICE.lock().await;
        if svc.generation != gen {
            return None;
        }
        drop(svc);
        let routes = session.route_snapshots().await;
        for r in routes {
            if let Some(port_str) = r.hostname.strip_prefix(SCAFFOLD_HOSTNAME_PREFIX) {
                let Ok(port) = port_str.parse::<u16>() else {
                    continue;
                };
                if port > 1024 {
                    if let Some(ip) = r.ipv4_addr() {
                        return Some((ip, port));
                    }
                }
            }
        }
    }
    None
}

/// 回滚 guest 加入（失败时清理会话/广播器）。
fn rollback_join(svc: &mut OnlineServiceInner) {
    svc.generation += 1;
    if let Some(s) = svc.session.take() {
        tauri::async_runtime::block_on(s.stop());
    }
    svc.advertiser = None;
    svc.status = OnlineStatus::Idle;
    svc.room = None;
    svc.profiles = None;
    svc.scaffold_port = None;
}
