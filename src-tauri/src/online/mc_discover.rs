//! Minecraft 局域网组播监听：自动发现"对局域网开放"的游戏端口。
//!
//! MC 局域网游戏每 1.5s 向 `224.0.2.60:4445`（IPv4）发组播，
//! 载荷格式 `[MOTD]...[/MOTD][AD]<port>[/AD]`。监听并解析 `<port>`，
//! 用于 host 侧自动获知游戏端口（用户无需手动填写）。

use std::net::{Ipv4Addr, SocketAddr, UdpSocket};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

/// MC 局域网组播地址与端口（与游戏广播契约一致）。
const MC_MULTICAST_IP: Ipv4Addr = Ipv4Addr::new(224, 0, 2, 60);
const MC_MULTICAST_PORT: u16 = 4445;
/// 组播包存活窗口：包每 1.5s 发一次，5s 无包视为端口失效。
const PORT_TTL: Duration = Duration::from_secs(5);

/// MC 端口扫描器：后台线程监听组播，持续刷新 `get_port()`。
pub struct MinecraftScanner {
    port: Arc<Mutex<Vec<(u16, Instant)>>>,
    /// drop 时通知线程退出。
    quit_tx: Sender<()>,
}

impl MinecraftScanner {
    /// 创建扫描器并启动监听线程。
    pub fn start() -> Self {
        let (quit_tx, quit_rx) = mpsc::channel::<()>();
        let port = Arc::new(Mutex::new(Vec::new()));
        let port_thread = port.clone();
        thread::spawn(move || {
            let _ = Self::run(quit_rx, port_thread);
        });
        Self { port, quit_tx }
    }

    /// 监听循环：绑定组播 socket，收包解析端口，超时老化。
    fn run(quit_rx: Receiver<()>, port: Arc<Mutex<Vec<(u16, Instant)>>>) -> std::io::Result<()> {
        let socket = UdpSocket::bind(SocketAddr::from(([0, 0, 0, 0], MC_MULTICAST_PORT)))?;
        socket.join_multicast_v4(&MC_MULTICAST_IP, &Ipv4Addr::UNSPECIFIED)?;
        socket.set_read_timeout(Some(Duration::from_millis(300)))?;

        let mut buf = [0u8; 4096];
        loop {
            // 退出信号（channel 断开也算）
            match quit_rx.try_recv() {
                Ok(()) | Err(mpsc::TryRecvError::Disconnected) => return Ok(()),
                Err(mpsc::TryRecvError::Empty) => {}
            }
            if let Ok((len, _)) = socket.recv_from(&mut buf) {
                if let Some(p) = parse_port(&buf[..len]) {
                    let mut list = port.lock().unwrap();
                    list.retain(|(pp, _)| *pp != p);
                    list.push((p, Instant::now()));
                }
            }
            // 老化过期端口
            let now = Instant::now();
            port.lock()
                .unwrap()
                .retain(|(_, at)| now.duration_since(*at) < PORT_TTL);
        }
    }

    /// 当前监测到的 MC 端口（None = 尚未发现/已过期）。
    pub fn get_port(&self) -> Option<u16> {
        self.port.lock().unwrap().iter().map(|(p, _)| *p).next()
    }
}

impl Drop for MinecraftScanner {
    fn drop(&mut self) {
        let _ = self.quit_tx.send(());
    }
}

/// 解析组播载荷中的 `[AD]<port>[/AD]`。
fn parse_port(data: &[u8]) -> Option<u16> {
    let text = String::from_utf8_lossy(data);
    let ad_start = text.find("[AD]")?;
    let rest = &text[ad_start + "[AD]".len()..];
    let port_str = rest.split("[/AD]").next()?;
    port_str.trim().parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_mc_multicast_payload() {
        assert_eq!(
            parse_port(b"[MOTD]A server[/MOTD][AD]25565[/AD]"),
            Some(25565)
        );
        assert_eq!(parse_port(b"[AD]50000[/AD]"), Some(50000));
        assert_eq!(parse_port(b"nope"), None);
        assert_eq!(parse_port(b"[AD]abc[/AD]"), None);
    }
}
