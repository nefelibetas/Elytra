//! 联机链路诊断（Scaffolding 协议模型）：host 开联机 + 本地 echo 服务器，
//! guest 加入后经虚拟网数据面连接 host。走真实生产代码
//! `EasyTierSession::start`（与 UI 联机同一路径，含 easytier 日志初始化）。
//!
//! 链路（no_tun + smoltcp 全数据面）：
//!   guest data_plane_tcp_connect(host虚拟IP:scaffold_port) → 信息协议
//!   guest data_plane_tcp_connect(host虚拟IP:game_port)  → 游戏流量（proxy NAT）
//!
//! 已知坑（session.rs 文档同款）：
//! - bind_device=false：缺则 Windows 10049 连不上
//! - kcp feature + enable_kcp_proxy：缺则数据面回退 smoltcp，
//!   MC 登录配置同步（registry 大数据）中断
//!
//! 运行：cd src-tauri && cargo test --test online_diag -- --nocapture --test-threads=1

use std::io::{Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener};
use std::time::Duration;

use elytra_lib::online::{codec, create_room, EasyTierSession, SessionConfig, SessionRole};

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
                        let mut buf = [0u8; 65536];
                        loop {
                            match s.read(&mut buf) {
                                Ok(0) | Err(_) => break,
                                Ok(n) => {
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

fn free_port() -> u16 {
    TcpListener::bind(("127.0.0.1", 0))
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn host_guest_scaffolding_roundtrip() {
    let mc_port = free_port();
    let scaffold_port = free_port();
    eprintln!(
        "== 端口：mc={mc_port} scaffold={scaffold_port}"
    );
    spawn_echo_server(mc_port);

    // 同一房间：host 建，guest 解析同一凭据
    let room = create_room().expect("生成房间码");

    // host：无外部 peers（不连公共节点），自建网等待 guest 直连
    let host_cfg = SessionConfig {
        role: SessionRole::Host,
        room: room.clone(),
        local_port: None,
        mc_port: Some(mc_port),
        public_nodes: Vec::new(),
        scaffold_port: Some(scaffold_port),
    };
    let host = EasyTierSession::start(host_cfg).await.expect("host 启动");
    // host 侧起信息服务器（OS 监听 scaffold_port，proxy NAT 直连）——模拟 start_host
    // 协议 handler：c:ping 回显 / c:server_port 返回入口端口 / 玩家表
    {
        let port = scaffold_port;
        let entry = mc_port;
        std::thread::spawn(move || {
            let listener = TcpListener::bind(("0.0.0.0", port)).expect("bind info server");
            eprintln!("== host 信息服务器 OS 监听 0.0.0.0:{port}");
            for stream in listener.incoming() {
                match stream {
                    Ok(mut stream) => {
                        let entry = entry;
                        std::thread::spawn(move || loop {
                            // 帧编解码走生产 codec（协议单一来源）；响应逻辑模拟对端
                            // 实现，不复用生产 handler，保住跨实现互操作验证价值。
                            let req = match codec::read_request(&mut stream) {
                                Ok(req) => req,
                                Err(_) => return,
                            };
                            let resp = match req.kind().as_str() {
                                "c:ping" => codec::Response::ok(req.body),
                                "c:server_port" => {
                                    codec::Response::ok(entry.to_be_bytes().to_vec())
                                }
                                "c:player_ping" => codec::Response::ok(Vec::new()),
                                "c:player_profiles_list" => codec::Response::ok(
                                    br#"[{"name":"host","machine_id":"m0","vendor":"test","kind":"HOST"},{"name":"tester","machine_id":"m1","vendor":"test","kind":"GUEST"}]"#.to_vec(),
                                ),
                                _ => codec::Response::fail(
                                    codec::STATUS_UNKNOWN_ERROR,
                                    b"not implemented".to_vec(),
                                ),
                            };
                            if codec::write_response(&mut stream, &resp).is_err() {
                                return;
                            }
                        });
                    }
                    Err(e) => eprintln!("== host 信息服务器 accept 失败: {e}"),
                }
            }
        });
    }
    // 游戏端口由 echo 服务器直接监听（proxy NAT 直连 127.0.0.1:mc_port），无需 host 代理
    eprintln!("== host 信息服务器已启动（OS 监听，proxy NAT 直连）");

    // guest：直连 host 的监听端口 11010（127.0.0.1 模拟同网直连 host 局域网 IP）
    eprintln!("== host 实例已启动（生产路径）");

    // guest：直连 host 的监听端口 11010（127.0.0.1 模拟同网直连 host 局域网 IP）
    let guest_cfg = SessionConfig {
        role: SessionRole::Guest,
        room: room.clone(),
        local_port: None,
        mc_port: None,
        public_nodes: vec!["tcp://127.0.0.1:11010".to_string()],
        scaffold_port: None,
    };
    let guest = EasyTierSession::start(guest_cfg).await.expect("guest 启动");
    eprintln!("== guest 实例已启动（生产路径）");

    // 等 peer 互发现 + 虚拟 IP 分配
    eprintln!("== 等待 peer 互发现…");
    tokio::time::sleep(Duration::from_secs(8)).await;

    // 1) guest 路由表发现中心（hostname 前缀匹配）
    eprintln!("== 测试 1：guest 发现 Scaffolding 中心");
    let routes = guest.route_snapshots().await;
    let mut center: Option<(Ipv4Addr, u16)> = None;
    for r in &routes {
        eprintln!(
            "   route: peer={} hostname={:?} ip={:?}",
            r.peer_id,
            r.hostname,
            r.ipv4_addr()
        );
        if let Some(port_str) = r.hostname.strip_prefix("scaffolding-mc-server-") {
            if let Ok(port) = port_str.parse::<u16>() {
                if let Some(ip) = r.ipv4_addr() {
                    center = Some((ip, port));
                }
            }
        }
    }
    let (center_ip, center_port) = center.expect("应发现中心（hostname scaffolding-mc-server-*）");
    assert_eq!(center_port, scaffold_port, "中心端口应为 host 的 scaffold_port");
    eprintln!("== 测试 1 OK：中心 = {center_ip}:{center_port}");

    // 2) 信息协议：guest 数据面连中心 scaffold_port → c:ping 回显 + c:server_port
    eprintln!("== 测试 2：信息协议（c:ping + c:server_port）");
    let center_addr = SocketAddr::from((center_ip, center_port));
    let t2 = tokio::time::timeout(Duration::from_secs(20), async {
        // 数据面请求助手：帧读写走生产 codec async API
        async fn req(
            s: &mut (impl tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin),
            kind: (&str, &str),
            body: &[u8],
        ) -> std::io::Result<codec::Response> {
            codec::write_request_async(s, kind.0, kind.1, body).await?;
            codec::read_response_async(s).await
        }
        let mut s = guest
            .data_plane_tcp_connect(center_addr, Duration::from_secs(10))
            .await
            .expect("数据面连中心");
        let resp = req(&mut s, ("c", "ping"), b"probe").await.unwrap();
        assert_eq!(resp.status, 0);
        assert_eq!(resp.body, b"probe", "c:ping 应回显");
        let resp = req(&mut s, ("c", "server_port"), b"").await.unwrap();
        assert_eq!(resp.status, 0, "host 已检测到游戏端口（mc_port 已配）");
        assert_eq!(resp.body.len(), 2);
        let server_port = u16::from_be_bytes([resp.body[0], resp.body[1]]);
        assert_eq!(server_port, mc_port, "c:server_port 应返回游戏端口");
        let resp = req(
            &mut s,
            ("c", "player_ping"),
            br#"{"name":"tester","machine_id":"m1","vendor":"test"}"#,
        )
        .await
        .unwrap();
        assert_eq!(resp.status, 0);
        let resp = req(&mut s, ("c", "player_profiles_list"), b"").await.unwrap();
        assert_eq!(resp.status, 0);
        let profiles: serde_json::Value = serde_json::from_slice(&resp.body).unwrap();
        assert_eq!(profiles.as_array().unwrap().len(), 2, "host + 1 guest");
    })
    .await;
    match t2 {
        Ok(_) => eprintln!("== 测试 2 OK：信息协议全通"),
        Err(e) => eprintln!("== 测试 2 失败: {e}"),
    }

    // 3) 游戏流量：guest 数据面连 centerIP:entry_port（应用层代理 → 本机 echo）
    eprintln!("== 测试 3：guest 数据面 → host 游戏端口 → echo");
    let game_addr = SocketAddr::from((center_ip, mc_port));
    let t3 = tokio::time::timeout(Duration::from_secs(20), async {
        let mut s = guest
            .data_plane_tcp_connect(game_addr, Duration::from_secs(10))
            .await
            .expect("数据面连接 entry");
        // 数据面 stream 实现 AsyncRead/Write：用 tokio 读写
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        s.write_all(b"t3").await.unwrap();
        let mut buf = [0u8; 2];
        s.read_exact(&mut buf).await.unwrap();
        assert_eq!(&buf, b"t3");
    })
    .await;
    match t3 {
        Ok(_) => eprintln!("== 测试 3 OK：guest → 虚拟网 → host 代理 → echo 全链路通"),
        Err(e) => eprintln!("== 测试 3 失败：guest → host 链路不通: {e}"),
    }

    // 4) 大数据传输（模拟 MC 登录配置同步 registry；smoltcp 数据面曾在此中断，
    //    kcp feature + enable_kcp_proxy 后数据面走 KCP 应稳定）
    eprintln!("== 测试 4：guest → host 游戏端口 5MB（模拟 MC registry 同步）");
    let t4 = tokio::time::timeout(Duration::from_secs(60), async {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let mut s = guest
            .data_plane_tcp_connect(game_addr, Duration::from_secs(10))
            .await
            .expect("数据面连接 entry");
        let chunk = vec![0x5au8; 8192];
        let total = 5 * 1024 * 1024;
        let mut written = 0usize;
        while written < total {
            let n = (total - written).min(chunk.len());
            s.write_all(&chunk[..n]).await.unwrap();
            written += n;
        }
        s.flush().await.unwrap();
        let mut read_back = 0usize;
        let mut buf = [0u8; 8192];
        while read_back < total {
            let n = s.read(&mut buf).await.unwrap();
            assert!(n > 0, "大数据传输中断（read 返回 0）");
            assert!(buf[..n].iter().all(|&b| b == 0x5a), "数据损坏");
            read_back += n;
        }
        assert_eq!(read_back, total, "回读字节数不符");
    })
    .await;
    match t4 {
        Ok(_) => eprintln!("== 测试 4 OK：5MB 大数据传输完整（数据面正常）"),
        Err(e) => eprintln!("== 测试 4 失败：大数据传输中断: {e}"),
    }

    // 5) easytier 日志落盘验证（文件懒创建：首条日志事件才建）
    tokio::time::sleep(Duration::from_secs(1)).await;
    let log_path = elytra_lib::util::exe_dir()
        .join(elytra_lib::constants::CONFIG_DIR)
        .join("online-easytier.log");
    eprintln!(
        "== 测试 5：easytier 日志 = {log_path:?} exists={}",
        log_path.exists()
    );
    assert!(
        log_path.exists(),
        "easytier 日志应已生成（host/guest 实例运行过）"
    );

    host.stop().await;
    guest.stop().await;
    eprintln!("== 会话已停止");
}
