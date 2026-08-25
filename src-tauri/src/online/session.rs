//! EasyTier 会话封装：库内嵌（非子进程）创建/启动/停止虚拟局域网实例。
//!
//! 关键配置（spike 验证的配方）：
//! - **no_tun + use_smoltcp**：纯用户态，无需 wintun 驱动/管理员权限；
//! - **socks5 feature** 必须启用（easytier `gateway_enabled = cfg!(feature="socks5")`，
//!   否则端口转发不启动）；
//! - host 固定虚拟 IP，guest DHCP 拿 IP；
//! - 双方连同一公共协调节点（`-p tcp://node.invalid:11010`），虚拟网内互发现，
//!   无需知道对方真实 IP；
//! - 端口转发：host `0.0.0.0:mc_port → 127.0.0.1:mc_port`（虚拟网流量桥到本地游戏），
//!   guest `127.0.0.1:local_port → host虚拟IP:mc_port`（本地可连）。

use std::net::{Ipv4Addr, SocketAddr};
use std::sync::Arc;

use easytier::common::config::{ConfigLoader, TomlConfigLoader};
use easytier::instance::factory::{create_native_instance, NativeCoreInstance};
use easytier_core::config::gateway::PortForwardConfig;
use easytier_core::config::toml::{NetworkIdentity, PeerConfig};

use super::room::RoomCredential;

/// 免费公共协调节点（EasyTier 社区提供，打洞失败时中继兜底）。
pub const DEFAULT_PUBLIC_NODES: &[&str] = &[
    "tcp://node.invalid:11010",
    "tcp://node.invalid:54321",
];

/// host 虚拟网固定 IP（与陶瓦同网段，避免真实网络冲突）。
pub const HOST_VIRTUAL_IP: &str = "10.144.144.1";

/// host 监听端口（供 guest/公共节点连接，虚拟网协调用）。
pub const HOST_LISTEN_PORT: u16 = 11010;

/// 会话角色。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionRole {
    Host,
    Guest,
}

/// 会话配置（由 OnlineService 组装）。
pub struct SessionConfig {
    pub role: SessionRole,
    pub room: RoomCredential,
    /// guest 本地暴露端口（Guest 角色用；None 则随机分配）。
    pub local_port: Option<u16>,
    /// 公共协调节点列表（可覆盖默认）。
    pub public_nodes: Vec<String>,
}

/// EasyTier 会话：持有运行中的实例。
pub struct EasyTierSession {
    instance: Arc<NativeCoreInstance>,
    pub role: SessionRole,
    /// guest 本地转发端口（Guest 角色，连接成功后暴露给用户）。
    pub local_port: u16,
}

impl EasyTierSession {
    /// 创建并启动会话。
    pub async fn start(config: SessionConfig) -> Result<Self, String> {
        let toml = build_config(&config)?;
        let instance = create_native_instance(toml).map_err(|e| format!("创建联机会话失败\n{e}"))?;
        instance
            .clone()
            .start()
            .await
            .map_err(|e| format!("启动联机会话失败\n{e}"))?;
        let local_port = config.local_port.unwrap_or(0);
        Ok(Self {
            instance,
            role: config.role,
            local_port,
        })
    }

    /// 停止会话（实例 drop 时自动清理）。
    pub async fn stop(&self) {
        self.instance.stop().await;
    }
}

/// 构建 easytier TOML 配置。
fn build_config(config: &SessionConfig) -> Result<TomlConfigLoader, String> {
    let cfg = TomlConfigLoader::default();
    cfg.set_network_identity(NetworkIdentity::new(
        config.room.network_name.clone(),
        config.room.network_secret.clone(),
    ));
    set_no_tun(&cfg);

    // 公共协调节点：host/guest 连同一批节点，虚拟网内互发现
    let nodes: Vec<String> = if config.public_nodes.is_empty() {
        DEFAULT_PUBLIC_NODES.iter().map(|s| s.to_string()).collect()
    } else {
        config.public_nodes.clone()
    };
    let mut peers = Vec::new();
    for uri_str in nodes {
        let uri = uri_str
            .parse()
            .map_err(|e| format!("公共节点地址非法\n{e}"))?;
        peers.push(PeerConfig {
            uri,
            peer_public_key: None,
        });
    }
    cfg.set_peers(peers);

    // 监听：host 固定端口（供 peer 连入），guest 随机端口（避免冲突）
    let listen = match config.role {
        SessionRole::Host => format!("tcp://0.0.0.0:{HOST_LISTEN_PORT}"),
        SessionRole::Guest => "tcp://0.0.0.0:0".to_string(),
    };
    cfg.set_listeners(vec![listen.parse().map_err(|e| format!("监听地址非法\n{e}"))?]);

    match config.role {
        SessionRole::Host => {
            cfg.set_ipv4(Some(
                HOST_VIRTUAL_IP
                    .parse()
                    .map_err(|e| format!("host 虚拟 IP 非法\n{e}"))?,
            ));
            // 端口可选：等待游戏"对局域网开放"期间先建网不转发，
            // 检测到端口后由上层重建实例补转发（网名密钥不变，peer 自动重连）。
            if let Some(mc_port) = config.room.port {
                // 虚拟网内收到的游戏流量 → 本地游戏服务器
                cfg.set_port_forwards(vec![PortForwardConfig {
                    bind_addr: SocketAddr::from((Ipv4Addr::UNSPECIFIED, mc_port)),
                    dst_addr: SocketAddr::from((Ipv4Addr::LOCALHOST, mc_port)),
                    proto: "tcp".to_string(),
                }]);
            }
        }
        SessionRole::Guest => {
            cfg.set_dhcp(true);
            // 本地端口 → host 虚拟 IP 的游戏端口
            let mc_port = config.room.port.ok_or("房间码缺少游戏端口")?;
            let local_port = config.local_port.unwrap_or(0);
            cfg.set_port_forwards(vec![PortForwardConfig {
                bind_addr: SocketAddr::from((Ipv4Addr::LOCALHOST, local_port)),
                dst_addr: SocketAddr::from(([10, 144, 144, 1], mc_port)),
                proto: "tcp".to_string(),
            }]);
        }
    }
    Ok(cfg)
}

/// 设 no_tun：Windows 无管理员/无 wintun 驱动时走纯用户态端口转发（无虚拟网卡）。
/// no_tun 数据面靠 smoltcp 用户态协议栈，须同时开 use_smoltcp。
fn set_no_tun(cfg: &TomlConfigLoader) {
    let mut flags = cfg.get_flags();
    flags.no_tun = true;
    flags.use_smoltcp = true;
    cfg.set_flags(flags);
}