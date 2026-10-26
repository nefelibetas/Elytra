//! EasyTier 会话封装：库内嵌（非子进程）创建/启动/停止虚拟局域网实例；协调节点由 OnlineService 显式传入（空 = 不连外部节点）。
//!
//! 关键配置（硬约束）：no_tun + use_smoltcp 纯用户态（无需 wintun 驱动/管理员权限），且 socks5 feature 必须启用（easytier `gateway_enabled` 由 `cfg!(feature="socks5")` 决定）；
//! bind_device=false：默认 true 会预绑定本机 IPv4，bind 物理 IP 后 connect 回环/局域网在 Windows 报 10049；
//! enable_kcp_proxy=true：port_forward 数据面首选 KCP（PreferKcp），关闭则回退 smoltcp 用户态 TCP，MC 配置同步会中断；
//! host 固定虚拟 IP、guest DHCP 取 IP；host 用 OS `TcpListener` 监听，guest 直连 `host虚拟IP:端口` 经 easytier proxy NAT 转 `127.0.0.1:同端口`，host 须在 `tcp_whitelist` 放开信息端口 + 游戏端口否则被拒。

use std::net::{Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use easytier::common::config::{
    ConfigLoader, ConsoleLoggerConfig, FileLoggerConfig, LoggingConfig, TomlConfigLoader,
};
use easytier::instance::factory::{create_native_instance, NativeCoreInstance};
use easytier_core::config::toml::{NetworkIdentity, PeerConfig};

use super::scaffold::room::RoomCredential;

/// easytier 全局日志初始化锁（其 LOGGER 是进程级 OnceLock，重复 init 报错）。
static EASYTIER_LOG_INIT: std::sync::Once = std::sync::Once::new();

/// 初始化 easytier 文件日志（联机诊断）：记录数据面实际路由、peer/虚拟 IP、连接建立与断开原因，
/// 输出到 exe 旁 `Elytra/online-easytier.log`（debug 级，10MB × 3 轮转）。幂等（Once）；失败仅打印 stderr。
pub(crate) fn init_easytier_logging() {
    EASYTIER_LOG_INIT.call_once(|| {
        let dir = crate::util::config_dir();
        let _ = std::fs::create_dir_all(&dir);
        let cfg = LoggingConfig {
            file_logger: Some(FileLoggerConfig {
                level: Some("debug".into()),
                file: Some("online-easytier.log".into()),
                dir: Some(dir.to_string_lossy().into_owned()),
                size_mb: Some(10),
                count: Some(3),
            }),
            console_logger: Some(ConsoleLoggerConfig {
                level: Some("info".into()),
            }),
        };
        if let Err(e) = easytier::common::log::init(&cfg, false) {
            eprintln!("[online] easytier 日志初始化失败: {e}");
        }
    });
}

/// host 虚拟网固定 IP（私有保留段，避免与真实网络冲突）。
pub const HOST_VIRTUAL_IP: &str = "10.144.144.1";

/// host 监听端口（供 guest/协调节点连接，虚拟网协调用）。
pub const HOST_LISTEN_PORT: u16 = 11010;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionRole {
    Host,
    Guest,
}

/// 路由快照（避开 easytier 私有 proto 类型）。
#[derive(Debug, Clone)]
pub struct RouteSnapshot {
    pub peer_id: u32,
    pub hostname: String,
    /// 虚拟 IPv4（大端 4 字节；None = 未分配）。
    pub ipv4: Option<[u8; 4]>,
    /// 路由跳数：1 = 直连，>1 = 经中继。
    pub cost: i32,
    pub next_hop_peer_id: u32,
    /// 路径延迟（毫秒；<=0 = 未知）。
    pub path_latency: i32,
    /// STUN UDP NAT 类型（proto 枚举值；0 = Unknown）。
    pub udp_nat_type: i32,
}

impl RouteSnapshot {
    pub fn ipv4_addr(&self) -> Option<Ipv4Addr> {
        self.ipv4.map(Ipv4Addr::from)
    }

    /// 直连（打洞或 listener 直连）为 cost 1；经公共节点中继时 cost 更大。
    pub fn is_direct(&self) -> bool {
        self.cost == 1
    }

    pub fn latency_ms(&self) -> Option<i32> {
        (self.path_latency > 0).then_some(self.path_latency)
    }
}

pub struct SessionConfig {
    pub role: SessionRole,
    pub room: RoomCredential,
    /// guest 本地暴露端口（None = 随机分配）。
    pub local_port: Option<u16>,
    /// host 真实 MC 端口（仅本地转发 dst 用，不进房间码）。
    pub mc_port: Option<u16>,
    /// 协调节点列表（空 = 不连外部节点）。
    pub public_nodes: Vec<String>,
    /// host 信息服务器端口（设置 hostname `scaffolding-mc-server-{port}`）。
    pub scaffold_port: Option<u16>,
    /// 是否允许经公共节点中继（false = p2p_only，数据面只走已建立的直连）。
    pub allow_relay: bool,
}

/// EasyTier 会话：持有运行中的实例。
#[derive(Clone)]
pub struct EasyTierSession {
    instance: Arc<NativeCoreInstance>,
    pub role: SessionRole,
    /// guest 本地转发端口（连接成功后暴露给用户）。
    pub local_port: u16,
    /// 本会话创建时的中继策略；配置在会话中修改时以会话值为准（下次建网生效）。
    pub allow_relay: bool,
}

impl EasyTierSession {
    pub async fn start(config: SessionConfig) -> Result<Self, String> {
        init_easytier_logging();
        let toml = build_config(&config)?;
        let instance = create_native_instance(toml).map_err(common_err_ctx_key!(
            "errors.online.failed",
            "创建联机会话失败"
        ))?;
        instance.clone().start().await.map_err(common_err_ctx_key!(
            "errors.online.failed",
            "启动联机会话失败"
        ))?;
        let local_port = config.local_port.unwrap_or(0);
        Ok(Self {
            instance,
            role: config.role,
            local_port,
            allow_relay: config.allow_relay,
        })
    }

    /// 停止会话（实例 drop 自动清理）。
    pub async fn stop(&self) {
        self.instance.stop().await;
    }

    /// 数据面 TCP 连接（guest 连服务器/游戏端口）。
    pub async fn data_plane_tcp_connect(
        &self,
        addr: SocketAddr,
        timeout: Duration,
    ) -> Result<easytier_core::gateway::DataPlaneTcpStream, String> {
        self.instance
            .data_plane_tcp_connect(addr, timeout)
            .await
            .map_err(common_err_ctx_key!(
                "errors.online.failed",
                "数据面连接失败"
            ))
    }

    /// 路由快照（guest 发现中心：匹配 hostname 前缀得虚拟 IP + 端口）。
    pub async fn route_snapshots(&self) -> Vec<RouteSnapshot> {
        self.instance
            .route_snapshots()
            .await
            .into_iter()
            .map(|r| RouteSnapshot {
                peer_id: r.peer_id,
                hostname: r.hostname,
                ipv4: r
                    .ipv4_addr
                    .and_then(|a| a.address)
                    .map(|a| a.addr.to_be_bytes()),
                cost: r.cost,
                next_hop_peer_id: r.next_hop_peer_id,
                path_latency: r.path_latency,
                udp_nat_type: r.stun_info.map(|s| s.udp_nat_type).unwrap_or(0),
            })
            .collect()
    }
}

fn build_config(config: &SessionConfig) -> Result<TomlConfigLoader, String> {
    let cfg = TomlConfigLoader::default();
    cfg.set_network_identity(NetworkIdentity::new(
        config.room.network_name.clone(),
        config.room.network_secret.clone(),
    ));
    set_no_tun(&cfg, config.role, config.allow_relay);

    // 协调节点：空列表 = 不连外部节点（host 自建网 / guest 直连 host）
    let nodes: Vec<String> = config.public_nodes.clone();
    let mut peers = Vec::new();
    for uri_str in nodes {
        let uri = uri_str.parse().map_err(common_err_ctx_key!(
            "errors.online.failed",
            "协调节点地址非法"
        ))?;
        peers.push(PeerConfig {
            uri,
            peer_public_key: None,
        });
    }
    cfg.set_peers(peers);

    // 监听：host 固定端口（供 peer 连入），guest 随机端口（避免冲突）。
    // TCP + UDP 双监听：UDP 打洞成功率远高于 TCP，开放 NAT 主机也能被直接 UDP 连接。
    let listeners: Vec<String> = match config.role {
        SessionRole::Host => vec![
            format!("tcp://0.0.0.0:{HOST_LISTEN_PORT}"),
            format!("udp://0.0.0.0:{HOST_LISTEN_PORT}"),
        ],
        SessionRole::Guest => vec!["tcp://0.0.0.0:0".to_string(), "udp://0.0.0.0:0".to_string()],
    };
    let listeners = listeners
        .iter()
        .map(|s| {
            s.parse()
                .map_err(common_err_ctx_key!("errors.online.failed", "监听地址非法"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    cfg.set_listeners(listeners);

    match config.role {
        SessionRole::Host => {
            cfg.set_ipv4(Some(HOST_VIRTUAL_IP.parse().map_err(
                common_err_ctx_key!("errors.online.failed", "host 虚拟 IP 非法"),
            )?));
            // Scaffolding 中心 hostname：guest 凭它发现中心虚拟 IP + 端口
            if let Some(port) = config.scaffold_port {
                cfg.set_hostname(Some(format!("scaffolding-mc-server-{port}")));
            }
            // 入站白名单：信息端口 + 游戏端口（proxy NAT 直连本机端口；游戏端口后知，由重建实例补齐）
            let mut whitelist = Vec::new();
            if let Some(port) = config.scaffold_port {
                whitelist.push(port.to_string());
            }
            if let Some(port) = config.mc_port {
                whitelist.push(port.to_string());
            }
            if !whitelist.is_empty() {
                cfg.set_tcp_whitelist(whitelist);
            }
        }
        SessionRole::Guest => {
            cfg.set_dhcp(true);
            // guest 不接收入站；协议/游戏流量走数据面直连（data_plane_tcp_connect）
        }
    }
    Ok(cfg)
}

/// no_tun：纯用户态端口转发（无虚拟网卡），数据面靠 smoltcp 用户态协议栈，须同时开 use_smoltcp。
/// P2P 策略：host 声明 need_p2p（所有人优先连它），guest 设 disable_p2p（不与其他 guest 互相打洞）；
/// p2p_only = !allow_relay——数据面只走已建立的直连，打洞失败时宁可失败也不占用公共节点带宽。
fn set_no_tun(cfg: &TomlConfigLoader, role: SessionRole, allow_relay: bool) {
    let mut flags = cfg.get_flags();
    flags.no_tun = true;
    flags.use_smoltcp = true;
    // bind_device 默认 true 会预绑定本机 IPv4：bind 物理 IP 后 connect 回环/局域网在 Windows 报
    // WSAEADDRNOTAVAIL(10049)；关闭后由 OS 自动选源。
    flags.bind_device = false;
    // port_forward 数据面首选 KCP（PreferKcp，由 enable_kcp_proxy 控制，默认 false）；
    // 不开启则回退 smoltcp 用户态 TCP，MC 配置同步（数百 KB~数 MB）会中断。
    flags.enable_kcp_proxy = true;
    flags.need_p2p = role == SessionRole::Host;
    flags.disable_p2p = role == SessionRole::Guest;
    flags.p2p_only = !allow_relay;
    cfg.set_flags(flags);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(role: SessionRole, scaffold_port: Option<u16>, mc_port: Option<u16>) -> SessionConfig {
        let room = crate::online::create_room().unwrap();
        SessionConfig {
            role,
            room,
            local_port: Some(49152),
            mc_port,
            public_nodes: Vec::new(),
            scaffold_port,
            allow_relay: false,
        }
    }

    fn listener_schemes(toml: &TomlConfigLoader) -> Vec<String> {
        toml.get_listener_uris()
            .iter()
            .map(|u| u.scheme().to_string())
            .collect()
    }

    #[test]
    fn host_uses_udp_and_tcp_listeners_and_need_p2p() {
        let toml = build_config(&cfg(SessionRole::Host, Some(13448), None)).expect("建网应成功");
        let schemes = listener_schemes(&toml);
        assert!(schemes.contains(&"tcp".to_string()), "host 应有 TCP 监听");
        assert!(schemes.contains(&"udp".to_string()), "host 应有 UDP 监听");
        let flags = toml.get_flags();
        assert!(flags.need_p2p, "host 应声明 need_p2p");
        assert!(!flags.disable_p2p, "host 不应 disable_p2p");
        assert!(flags.p2p_only, "默认仅直连应为 p2p_only");
    }

    #[test]
    fn guest_disables_p2p_and_uses_udp() {
        let toml = build_config(&cfg(SessionRole::Guest, None, None)).expect("建网应成功");
        let schemes = listener_schemes(&toml);
        assert!(schemes.contains(&"udp".to_string()), "guest 应有 UDP 监听");
        let flags = toml.get_flags();
        assert!(
            flags.disable_p2p,
            "guest 应 disable_p2p（不与其他 guest 打洞）"
        );
        assert!(!flags.need_p2p, "guest 不应声明 need_p2p");
        assert!(flags.p2p_only, "默认仅直连应为 p2p_only");
    }

    #[test]
    fn allow_relay_disables_p2p_only() {
        let mut c = cfg(SessionRole::Guest, None, None);
        c.allow_relay = true;
        let toml = build_config(&c).expect("建网应成功");
        assert!(!toml.get_flags().p2p_only, "允许中继时不得开 p2p_only");
    }

    #[test]
    fn route_directness_and_latency() {
        let mut r = RouteSnapshot {
            peer_id: 1,
            hostname: "h".into(),
            ipv4: Some([10, 0, 0, 1]),
            cost: 1,
            next_hop_peer_id: 2,
            path_latency: 23,
            udp_nat_type: 0,
        };
        assert!(r.is_direct());
        assert_eq!(r.latency_ms(), Some(23));
        r.cost = 3;
        r.path_latency = -1;
        assert!(!r.is_direct());
        assert_eq!(r.latency_ms(), None);
    }

    #[test]
    fn host_sets_scaffolding_hostname() {
        let c = cfg(SessionRole::Host, Some(13448), None);
        let toml = build_config(&c).expect("host 建网应成功");
        assert_eq!(
            toml.get_hostname(),
            "scaffolding-mc-server-13448",
            "hostname 应为协议格式"
        );
        assert!(
            toml.get_tcp_whitelist().contains(&"13448".to_string()),
            "白名单应放开 Scaffolding 端口"
        );
        assert!(toml.get_port_forwards().is_empty());
    }

    #[test]
    fn host_whitelists_all_ports() {
        let c = cfg(SessionRole::Host, Some(13448), Some(25565));
        let toml = build_config(&c).expect("host 建网应成功");
        let wl = toml.get_tcp_whitelist();
        assert!(wl.contains(&"13448".to_string()), "信息端口应入白名单");
        assert!(
            wl.contains(&"25565".to_string()),
            "游戏端口应入白名单（proxy NAT 直连）"
        );
        assert!(toml.get_port_forwards().is_empty(), "host 不配转发");
    }

    #[test]
    fn host_no_scaffold_port_no_hostname() {
        let c = cfg(SessionRole::Host, None, None);
        let toml = build_config(&c).expect("host 建网应成功");
        assert!(toml.get_hostname().is_empty());
    }

    #[test]
    fn guest_uses_dhcp_no_whitelist() {
        let c = cfg(SessionRole::Guest, None, None);
        let toml = build_config(&c).expect("guest 建网应成功");
        assert!(toml.get_dhcp(), "guest 应 DHCP 拿 IP");
        assert!(toml.get_tcp_whitelist().is_empty());
        assert!(toml.get_port_forwards().is_empty());
    }

    #[test]
    fn easytier_logging_inits_once() {
        // 幂等：重复调用不 panic（easytier LOGGER 进程级单例）；
        // 日志文件懒创建，落盘由集成测试 online_diag 验证。
        init_easytier_logging();
        init_easytier_logging();
    }
}
