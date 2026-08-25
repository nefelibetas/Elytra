//! 联机领域层：EasyTier 库内嵌虚拟局域网 + 端口转发（host/guest）。
//!
//! host 流程（顺序自由）：
//! 1. `start_host` → 探测入口端口 → 先建虚拟网（无转发），状态 `Waiting`，后台组播轮询；
//! 2. 游戏"对局域网开放"→ 扫描器拿到游戏端口 → 重建实例补转发 → `Hosting`，
//!    房间码补全入口端口段，可分享。
//!
//! guest：`join(room_code)` → 解析房间码拿 host 入口端口 → 加入虚拟网 + 本地转发。
//!
//! 端口设计（10013 冲突修正）：真实 MC"对局域网开放"会 bind `0.0.0.0:mc_port`，
//! easytier 转发若 bind 同端口即 WSAEACCES。故 host 转发 bind **入口端口**
//! （探测的空闲端口，≠游戏端口，编入房间码）→ dst 本地游戏端口；
//! guest 转发 dst 为 host 虚拟 IP 的入口端口。
//!
//! 事件：经 `EMIT_TX` 队列派发 `online-state`（载荷 [`OnlineSnapshot`]）。

mod mc_discover;
mod room;
mod session;

pub use room::{create_room, parse_room_code, RoomCredential};
pub use session::EasyTierSession;

use std::sync::LazyLock;
use std::time::Duration;
use tokio::sync::Mutex;

use crate::events::ONLINE_STATE;
use crate::task::emit_event;

use self::mc_discover::MinecraftScanner;
use self::session::{SessionConfig, SessionRole};

/// 端口发现轮询间隔（游戏每 1.5s 广播一次组播，1s 轮询足够快）。
const DISCOVER_POLL: Duration = Duration::from_secs(1);

/// 联机状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OnlineStatus {
    /// 未联机。
    Idle,
    /// host 虚拟网已建、等待游戏开放局域网（未拿到游戏端口）。
    Waiting,
    /// host 联机中（已检测到游戏端口，转发生效）。
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
    /// guest 本地转发端口（游戏内填 127.0.0.1:<port>）；非 guest 为 None。
    pub local_port: Option<u16>,
}

/// OnlineService 单例（对齐 TaskManager：进程内唯一，模块级 LazyLock）。
struct OnlineServiceInner {
    session: Option<EasyTierSession>,
    status: OnlineStatus,
    room: Option<RoomCredential>,
    /// 组播扫描器（host 用；drop 即停线程）。
    scanner: Option<MinecraftScanner>,
    /// 会话代际：stop/start_host 递增，旧的后台发现任务据此自杀。
    generation: u64,
}

impl Default for OnlineServiceInner {
    fn default() -> Self {
        Self {
            session: None,
            status: OnlineStatus::Idle,
            room: None,
            scanner: None,
            generation: 0,
        }
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
    }
}

fn emit_snapshot(app: &tauri::AppHandle, snap: &OnlineSnapshot) {
    emit_event(app, ONLINE_STATE, snap.clone());
}

