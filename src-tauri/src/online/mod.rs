//! 联机领域层：Scaffolding 协议 + EasyTier 库内嵌虚拟局域网。
//!
//! 协议（Scaffolding-MC/Scaffolding-MC）：
//! - 房间码 `U/XXXX-XXXX-XXXX-XXXX`（base34 + 模 7 校验）→ 网络名/密钥明文映射；
//! - 联机中心 = host 虚拟网节点，hostname `scaffolding-mc-server-{port}`；
//! - 信息协议 TCP：`c:ping` / `c:protocols` / `c:server_port` / `c:player_ping` /
//!   `c:player_profiles_list`（全部标准协议，命名空间 `c`）；
//! - 标准流程：guest 入网 → 发现中心 → 心跳 → 协商 → server_port → 每 5s 心跳。
//!
//! 实现要点（no_tun + smoltcp：虚拟网流量全走数据面，无虚拟网卡）：
//! - **信息协议走数据面**：host `data_plane_tcp_bind(scaffold_port)` 在虚拟 IP 监听，
//!   guest `data_plane_tcp_connect(虚拟IP:scaffold_port)` 连接；
//! - **游戏流量也走数据面**：host `data_plane_tcp_bind(entry)` 应用层代理
//!   → `127.0.0.1:game_port`（MC 监听）；guest `data_plane_tcp_connect(虚拟IP:entry)`
//!   代理 → MC 客户端本地端口。`c:server_port` 返回 entry（虚拟网内可达的 MC 端口）；
//! - **MC 会话校验绕行**：guest 端 `mc_advertise` 广播模拟局域网服务器（保留）；
//! - **host 游戏端口检测**：`mc_discover` 组播扫描（保留），检测到后代理目标更新；
//! - 白名单 [scaffold_port, entry] 建网时一次配好（ACL 管数据面入站）。
//!
//! 事件：经 `EMIT_TX` 队列派发 `online-state`（载荷 [`OnlineSnapshot`]，含玩家列表）。

mod mc_advertise;
mod mc_discover;
mod scaffold;
mod session;

pub use scaffold::room::{create_room, parse_room_code, RoomCredential};
pub use scaffold::profiles::{Profile, ProfileKind};
pub use session::{EasyTierSession, RouteSnapshot, SessionConfig, SessionRole};

use std::net::{Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::sync::LazyLock;
use std::time::Duration;
use tokio::sync::Mutex;

use parking_lot::Mutex as PMutex;

use crate::events::ONLINE_STATE;
use crate::task::emit_event;

use self::mc_advertise::McAdvertiser;
use self::mc_discover::MinecraftScanner;
use self::scaffold::codec::{read_response_async, write_request_async, Request, Response};
use self::scaffold::profiles::ProfileTable;
use self::scaffold::server::Handlers;

/// 端口发现轮询间隔（游戏每 1.5s 广播一次组播，1s 轮询足够快）。
const DISCOVER_POLL: Duration = Duration::from_secs(1);

/// Scaffolding 信息服务器默认端口（参考 Terracotta；占用时回退随机）。
const DEFAULT_SCAFFOLD_PORT: u16 = 13448;

/// 协议前缀：中心 hostname。
const SCAFFOLD_HOSTNAME_PREFIX: &str = "scaffolding-mc-server-";

/// 联机状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OnlineStatus {
    /// 未联机。
    Idle,
    /// host 虚拟网已建、等待游戏开放局域网（未拿到游戏端口）。
    Waiting,
    /// host 联机中（已检测到游戏端口，代理生效）。
    Hosting,
    /// guest 已加入。
    Joining,
}

impl OnlineStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Waiting => "waiting",
            Self::Hosting => "hosting",
            Self::Joining => "joining",
        }
    }
}

/// 联机状态快照：事件载荷 + 状态查询返回值（前端单一数据源）。
#[derive(Debug, Clone, serde::Serialize)]
pub struct OnlineSnapshot {
    pub status: &'static str,
    pub room_code: Option<String>,
    /// guest 本地转发端口（游戏内连 127.0.0.1:<port>，经"局域网"标签页进入）；非 guest 为 None。
    pub local_port: Option<u16>,
    /// 玩家列表（HOST + GUEST；协议 c:player_profiles_list）。
    pub players: Vec<Profile>,
}

/// host 共享状态（信息服务器 handler + 代理任务用）。
struct HostShared {
    /// 当前游戏端口（None = 未检测到；c:server_port 据此）。
    mc_port: parking_lot::RwLock<Option<u16>>,
    /// 玩家表（含 HOST + GUEST）。
    profiles: PMutex<ProfileTable>,
}

