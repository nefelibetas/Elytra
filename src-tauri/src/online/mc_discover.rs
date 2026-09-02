//! Minecraft 局域网组播监听：自动发现"对局域网开放"的游戏端口与存档名。
//!
//! MC 局域网游戏每 1.5s 向 `224.0.2.60:4445`（IPv4）发组播，
//! 载荷格式 `[MOTD]...[/MOTD][AD]<port>[/AD]`。监听并解析 `<port>` 与 MOTD，
//! 用于 host 侧自动获知游戏端口（用户无需手动填写）与存档名（编入房间码，
//! guest 端 MC 局域网广播显示真实世界名）。

use std::net::{Ipv4Addr, SocketAddr, UdpSocket};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use parking_lot::Mutex;

/// MC 局域网组播地址与端口（与游戏广播契约一致）。
const MC_MULTICAST_IP: Ipv4Addr = Ipv4Addr::new(224, 0, 2, 60);
const MC_MULTICAST_PORT: u16 = 4445;
/// 组播包存活窗口：包每 1.5s 发一次，5s 无包视为端口失效。
const PORT_TTL: Duration = Duration::from_secs(5);

/// 监测到的 MC 服务器（端口 + 存档名 MOTD + 最后活动时间）。
type McEntry = (u16, String, Instant);

/// MC 端口扫描器：后台线程监听组播，持续刷新 `get_port()` / `get_motd()`。
pub struct MinecraftScanner {
    entries: Arc<Mutex<Vec<McEntry>>>,
    /// drop 时通知线程退出。
    quit_tx: Sender<()>,
}

impl MinecraftScanner {
    /// 创建扫描器并启动监听线程。
    pub fn start() -> Self {
        let (quit_tx, quit_rx) = mpsc::channel::<()>();
        let entries = Arc::new(Mutex::new(Vec::new()));
        let entries_thread = entries.clone();
        thread::spawn(move || {
            let _ = Self::run(quit_rx, entries_thread);
        });
        Self { entries, quit_tx }
    }

    /// 监听循环：绑定组播 socket，收包解析端口与 MOTD，超时老化。
    fn run(quit_rx: Receiver<()>, entries: Arc<Mutex<Vec<McEntry>>>) -> std::io::Result<()> {
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
                let data = &buf[..len];
                if let Some(p) = parse_port(data) {
                    let motd = parse_motd(data).unwrap_or_default();
                    let mut list = entries.lock();
                    list.retain(|(pp, _, _)| *pp != p);
                    list.push((p, motd, Instant::now()));
                }
            }
            // 老化过期端口
            let now = Instant::now();
            entries
                .lock()
                .retain(|(_, _, at)| now.duration_since(*at) < PORT_TTL);
        }
    }

    /// 当前监测到的 MC 端口（None = 尚未发现/已过期）。
    pub fn get_port(&self) -> Option<u16> {
        self.entries.lock().iter().map(|(p, _, _)| *p).next()
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

/// 解析组播载荷中的 `[MOTD]<motd>[/MOTD]`（无则空串）。
fn parse_motd(data: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(data);
    let motd_start = text.find("[MOTD]")?;
    let rest = &text[motd_start + "[MOTD]".len()..];
    let motd = rest.split("[/MOTD]").next()?;
    let motd = motd.trim();
    if motd.is_empty() {
        None
    } else {
        Some(motd.to_string())
    }
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

    #[test]
    fn parses_motd_from_payload() {
        assert_eq!(
            parse_motd("[MOTD]新的世界[/MOTD][AD]25565[/AD]".as_bytes()).as_deref(),
            Some("新的世界")
        );
        assert_eq!(parse_motd(b"[AD]25565[/AD]"), None, "无 MOTD 段");
        assert_eq!(parse_motd(b"[MOTD]  [/MOTD][AD]1[/AD]"), None, "空 MOTD");
        assert_eq!(parse_motd(b"nope"), None);
    }
}
