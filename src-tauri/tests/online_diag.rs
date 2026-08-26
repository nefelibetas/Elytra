//! 联机链路诊断（复现：host 开联机 + 本地测试服务器，guest 加入后经
//! 虚拟网 + 端口转发连接 host 本地服务器）。
//!
//! 真实场景链路：
//!   guest 游戏 → 127.0.0.1:guest_local_port（guest OS 监听）
//!   → data plane smoltcp → 虚拟网 10.144.144.1:entry_port
//!   → host OS 监听 0.0.0.0:entry_port → 127.0.0.1:mc_port（本地游戏）
//!
//! 已知坑：flags.bind_device 默认 true 会枚举本机 IPv4 预绑定源地址，
//! bind 物理 IP 后 connect 127.0.0.1(回环) → Windows WSAEADDRNOTAVAIL(10049)。
//! 必须 bind_device=false（由 OS 自动选源）。
//!
//! 运行：cd src-tauri && cargo test --test online_diag -- --nocapture --test-threads=1

use std::io::{Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::time::Duration;

use easytier::common::config::{ConfigLoader, TomlConfigLoader};
use easytier::instance::factory::{create_native_instance, subscribe_native_instance_event};
use easytier_core::config::gateway::PortForwardConfig;
use easytier_core::config::toml::{NetworkIdentity, PeerConfig};

const HOST_VIRTUAL_IP: &str = "10.144.144.1";
const HOST_LISTEN_PORT: u16 = 11010;

fn free_port() -> u16 {
    TcpListener::bind(("127.0.0.1", 0))
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

/// 复制 session.rs build_config 逻辑（诊断用）。
fn build_config(
    network_name: &str,
    network_secret: &str,
    role: &str,
    entry_port: u16,
    local_port: Option<u16>,
    mc_port: Option<u16>,
    peers: &[&str],
) -> TomlConfigLoader {
    let cfg = TomlConfigLoader::default();
    cfg.set_network_identity(NetworkIdentity::new(
        network_name.to_string(),
        network_secret.to_string(),
    ));
    let mut flags = cfg.get_flags();
    flags.no_tun = true;
    flags.use_smoltcp = true;
    // bind_device=true 会枚举本机 IPv4 预绑定源地址：
    // bind 物理 IP 后 connect 127.0.0.1(回环) → Windows WSAEADDRNOTAVAIL(10049)。
    // 关闭后由 OS 自动选源，回环/局域网都正常。
    flags.bind_device = false;
    cfg.set_flags(flags);

    let peers: Vec<PeerConfig> = peers
        .iter()
        .map(|s| PeerConfig {
            uri: s.parse().unwrap(),
            peer_public_key: None,
        })
        .collect();
    cfg.set_peers(peers);

    let listen = if role == "host" {
        format!("tcp://0.0.0.0:{HOST_LISTEN_PORT}")
    } else {
        "tcp://0.0.0.0:0".to_string()
    };
    cfg.set_listeners(vec![listen.parse().unwrap()]);

    match role {
        "host" => {
            cfg.set_ipv4(Some(HOST_VIRTUAL_IP.parse().unwrap()));
            if let Some(mc) = mc_port {
                cfg.set_port_forwards(vec![PortForwardConfig {
                    bind_addr: SocketAddr::from((Ipv4Addr::UNSPECIFIED, entry_port)),
                    dst_addr: SocketAddr::from((Ipv4Addr::LOCALHOST, mc)),
                    proto: "tcp".to_string(),
                }]);
            }
        }
        _ => {
            cfg.set_dhcp(true);
            cfg.set_port_forwards(vec![PortForwardConfig {
                bind_addr: SocketAddr::from((Ipv4Addr::LOCALHOST, local_port.unwrap())),
                dst_addr: SocketAddr::from((
                    HOST_VIRTUAL_IP.parse::<Ipv4Addr>().unwrap(),
                    entry_port,
                )),
                proto: "tcp".to_string(),
            }]);
        }
    }
    cfg
}

/// 本地 echo 服务器：接受连接、回显收到的字节（模拟 MC 服务器）。
fn spawn_echo_server(mc_port: u16) {
    std::thread::spawn(move || {
        let listener = TcpListener::bind(("127.0.0.1", mc_port)).expect("bind echo server");
        eprintln!("[echo] listening 127.0.0.1:{mc_port}");
        for stream in listener.incoming() {
            match stream {
                Ok(mut s) => {
                    eprintln!("[echo] accepted {s:?}");
                    std::thread::spawn(move || {
                        let mut buf = [0u8; 1024];
                        loop {
                            match s.read(&mut buf) {
                                Ok(0) | Err(_) => break,
                                Ok(n) => {
                                    eprintln!("[echo] recv {} bytes, echo back", n);
                                    if s.write_all(&buf[..n]).is_err() {
                                        break;
                                    }
                                }
                            }
                        }
                    });
                }
                Err(e) => eprintln!("[echo] accept error: {e}"),
            }
        }
    });
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn host_guest_tcp_roundtrip() {
    let mc_port = free_port();
    let entry_port = free_port();
    let guest_local_port = free_port();
    eprintln!("== 端口：mc={mc_port} entry={entry_port} guest_local={guest_local_port}");
    spawn_echo_server(mc_port);

    // 网络凭据：固定串（诊断用，绕过 room 派生）
    let (net_name, net_secret) = ("diag-net-v1", "diag-secret-v1");

    // host：无外部 peers（不连公共节点），自建网等待 guest 直连
    let host_cfg = build_config(net_name, net_secret, "host", entry_port, None, Some(mc_port), &[]);
    let host = create_native_instance(host_cfg).expect("host 实例创建");
    if let Some(mut sub) = subscribe_native_instance_event(&host) {
        tokio::spawn(async move {
            while let Ok(ev) = sub.recv().await {
                eprintln!("[host-event] {ev:?}");
            }
        });
    }
    host.clone().start().await.expect("host 启动");
    eprintln!("== host 实例已启动");

    // 裸 std TCP 探测：host 的 11010 listener 是否真的在 OS 层监听
    tokio::time::sleep(Duration::from_secs(2)).await;
    eprintln!("== 探测 0：裸 connect 127.0.0.1:11010");
    match std::net::TcpStream::connect_timeout(
        &"127.0.0.1:11010".parse().unwrap(),
        Duration::from_secs(2),
    ) {
        Ok(_) => eprintln!("== 探测 0 OK：host 11010 OS 监听可达"),
        Err(e) => eprintln!("== 探测 0 失败：host 11010 不可达: {e}"),
    }

    // guest：直连 host 的监听端口 11010（127.0.0.1 模拟同网直连 host 局域网 IP）
    let guest_cfg = build_config(
        net_name,
        net_secret,
        "guest",
        entry_port,
        Some(guest_local_port),
        None,
        &["tcp://127.0.0.1:11010"],
    );
    let guest = create_native_instance(guest_cfg).expect("guest 实例创建");
    if let Some(mut sub) = subscribe_native_instance_event(&guest) {
        tokio::spawn(async move {
            while let Ok(ev) = sub.recv().await {
                eprintln!("[guest-event] {ev:?}");
            }
        });
    }
    guest.clone().start().await.expect("guest 启动");
    eprintln!("== guest 实例已启动");

    // 等 peer 互发现 + 虚拟 IP 分配
    eprintln!("== 等待 peer 互发现…");
    tokio::time::sleep(Duration::from_secs(8)).await;

    // 1) host 本地直连测试：OS 监听入口端口 → 转发到本地 echo
    eprintln!("== 测试 1：host 本机直连 127.0.0.1:{entry_port}");
    let t1 = tokio::time::timeout(
        Duration::from_secs(5),
        async {
            let mut s = TcpStream::connect(("127.0.0.1", entry_port)).unwrap();
            s.write_all(b"t1").unwrap();
            let mut buf = [0u8; 2];
            s.read_exact(&mut buf).unwrap();
            assert_eq!(&buf, b"t1");
        },
    )
    .await;
    match t1 {
        Ok(_) => eprintln!("== 测试 1 OK：host 转发链路通"),
        Err(_) => eprintln!("== 测试 1 失败：host 转发链路不通（入口端口未监听或转发失败）"),
    }

    // 2) guest → host 经虚拟网
    eprintln!("== 测试 2：guest 连 127.0.0.1:{guest_local_port}");
    let t2 = tokio::time::timeout(
        Duration::from_secs(20),
        async {
            let mut s = TcpStream::connect(("127.0.0.1", guest_local_port)).unwrap();
            s.write_all(b"t2").unwrap();
            let mut buf = [0u8; 2];
            s.read_exact(&mut buf).unwrap();
            assert_eq!(&buf, b"t2");
        },
    )
    .await;
    match t2 {
        Ok(_) => eprintln!("== 测试 2 OK：guest → 虚拟网 → host → echo 全链路通"),
        Err(_) => eprintln!("== 测试 2 失败：guest → host 虚拟网链路不通"),
    }

    host.stop().await;
    guest.stop().await;
    eprintln!("== 会话已停止");
}
