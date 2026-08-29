//! MC 局域网服务器广播模拟（guest 端）。
//!
//! 背景：MC 26.2 客户端对"手动输入 IP"的连接做会话校验，离线账号会报
//! "登录失败：无效会话"；而"局域网发现"路径（多人游戏 → 局域网标签页）
//! 跳过会话校验，离线可正常进入。Elytra 联机（guest 连本地转发端口）
//! 属于前者，故登录失败。
//!
//! 修复：guest 加入联机后，在本机周期性地向 MC 局域网发现组播
//! （224.0.2.60:4445）广播 `[MOTD]<名字>[/MOTD][AD]<本地端口>[/AD]`，
//! 使 guest 的 MC 客户端在"局域网"标签页看到该服务器（本机组播回环），
//! 点击进入走局域网发现路径，跳过会话校验，经 easytier 转发到 host 正常登录。
//! 服务器名取房间码携带的 host 存档名（MOTD），无则回退固定文案。
//!
//! 广播 socket bind 0.0.0.0 → 源 IP 为 OS 选的主网卡地址（回环源本机收不到，
//! 实测 127.0.0.1 源无效）；MC 客户端据此连 `<本机IP>:本地端口`
//! （guest 转发端口须同绑 0.0.0.0，见 session.rs）。

use std::net::UdpSocket;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread;
use std::time::Duration;

/// MC 局域网发现组播地址与端口（与 mc_discover.rs 一致）。
const MC_MULTICAST_ADDR: &str = "224.0.2.60";
const MC_MULTICAST_PORT: u16 = 4445;

/// 广播周期：MC 客户端对广播的 TTL 为 5s，1.5s 一次足够刷新。
const ADVERTISE_INTERVAL: Duration = Duration::from_millis(1500);

/// 广播中显示的服务器名回退文案（房间码无 host 存档名时）。
const DEFAULT_MOTD: &str = "Elytra 联机";

/// 组播广播器：drop 即停线程。
pub struct McAdvertiser {
    quit_tx: Sender<()>,
}

impl McAdvertiser {
    /// 带自定义服务器名启动（host 存档名；空串回退默认文案）。
    pub fn start_with_motd(local_port: u16, motd: &str) -> Self {
        let motd = if motd.trim().is_empty() {
            DEFAULT_MOTD
        } else {
            motd
        };
        Self::start_internal(local_port, motd, MC_MULTICAST_PORT)
    }

    fn start_internal(local_port: u16, motd: &str, multicast_port: u16) -> Self {
        let (quit_tx, quit_rx) = channel::<()>();
        let payload = format!("[MOTD]{motd}[/MOTD][AD]{local_port}[/AD]");
        thread::spawn(move || {
            // bind 0.0.0.0：源 IP 由 OS 选主网卡地址（回环源收不到，实测），
            // MC 客户端连 <本机IP>:local_port（转发端口同绑 0.0.0.0）。
            let Ok(socket) = UdpSocket::bind(("0.0.0.0", 0)) else {
                eprintln!("[online] MC 广播 socket 绑定失败（联机功能不受影响）");
                return;
            };
            // 本机回环：MC 客户端与广播同机，需组播回环才能收到
            let _ = socket.set_multicast_loop_v4(true);
            let _ = socket.set_multicast_ttl_v4(1);
            while !is_quit(&quit_rx) {
                let _ = socket.send_to(payload.as_bytes(), (MC_MULTICAST_ADDR, multicast_port));
                thread::sleep(ADVERTISE_INTERVAL);
            }
        });
        Self { quit_tx }
    }
}

fn is_quit(quit_rx: &Receiver<()>) -> bool {
    matches!(
        quit_rx.try_recv(),
        Ok(()) | Err(std::sync::mpsc::TryRecvError::Disconnected)
    )
}

impl Drop for McAdvertiser {
    fn drop(&mut self) {
        let _ = self.quit_tx.send(());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn advertiser_starts_and_stops() {
        // 启动 + drop 不 panic（线程自杀）
        let a = McAdvertiser::start_with_motd(25565, "");
        thread::sleep(Duration::from_millis(200));
        drop(a);
    }

    #[test]
    fn builds_multicast_payload() {
        // 与 mc_discover 的 parse_port 格式互操作
        let payload = format!("[MOTD]Elytra 联机[/MOTD][AD]34567[/AD]");
        assert!(payload.starts_with("[MOTD]"));
        assert!(payload.contains("[AD]34567[/AD]"));
    }

    #[test]
    fn motd_fallback_to_default() {
        // 空串 MOTD → 回退默认文案（payload 不为空 MOTD 段）
        let a = McAdvertiser::start_with_motd(40000, "");
        drop(a);
        let a = McAdvertiser::start_with_motd(40000, "新的世界");
        drop(a);
    }

    /// 端到端：启动广播器（注入测试组播端口），加入组播组监听，
    /// 应在本机回环收到格式正确的广播（Windows 组播回环验证）。
    #[test]
    fn broadcast_loopback_reachable() {
        const TEST_PORT: u16 = 4448;
        let a = McAdvertiser::start_internal(34567, "test", TEST_PORT);
        let recv = UdpSocket::bind(("0.0.0.0", TEST_PORT)).expect("bind listen");
        recv.join_multicast_v4(&"224.0.2.60".parse().unwrap(), &"0.0.0.0".parse().unwrap())
            .expect("join multicast group");
        recv.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        let mut buf = [0u8; 256];
        let (n, src) = recv.recv_from(&mut buf).expect("应收到组播回环广播");
        let text = String::from_utf8_lossy(&buf[..n]);
        assert!(
            text.starts_with("[MOTD]") && text.contains("[AD]34567[/AD]"),
            "广播格式不符: {text}"
        );
        eprintln!("收到广播 {text} from {src}");
        drop(a);
    }
}
