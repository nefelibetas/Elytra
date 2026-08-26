//! EasyTier 会话封装：库内嵌（非子进程）创建/启动/停止虚拟局域网实例。
//!
//! 关键配置：
//! - **no_tun + use_smoltcp**：纯用户态，无需 wintun 驱动/管理员权限；
//! - **socks5 feature** 必须启用（easytier `gateway_enabled` 由
//!   `cfg!(feature="socks5")` 经 `smoltcp` → `proxy-smoltcp-stack` 传递控制），
//!   否则端口转发不启动；
//! - **bind_device = false**：easytier 默认 true 会枚举本机 IPv4 预绑定源地址，
//!   bind 物理 IP 后 connect 回环/局域网目标在 Windows 报 10049（实测复现）；
//! - host 固定虚拟 IP，guest DHCP 拿 IP；
//! - 端口转发方向（spike 验证 + 冲突修正）：
//!   host 转发 `bind 0.0.0.0:入口端口 → dst 127.0.0.1:mc_port`；
//!   guest 转发 `bind 127.0.0.1:本地端口 → dst host虚拟IP:入口端口`。
//!   入口端口是 host 探测的空闲端口（≠游戏端口），避免与游戏自身监听冲突
//!   （真实 MC"对局域网开放"会 bind 0.0.0.0:mc_port，若 easytier 也 bind 同端口
//!   即 10013/WSAEACCES）。入口端口编入房间码，guest 据此连接。
//! - 协调节点：easytier 官方公共节点（public.easytier.top / public2.easytier.cn）
//!   已于 2026 年下线（DNS NXDOMAIN，实测），不再作为默认值；由 OnlineService
//!   显式传入（空 = 不连外部节点，host 自建网 / guest 直连 host）。

use std::net::{Ipv4Addr, SocketAddr};
use std::sync::Arc;

use easytier::common::config::{ConfigLoader, TomlConfigLoader};
use easytier::instance::factory::{create_native_instance, NativeCoreInstance};
use easytier_core::config::gateway::PortForwardConfig;
use easytier_core::config::toml::{NetworkIdentity, PeerConfig};

use super::room::RoomCredential;

/// host 虚拟网固定 IP（与陶瓦同网段，避免真实网络冲突）。
pub const HOST_VIRTUAL_IP: &str = "10.144.144.1";

/// host 虚拟网固定 IP 解析（session 转发 dst 用；[Ipv4Addr;4]）。
const HOST_VIRTUAL_IP_V4: Ipv4Addr = Ipv4Addr::new(10, 144, 144, 1);

