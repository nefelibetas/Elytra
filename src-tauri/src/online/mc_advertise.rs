//! MC 局域网服务器广播模拟（guest 端）。
//!
//! MC 26.2 对"手动输入 IP"连接做会话校验（离线账号报"无效会话"），"局域网发现"路径跳过校验；guest 连本地转发端口属前者，
//! 故周期向 `224.0.2.60:4445` 广播 `[MOTD]<名字>[/MOTD][AD]<本地端口>[/AD]`，让客户端在"局域网"标签页看到服务器。
//! MOTD 取房间码携带的 host 存档名，无则回退固定文案。
//! socket bind 0.0.0.0（源 IP 须为主网卡：回环源收不到）；MC 据此连 `<本机IP>:本地端口`，故转发端口同绑 0.0.0.0。

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
            // bind 0.0.0.0：OS 选主网卡源 IP（回环源收不到）；MC 连 <本机IP>:local_port，转发端口同绑 0.0.0.0。
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
        let a = McAdvertiser::start_with_motd(25565, "");
        thread::sleep(Duration::from_millis(200));
        drop(a);
    }

    #[test]
    fn builds_multicast_payload() {
        let payload = "[MOTD]Elytra 联机[/MOTD][AD]34567[/AD]".to_string();
        assert!(payload.starts_with("[MOTD]"));
        assert!(payload.contains("[AD]34567[/AD]"));
    }

    #[test]
    fn motd_fallback_to_default() {
        let a = McAdvertiser::start_with_motd(40000, "");
        drop(a);
        let a = McAdvertiser::start_with_motd(40000, "新的世界");
        drop(a);
    }

    /// 端到端：注入测试组播端口启动广播器并加入组播组，应在本机回环收到格式正确的广播。
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
