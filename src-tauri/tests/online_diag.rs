//! 联机链路诊断（复现：host 开联机 + 本地测试服务器，guest 加入后经
//! 虚拟网 + 端口转发连接 host 本地服务器）。走真实生产代码
//! `EasyTierSession::start`（与 UI 联机同一路径，含 easytier 日志初始化）。
//!
//! 真实场景链路：
//!   guest 游戏 → 127.0.0.1:guest_local_port（guest OS 监听）
//!   → data plane（KCP/smoltcp）→ 虚拟网 10.144.144.1:entry_port
//!   → host OS 监听 0.0.0.0:entry_port → 127.0.0.1:mc_port（本地游戏）
//!
//! 已知坑（session.rs 文档同款）：
//! - bind_device=false：缺则 Windows 10049 连不上
//! - kcp feature + enable_kcp_proxy：缺则数据面回退 smoltcp，
//!   MC 登录配置同步（registry 大数据）中断
//!
//! 运行：cd src-tauri && cargo test --test online_diag -- --nocapture --test-threads=1

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::time::Duration;

use elytra_lib::online::{create_room, EasyTierSession, SessionConfig, SessionRole};

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
async fn host_guest_tcp_roundtrip() {
    let mc_port = free_port();
    let entry_port = free_port();
    let guest_local_port = free_port();
    eprintln!("== 端口：mc={mc_port} entry={entry_port} guest_local={guest_local_port}");
    spawn_echo_server(mc_port);

    // 同一房间：host 建（带入口端口），guest 复用同一凭据
    let room = create_room().with_port(entry_port);

    // host：无外部 peers（不连公共节点），自建网等待 guest 直连
    let host_cfg = SessionConfig {
        role: SessionRole::Host,
        room: room.clone(),
        local_port: None,
        mc_port: Some(mc_port),
        public_nodes: Vec::new(),
    };
    let host = EasyTierSession::start(host_cfg).await.expect("host 启动");
    eprintln!("== host 实例已启动（生产路径）");

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
    let guest_cfg = SessionConfig {
        role: SessionRole::Guest,
        room: room.clone(),
        local_port: Some(guest_local_port),
        mc_port: None,
        public_nodes: vec!["tcp://127.0.0.1:11010".to_string()],
    };
    let guest = EasyTierSession::start(guest_cfg).await.expect("guest 启动");
    eprintln!("== guest 实例已启动（生产路径）");

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

    // 2) guest → host 经虚拟网（小数据）
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

    // 3) 大数据传输（模拟 MC 登录配置同步 registry；smoltcp 数据面曾在此中断，
    //    kcp feature + enable_kcp_proxy 后数据面走 KCP 应稳定）
    eprintln!("== 测试 3：guest → host 传输 5MB（模拟 MC registry 同步）");
    let t3 = tokio::time::timeout(
        Duration::from_secs(60),
        async {
            let mut s = TcpStream::connect(("127.0.0.1", guest_local_port)).unwrap();
            let chunk = vec![0x5au8; 8192];
            let total = 5 * 1024 * 1024;
            let mut written = 0usize;
            while written < total {
                let n = (total - written).min(chunk.len());
                s.write_all(&chunk[..n]).unwrap();
                written += n;
            }
            s.flush().unwrap();
            let mut read_back = 0usize;
            let mut buf = [0u8; 8192];
            while read_back < total {
                let n = s.read(&mut buf).unwrap();
                assert!(n > 0, "大数据传输中断（read 返回 0）");
                assert!(buf[..n].iter().all(|&b| b == 0x5a), "数据损坏");
                read_back += n;
            }
            assert_eq!(read_back, total, "回读字节数不符");
        },
    )
    .await;
    match t3 {
        Ok(_) => eprintln!("== 测试 3 OK：5MB 大数据传输完整（数据面正常）"),
        Err(e) => eprintln!("== 测试 3 失败：大数据传输中断: {e}"),
    }

    // 4) easytier 日志落盘验证（文件懒创建：首条日志事件才建）
    tokio::time::sleep(Duration::from_secs(1)).await;
    let log_path = elytra_lib::util::exe_dir()
        .join(elytra_lib::constants::CONFIG_DIR)
        .join("online-easytier.log");
    eprintln!("== 测试 4：easytier 日志 = {log_path:?} exists={}", log_path.exists());
    assert!(
        log_path.exists(),
        "easytier 日志应已生成（host/guest 实例运行过）"
    );

    host.stop().await;
    guest.stop().await;
    eprintln!("== 会话已停止");
}