/// host 监听端口（供 guest/协调节点连接，虚拟网协调用）。
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
    /// host 侧游戏端口（真实 MC 端口，仅 host 本地转发 dst 用，不进房间码）。
    pub mc_port: Option<u16>,
    /// 协调节点列表（host/guest 连同一批节点虚拟网内互发现；空 = 不连外部节点）。
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
    /// 创建并启动实例。
    pub async fn start(config: SessionConfig) -> Result<Self, String> {
        let toml = build_config(&config)?;
        let instance =
            create_native_instance(toml).map_err(|e| format!("创建联机会话失败\n{e}"))?;
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

    // 协调节点：host/guest 连同一批节点，虚拟网内互发现。
    // 空列表 = 不连任何外部节点（host 自建网 / guest 直连 host）。
    let nodes: Vec<String> = config.public_nodes.clone();
    let mut peers = Vec::new();
    for uri_str in nodes {
        let uri = uri_str
            .parse()
            .map_err(|e| format!("协调节点地址非法\n{e}"))?;
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
    cfg.set_listeners(vec![listen
        .parse()
        .map_err(|e| format!("监听地址非法\n{e}"))?]);

    match config.role {
        SessionRole::Host => {
            cfg.set_ipv4(Some(
                HOST_VIRTUAL_IP
                    .parse()
                    .map_err(|e| format!("host 虚拟 IP 非法\n{e}"))?,
            ));
            // Waiting 阶段 mc_port 尚无 → 只建网不配转发；
            // 检测到游戏端口后重建实例：入口端口 + 游戏端口齐备才配转发。
            if let (Some(entry_port), Some(mc_port)) = (config.room.port, config.mc_port) {
                // 虚拟网内 guest 访问 host虚拟IP:入口端口 → 本地游戏 127.0.0.1:mc_port
                cfg.set_port_forwards(vec![PortForwardConfig {
                    bind_addr: SocketAddr::from((Ipv4Addr::UNSPECIFIED, entry_port)),
                    dst_addr: SocketAddr::from((Ipv4Addr::LOCALHOST, mc_port)),
                    proto: "tcp".to_string(),
                }]);
            }
        }
        SessionRole::Guest => {
            cfg.set_dhcp(true);
            // 本地端口 → host 虚拟 IP 的入口端口（host 转本地游戏）
            let entry_port = config.room.port.ok_or("房间码缺少入口端口")?;
            let local_port = config.local_port.unwrap_or(0);
            cfg.set_port_forwards(vec![PortForwardConfig {
                bind_addr: SocketAddr::from((Ipv4Addr::LOCALHOST, local_port)),
                dst_addr: SocketAddr::from((HOST_VIRTUAL_IP_V4, entry_port)),
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
    // bind_device 默认 true：easytier 会枚举本机 IPv4 预绑定源地址再 connect。
    // bind 物理 IP 后 connect 127.0.0.1（回环）或部分局域网目标时，
    // Windows 报 WSAEADDRNOTAVAIL(10049) 导致连不上（实测复现）。
    // 关闭后由 OS 自动选源，回环/局域网都正常。
    flags.bind_device = false;
    // KCP 数据面出站：port_forward 数据面首选 KCP（PreferKcp），
    // 由 enable_kcp_proxy 控制（默认 false）；不开启则回退 smoltcp 用户态 TCP，
    // MC 登录配置同步（registry 数百 KB~数 MB）会中断（实测"连接中断"）。
    flags.enable_kcp_proxy = true;
    cfg.set_flags(flags);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(role: SessionRole, port: Option<u16>, mc_port: Option<u16>) -> SessionConfig {
        let room = match port {
            Some(p) => crate::online::create_room().with_port(p),
            None => crate::online::create_room(),
        };
        SessionConfig {
            role,
            room,
            local_port: Some(49152),
            mc_port,
            public_nodes: Vec::new(),
        }
    }

    #[test]
    fn host_waiting_no_mc_port_builds_without_error() {
        // Waiting 阶段：入口端口已编入、游戏端口未知 → 应能建网（无转发）。
        let c = cfg(SessionRole::Host, Some(40000), None);
        let toml = build_config(&c).expect("等待游戏端口时 host 建网不应失败");
        assert!(
            toml.get_port_forwards().is_empty(),
            "Waiting 阶段不应配转发"
        );
    }

    #[test]
    fn host_hosting_builds_forward_to_local_game() {
        // Hosting 阶段：入口端口 + 游戏端口齐备 → 转发到本地游戏端口。
        let c = cfg(SessionRole::Host, Some(40000), Some(25565));
        let toml = build_config(&c).expect("host 转发建网应成功");
        let fwds = toml.get_port_forwards();
        assert_eq!(fwds.len(), 1, "Hosting 阶段应配一条转发");
        assert_eq!(
            fwds[0].bind_addr,
            SocketAddr::from((Ipv4Addr::UNSPECIFIED, 40000))
        );
        assert_eq!(
            fwds[0].dst_addr,
            SocketAddr::from((Ipv4Addr::LOCALHOST, 25565))
        );
    }

    #[test]
    fn guest_forwards_to_host_virtual_ip_entry_port() {
        let c = cfg(SessionRole::Guest, Some(40000), None);
        let toml = build_config(&c).expect("guest 建网应成功");
        let fwds = toml.get_port_forwards();
        assert_eq!(fwds.len(), 1);
        assert_eq!(
            fwds[0].bind_addr,
            SocketAddr::from((Ipv4Addr::LOCALHOST, 49152))
        );
        assert_eq!(
            fwds[0].dst_addr,
            SocketAddr::from((HOST_VIRTUAL_IP_V4, 40000))
        );
    }
}
