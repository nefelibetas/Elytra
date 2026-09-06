//! EasyTier 会话封装：库内嵌（非子进程）创建/启动/停止虚拟局域网实例。
//!
//! 关键配置：
//! - **no_tun + use_smoltcp**：纯用户态，无需 wintun 驱动/管理员权限；
//! - **socks5 feature** 必须启用（easytier `gateway_enabled` 由
//!   `cfg!(feature="socks5")` 经 `smoltcp` → `proxy-smoltcp-stack` 传递控制），
//!   否则端口转发不启动；
//! - **bind_device = false**：easytier 默认 true 会枚举本机 IPv4 预绑定源地址，
//!   bind 物理 IP 后 connect 回环/局域网目标在 Windows 报 10049（实测复现）；
//! - **enable_kcp_proxy = true**：port_forward 数据面首选 KCP（PreferKcp），
//!   由该 flag 控制出站（默认 false）；不开启则回退 smoltcp 用户态 TCP，
//!   MC 登录配置同步（registry 数百 KB~数 MB）会中断（实测"连接中断"）；
//! - host 固定虚拟 IP，guest DHCP 拿 IP；
//! - **Scaffolding 模型**：host 用普通 OS `TcpListener` 监听信息端口/游戏端口，
//!   guest 数据面直连 `host虚拟IP:端口`，host 的 easytier proxy NAT
//!   （`tcp_proxy_service`）自动转发到 `127.0.0.1:同端口`——无需 easytier 端口转发；
//! - **入站白名单**（ACL）：host 须在 `tcp_whitelist` 放开信息端口 + 游戏端口，
//!   否则 proxy NAT 拒绝（`should_deny_tcp_proxy`）；
//! - 协调节点：由 OnlineService 显式传入（空 = 不连外部节点，host 自建网 /
//!   guest 直连 host）。