/// 开启联机（作为 host）：探测入口端口 → 立即建虚拟网进入 `Waiting`，
/// 后台等游戏"对局域网开放"的组播广播，检测到游戏端口后自动补转发并补全房间码。
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
    // 入口端口：全程不变的空闲端口（≠游戏端口，避免与游戏监听冲突）。
    // 一开始就编入房间码；Waiting 阶段先建网（无转发），
    // 检测到游戏端口后重建实例补转发（bind 此端口 → 本地游戏）。
    let entry_port = free_tcp_port()?;
    let room = create_room().with_port(entry_port);
    let cfg = SessionConfig {
        role: SessionRole::Host,
        room: room.clone(),
        local_port: None,
        mc_port: None,
        public_nodes: Vec::new(),
    };
    let s = EasyTierSession::start(cfg).await?;
    svc.session = Some(s);
    svc.status = OnlineStatus::Waiting;
    svc.room = Some(room);
    let snap = snapshot_inner(&svc);
    emit_snapshot(app, &snap);
    drop(svc);

    // 后台发现任务：拿到游戏端口 → 重建实例补转发（网名密钥不变，peer 自动重连）
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(DISCOVER_POLL).await;
            let mut svc = SERVICE.lock().await;
            // 已停止/重开/状态被改：本任务过期退出
            if svc.generation != gen || svc.status != OnlineStatus::Waiting {
                return;
            }
            let Some(mc_port) = svc.scanner.as_ref().and_then(|sc| sc.get_port()) else {
                continue;
            };
            // room 在 start_host 时已带入口端口，直接复用（身份不变）
            let Some(room) = svc.room.clone() else {
                return;
            };
            if let Some(old) = svc.session.take() {
                old.stop().await;
            }
            let cfg = SessionConfig {
                role: SessionRole::Host,
                room: room.clone(),
                local_port: None,
                mc_port: Some(mc_port),
                public_nodes: Vec::new(),
            };
            match EasyTierSession::start(cfg).await {
                Ok(s) => {
                    svc.session = Some(s);
                    svc.status = OnlineStatus::Hosting;
                    svc.room = Some(room);
                }
                Err(e) => {
                    // 重建失败：回退空闲态并释放扫描器，错误经快照无法携带——
                    // 记日志即可；用户可重试
                    eprintln!("联机端口转发重建失败: {e}");
                    svc.status = OnlineStatus::Idle;
                    svc.room = None;
                    svc.scanner = None;
                }
            }
            let snap = snapshot_inner(&svc);
            emit_snapshot(&app, &snap);
            return;
        }
    });
    Ok(snap)
}

/// 加入联机（作为 guest）：解析房间码（含 host 入口端口）→ 加入虚拟网 → 本地端口转发。
pub async fn join(app: &tauri::AppHandle, room_code: &str) -> Result<u16, String> {
    let mut svc = SERVICE.lock().await;
    if svc.session.is_some() {
        return Err("联机已在运行中\n请先停止当前联机会话".to_string());
    }
    let room = parse_room_code(room_code)
        .ok_or_else(|| "房间码无效\n请检查格式（U/XXXX-XXXX-XXXX-XXXX:端口）".to_string())?;
    room.port.ok_or_else(|| {
        "房间码缺少端口\n请让主机重新复制完整房间码（需在检测到游戏后生成）".to_string()
    })?;
    // 本地转发端口：预先占一个空闲端口再交给转发配置（bind :0 实际端口不可查）
    let local_port = free_tcp_port()?;
    svc.generation += 1;
    let cfg = SessionConfig {
        role: SessionRole::Guest,
        room: room.clone(),
        local_port: Some(local_port),
        mc_port: None,
        public_nodes: Vec::new(),
    };
    let s = EasyTierSession::start(cfg).await?;
    svc.session = Some(s);
    svc.status = OnlineStatus::Joining;
    svc.room = Some(room);
    let snap = snapshot_inner(&svc);
    emit_snapshot(app, &snap);
    Ok(local_port)
}

/// 停止联机（host/guest 通用）：递增代际使后台任务退出。
pub async fn stop(app: &tauri::AppHandle) -> Result<(), String> {
    let mut svc = SERVICE.lock().await;
    svc.generation += 1;
    if let Some(s) = svc.session.take() {
        s.stop().await;
    }
    svc.scanner = None; // Drop → 组播线程退出
    svc.status = OnlineStatus::Idle;
    svc.room = None;
    let snap = snapshot_inner(&svc);
    emit_snapshot(app, &snap);
    Ok(())
}

/// 当前状态快照（纯读）。
pub async fn status() -> OnlineSnapshot {
    let svc = SERVICE.lock().await;
    snapshot_inner(&svc)
}

/// 探测一个空闲 TCP 端口并释放（预留给端口转发绑定）。
fn free_tcp_port() -> Result<u16, String> {
    std::net::TcpListener::bind(("127.0.0.1", 0))
        .and_then(|l| l.local_addr())
        .map(|a| a.port())
        .map_err(|e| format!("分配本地端口失败\n{e}"))
}