/// OnlineService 单例（对齐 TaskManager：进程内唯一，模块级 LazyLock）。
struct OnlineServiceInner {
    session: Option<EasyTierSession>,
    status: OnlineStatus,
    room: Option<RoomCredential>,
    /// 组播扫描器（host 用；drop 即停线程）。
    scanner: Option<MinecraftScanner>,
    /// 组播广播器（guest 用：模拟 MC 局域网服务器；drop 即停线程）。
    advertiser: Option<McAdvertiser>,
    /// 会话代际：stop/start_host 递增，旧的后台任务据此自杀。
    generation: u64,
    /// 信息服务器端口（host；guest 侧为空）。
    scaffold_port: Option<u16>,
    /// host 玩家表（guest 侧为空）。
    profiles: Option<ProfileTable>,
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
    fn role(&self) -> SessionRole {
        self.session
            .as_ref()
            .map(|s| s.role)
            .unwrap_or(SessionRole::Host)
    }
}

static SERVICE: LazyLock<Mutex<OnlineServiceInner>> =
    LazyLock::new(|| Mutex::new(OnlineServiceInner::default()));

fn snapshot_inner(svc: &OnlineServiceInner) -> OnlineSnapshot {
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

fn emit_snapshot(app: &tauri::AppHandle, snap: &OnlineSnapshot) {
    emit_event(app, ONLINE_STATE, snap.clone());
}

/// 开启联机（作为 host）：建虚拟网（hostname + 白名单 [scaffold_port]）+
/// 起信息服务器（OS 监听 scaffold_port，guest 经 proxy NAT 直连）。
/// 状态 `Waiting`；后台等游戏"对局域网开放"，检测到端口后**重建实例**
/// 白名单补游戏端口（proxy NAT 直连本机 MC 监听）→ `Hosting`。
pub async fn start_host(app: &tauri::AppHandle) -> Result<OnlineSnapshot, String> {
    let mut svc = SERVICE.lock().await;
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
    let room = create_room();
    let public_nodes = crate::config::online::OnlineConfig::from_persist().public_nodes;
    let cfg = SessionConfig {
        role: SessionRole::Host,
        room: room.clone(),
        local_port: None,
        mc_port: None,
        public_nodes,
        scaffold_port: Some(scaffold_port),
    };
    let s = EasyTierSession::start(cfg).await?;
    // host 玩家表：房主
    let host_profile = Profile::new(
        online_player_name(),
        scaffold::identity::get_machine_id(),
        vendor_string(),
        ProfileKind::Host,
    );
    let profiles = ProfileTable::with_host(host_profile);
    let shared = Arc::new(HostShared {
        mc_port: parking_lot::RwLock::new(None),
        profiles: PMutex::new(profiles.clone()),
    });
    // 起信息服务器（OS 监听 0.0.0.0:scaffold_port，blocking 线程）
    let handlers = build_host_handlers(shared.clone());
    if let Err(e) = scaffold::server::start(handlers, scaffold_port) {
        eprintln!("[online] Scaffolding 信息服务器启动失败: {e}");
    }

    svc.session = Some(s);
    svc.status = OnlineStatus::Waiting;
    svc.room = Some(room);
    svc.scaffold_port = Some(scaffold_port);
    svc.profiles = Some(profiles);
    let snap = snapshot_inner(&svc);
    emit_snapshot(app, &snap);
    drop(svc);

    // 后台发现任务：Waiting → Hosting（游戏端口出现 → 重建实例补白名单）。
    let app_discover = app.clone();
    let shared_discover = shared.clone();
    tauri::async_runtime::spawn(async move {
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
            match (svc.status, mc_port) {
                (OnlineStatus::Waiting, Some(port)) => {
                    // 重建实例：白名单补游戏端口（proxy NAT 直连本机 MC 监听）
                    let Some(old) = svc.session.take() else {
                        return;
                    };
                    old.stop().await;
                    let Some(room) = svc.room.clone() else {
                        return;
                    };
                    let cfg = SessionConfig {
                        role: SessionRole::Host,
                        room,
                        local_port: None,
                        mc_port: Some(port),
                        public_nodes:
                            crate::config::online::OnlineConfig::from_persist().public_nodes,
                        scaffold_port: Some(svc.scaffold_port.unwrap_or(0)),
                    };
                    match EasyTierSession::start(cfg).await {
                        Ok(new_s) => {
                            svc.session = Some(new_s);
                            *shared_discover.mc_port.write() = Some(port);
                            svc.status = OnlineStatus::Hosting;
                        }
                        Err(e) => {
                            eprintln!("[online] 重建联机会话失败: {e}");
                            svc.status = OnlineStatus::Idle;
                            svc.room = None;
                            svc.scaffold_port = None;
                            svc.scanner = None;
                        }
                    }
                    let snap = snapshot_inner(&svc);
                    emit_snapshot(&app_discover, &snap);
                }
                (OnlineStatus::Hosting, None) => {
                    // 游戏端口消失：降回 Waiting（重建实例去游戏端口白名单）
                    let Some(old) = svc.session.take() else {
                        return;
                    };
                    old.stop().await;
                    let Some(room) = svc.room.clone() else {
                        return;
                    };
                    let cfg = SessionConfig {
                        role: SessionRole::Host,
                        room,
                        local_port: None,
                        mc_port: None,
                        public_nodes:
                            crate::config::online::OnlineConfig::from_persist().public_nodes,
                        scaffold_port: Some(svc.scaffold_port.unwrap_or(0)),
                    };
                    match EasyTierSession::start(cfg).await {
                        Ok(new_s) => {
                            svc.session = Some(new_s);
                            *shared_discover.mc_port.write() = None;
                            svc.status = OnlineStatus::Waiting;
                        }
                        Err(e) => {
                            eprintln!("[online] 重建联机会话失败: {e}");
                            svc.status = OnlineStatus::Idle;
                            svc.room = None;
                            svc.scaffold_port = None;
                            svc.scanner = None;
                        }
                    }
                    let snap = snapshot_inner(&svc);
                    emit_snapshot(&app_discover, &snap);
                }
                _ => {}
            }
        }
    });
    // 后台心跳维护任务：剔除超时 guest + 玩家表变化推前端
    {
        let app = app.clone();
        let shared = shared.clone();
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
    Ok(snap)
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

/// 加入联机（作为 guest）：解析房间码 → 建网 → 发现中心 → 心跳 → 协商 →
/// server_port → 建游戏代理（本地 → 中心游戏端口）→ MC 广播。返回本地端口。
pub async fn join(app: &tauri::AppHandle, room_code: &str) -> Result<u16, String> {
    let mut svc = SERVICE.lock().await;
    if svc.session.is_some() {
        return Err("联机已在运行中\n请先停止当前联机会话".to_string());
    }
    let room = parse_room_code(room_code)
        .ok_or_else(|| "房间码无效\n请检查格式（U/XXXX-XXXX-XXXX-XXXX）".to_string())?;
    svc.generation += 1;
    let gen = svc.generation;
    let public_nodes = crate::config::online::OnlineConfig::from_persist().public_nodes;
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

    // 发现中心：轮询 route_snapshots 匹配 hostname 前缀
    let (center_ip, center_port) = match discover_center(&s, gen).await {
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
    let mut stream = match s.data_plane_tcp_connect(center_addr, Duration::from_secs(10)).await {
        Ok(st) => st,
        Err(e) => {
            let mut svc = SERVICE.lock().await;
            rollback_join(&mut svc);
            let snap = snapshot_inner(&svc);
            emit_snapshot(app, &snap);
            return Err(format!("连接联机中心失败\n{e}"));
        }
    };

    // 立即心跳（协商前也要发，协议要求）
    let local_profile = Profile::new(
        online_player_name(),
        scaffold::identity::get_machine_id(),
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
        .map_err(|e| format!("发送心跳失败\n{e}"))?;
    let _ = read_response_async(&mut stream).await;

    // 协议协商：c:protocols
    write_request_async(&mut stream, "c", "protocols", b"")
        .await
        .map_err(|e| format!("协议协商失败\n{e}"))?;
    let protocols_resp = read_response_async(&mut stream)
        .await
        .map_err(|e| format!("协议协商失败\n{e}"))?;
    let supports_easytier_id = protocols_resp.status == 0 && {
        let server = String::from_utf8_lossy(&protocols_resp.body);
        server.split('\0').any(|p| p == "c:player_easytier_id")
    };

    // c:server_port（返回游戏端口；proxy NAT 直连 host 本机 MC 监听）
    write_request_async(&mut stream, "c", "server_port", b"")
        .await
        .map_err(|e| format!("获取服务器端口失败\n{e}"))?;
    let port_resp = read_response_async(&mut stream)
        .await
        .map_err(|e| format!("获取服务器端口失败\n{e}"))?;
    if port_resp.status != 0 || port_resp.body.len() != 2 {
        let mut svc = SERVICE.lock().await;
        rollback_join(&mut svc);
        let snap = snapshot_inner(&svc);
        emit_snapshot(app, &snap);
        return Err("服务器未启动\n请让主机先在游戏里开启局域网世界".to_string());
    }
    let game_port = u16::from_be_bytes([port_resp.body[0], port_resp.body[1]]);
    drop(stream);

    // 建游戏代理：本地 OS 监听 local_port → 数据面连 centerIP:game_port
    let local_port = free_tcp_port()?;
    let proxy_session = s.clone();
    tauri::async_runtime::spawn(async move {
        run_guest_proxy(proxy_session, local_port, center_ip, game_port).await;
    });

    // MC 广播模拟（guest 端局域网标签页看到服务器）
    let advertise_motd = crate::config::online::OnlineConfig::from_persist()
        .advertise_motd
        .filter(|s| !s.trim().is_empty());
    let motd = advertise_motd.unwrap_or_else(|| "Elytra 联机".to_string());
    let advertiser = McAdvertiser::start_with_motd(local_port, &motd);

    // 回写状态
    let mut svc = SERVICE.lock().await;
    svc.advertiser = Some(advertiser);
    svc.session.as_mut().unwrap().local_port = local_port;
    svc.profiles = Some(ProfileTable::with_host(local_profile.clone()));
    let snap = snapshot_inner(&svc);
    emit_snapshot(app, &snap);
    drop(svc);

    // 心跳 + 玩家列表同步任务（每 5s）
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let mut stream = match s
            .data_plane_tcp_connect(center_addr, Duration::from_secs(10))
            .await
        {
            Ok(st) => st,
            Err(_) => return,
        };
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
            if write_request_async(&mut stream, "c", "player_ping", ping.to_string().as_bytes())
                .await
                .is_err()
                || read_response_async(&mut stream).await.is_err()
            {
                return;
            }
            // 拉玩家列表
            if write_request_async(&mut stream, "c", "player_profiles_list", b"")
                .await
                .is_ok()
            {
                let resp = match read_response_async(&mut stream).await {
                    Ok(resp) if resp.status == 0 => resp,
                    _ => continue,
                };
                if let Ok(server_players) = serde_json::from_slice::<Vec<Profile>>(&resp.body) {
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
                    let mut svc = SERVICE.lock().await;
                    if svc.generation != gen {
                        return;
                    }
                    svc.profiles = Some(profiles);
                    let snap = snapshot_inner(&svc);
                    emit_snapshot(&app, &snap);
                }
            }
        }
    });

    Ok(local_port)
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
fn free_tcp_port() -> Result<u16, String> {
    std::net::TcpListener::bind(("127.0.0.1", 0))
        .and_then(|l| l.local_addr())
        .map(|a| a.port())
        .map_err(|e| format!("分配本地端口失败\n{e}"))
}

/// 联机昵称：读配置（OnlineConfig.nickname），空则回退系统用户名。
fn online_player_name() -> String {
    let cfg = crate::config::online::OnlineConfig::from_persist();
    if let Some(name) = cfg.nickname.as_ref().filter(|s| !s.trim().is_empty()) {
        name.clone()
    } else {
        std::env::var("USERNAME")
            .or_else(|_| std::env::var("USER"))
            .unwrap_or_else(|_| "Player".to_string())
    }
}

/// vendor 字符串：`Elytra {版本}`（协议 c:player_ping 的 vendor 字段）。
fn vendor_string() -> String {
    format!("Elytra {}", crate::constants::APP_VERSION)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hostname_prefix_matches() {
        assert!("scaffolding-mc-server-13448".starts_with(SCAFFOLD_HOSTNAME_PREFIX));
        assert!("scaffolding-mc-server-77844".starts_with(SCAFFOLD_HOSTNAME_PREFIX));
        assert!(!"scaffolding-mc-other".starts_with(SCAFFOLD_HOSTNAME_PREFIX));
    }

    #[test]
    fn player_name_fallback() {
        // 无配置 → 回退系统用户名（非空）
        let name = online_player_name();
        assert!(!name.is_empty());
    }

    /// 生产 handler 行为：c:ping 回显 / c:server_port 按游戏端口 / 玩家表维护。
    #[test]
    fn host_handlers_behavior() {
        let shared = Arc::new(HostShared {
            mc_port: parking_lot::RwLock::new(None),
            profiles: PMutex::new(ProfileTable::with_host(Profile::new(
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
            assert!(list.split(' ').any(|x| x == p), "缺协议 {p}");
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