use std::net::{Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use easytier::common::config::{ConfigLoader, TomlConfigLoader};
use easytier::instance::factory::{create_native_instance, NativeCoreInstance};
use easytier_core::config::toml::{NetworkIdentity, PeerConfig};

use super::scaffold::room::RoomCredential;

/// easytier 全局日志初始化锁（其 LOGGER 是进程级 OnceLock，重复 init 报错）。
static EASYTIER_LOG_INIT: std::sync::Once = std::sync::Once::new();

/// 初始化 easytier 文件日志（联机诊断）：抓数据面实际路由（KCP vs smoltcp）、
/// peer 连接/虚拟 IP、连接建立与断开原因，输出到 exe 旁
/// `Elytra/online-easytier.log`（debug 级，10MB × 3 轮转）。
/// 文件懒创建：首条 easytier 日志事件才建文件。
/// 调用幂等（Once）；失败仅打印 stderr，不影响联机主流程。
pub(crate) fn init_easytier_logging() {
    EASYTIER_LOG_INIT.call_once(|| {
        use easytier::common::config::{ConsoleLoggerConfig, FileLoggerConfig, LoggingConfig};
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

/// 会话角色。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionRole {
    Host,
    Guest,
}

/// 路由快照（轻量，避免依赖 easytier 私有 proto 类型）。
#[derive(Debug, Clone)]
pub struct RouteSnapshot {
    pub peer_id: u32,
    pub hostname: String,
    /// 虚拟 IPv4（大端 4 字节；None = 该 peer 未分配 IP）。
    pub ipv4: Option<[u8; 4]>,
}

impl RouteSnapshot {
    /// 虚拟 IPv4 解析为 `Ipv4Addr`。
    pub fn ipv4_addr(&self) -> Option<Ipv4Addr> {
        self.ipv4.map(Ipv4Addr::from)
    }
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
    /// host 信息服务器端口（Scaffolding 协议；设置 hostname `scaffolding-mc-server-{port}`）。
    pub scaffold_port: Option<u16>,
}

/// EasyTier 会话：持有运行中的实例。
#[derive(Clone)]
pub struct EasyTierSession {
    instance: Arc<NativeCoreInstance>,
    pub role: SessionRole,
    /// guest 本地转发端口（Guest 角色，连接成功后暴露给用户）。
    pub local_port: u16,
}

impl EasyTierSession {
    /// 创建并启动实例。
    pub async fn start(config: SessionConfig) -> Result<Self, String> {
        init_easytier_logging();
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

    /// 数据面 TCP 连接（guest 连 Scaffolding 服务器 / 游戏端口用）。
    pub async fn data_plane_tcp_connect(
        &self,
        addr: SocketAddr,
        timeout: Duration,
    ) -> Result<easytier_core::gateway::DataPlaneTcpStream, String> {
        self.instance
            .data_plane_tcp_connect(addr, timeout)
            .await
            .map_err(|e| format!("数据面连接失败\n{e}"))
    }

    /// 数据面 TCP 监听（host 起 Scaffolding 信息服务器用，监听在虚拟 IP 上）。
    pub async fn data_plane_tcp_bind(
        &self,
        port: u16,
        timeout: Duration,
    ) -> Result<easytier_core::gateway::DataPlaneTcpListener, String> {
        self.instance
            .data_plane_tcp_bind(port, timeout)
            .await
            .map_err(|e| format!("数据面监听失败\n{e}"))
    }

    /// 路由快照（guest 发现中心：匹配 hostname 前缀得虚拟 IP + 端口）。
    /// 返回自建轻量结构（避开 easytier proto 私有模块），字段与协议相关部分对应。
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
            })
            .collect()
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
            // 固定虚拟 IP（与协议无关，Elytra 侧方便调试/转发）
            cfg.set_ipv4(Some(
                HOST_VIRTUAL_IP
                    .parse()
                    .map_err(|e| format!("host 虚拟 IP 非法\n{e}"))?,
            ));
            // Scaffolding 中心 hostname：guest 凭它发现中心虚拟 IP + 端口
            if let Some(port) = config.scaffold_port {
                cfg.set_hostname(Some(format!("scaffolding-mc-server-{port}")));
            }
            // 入站白名单：信息端口 + 游戏端口（proxy NAT 直连本机端口；
            // 游戏端口后知 → 由 OnlineService 重建实例时补齐）
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
            // guest 不接收入站（无白名单）；协议/游戏流量都由
            // OnlineService 用数据面直连（data_plane_tcp_connect）。
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

    fn cfg(role: SessionRole, scaffold_port: Option<u16>, mc_port: Option<u16>) -> SessionConfig {
        let room = crate::online::create_room();
        SessionConfig {
            role,
            room,
            local_port: Some(49152),
            mc_port,
            public_nodes: Vec::new(),
            scaffold_port,
        }
    }

    #[test]
    fn host_sets_scaffolding_hostname() {
        // Scaffolding 中心：hostname 必须带协议端口
        let c = cfg(SessionRole::Host, Some(13448), None);
        let toml = build_config(&c).expect("host 建网应成功");
        assert_eq!(
            toml.get_hostname(),
            "scaffolding-mc-server-13448",
            "hostname 应为协议格式"
        );
        // 白名单应含 Scaffolding 端口
        assert!(
            toml.get_tcp_whitelist().contains(&"13448".to_string()),
            "白名单应放开 Scaffolding 端口"
        );
        // 无游戏端口 → 无转发（不依赖 port-forward）
        assert!(toml.get_port_forwards().is_empty());
    }

    #[test]
    fn host_whitelists_all_ports() {
        let c = cfg(SessionRole::Host, Some(13448), Some(25565));
        let toml = build_config(&c).expect("host 建网应成功");
        let wl = toml.get_tcp_whitelist();
        assert!(wl.contains(&"13448".to_string()), "信息端口应入白名单");
        assert!(wl.contains(&"25565".to_string()), "游戏端口应入白名单（proxy NAT 直连）");
        assert!(toml.get_port_forwards().is_empty(), "host 不配转发");
    }

    #[test]
    fn host_no_scaffold_port_no_hostname() {
        // 未设 Scaffolding 端口（异常态）→ 不设 hostname，避免错误暴露
        let c = cfg(SessionRole::Host, None, None);
        let toml = build_config(&c).expect("host 建网应成功");
        assert!(toml.get_hostname().is_empty());
    }

    #[test]
    fn guest_uses_dhcp_no_whitelist() {
        let c = cfg(SessionRole::Guest, None, None);
        let toml = build_config(&c).expect("guest 建网应成功");
        assert!(toml.get_dhcp(), "guest 应 DHCP 拿 IP");
        // guest 不接收入站
        assert!(toml.get_tcp_whitelist().is_empty());
        assert!(toml.get_port_forwards().is_empty());
    }

    #[test]
    fn easytier_logging_inits_once() {
        // 幂等：重复调用不 panic（easytier LOGGER 进程级单例）。
        // 日志文件为懒创建（easytier RollingFileAppender 首条日志事件才建），
        // 此处只验证初始化不报错；落盘由集成测试 online_diag 验证。
        init_easytier_logging();
        init_easytier_logging();
    }
}
