//! host 信息服务器：Scaffolding 协议 TCP 服务器（bind 本机 `0.0.0.0:port`）。
//!
//! 对齐参考实现 Terracotta：`TcpListener` bind 本机端口（默认 13448，失败回退 0），
//! 每连接一个线程，循环处理请求直到出错。协议 handler 由调用方注入
//! （`Handlers`），服务器只负责帧读写与分发。
//!
//! 关键：此服务器监听**本机**端口，guest 经 easytier 虚拟网直连 host 虚拟 IP 的
//! 该端口（no_tun 模式下由 easytier 数据面路由到 host 本机监听）。
//!
//! `start` 为 host 信息服务器生产入口（`start_host` 调用；OS 监听 + blocking 帧
//! 读写，对齐参考实现 Terracotta），返回 [`InfoServer`] 句柄——调用方须持有，
//! drop 即停 accept 线程并释放端口。guest 数据面（`join` 心跳/协商）不走本模块，
//! 直接用 codec async API 直连中心。

use std::io;
use std::net::{Ipv4Addr, SocketAddrV4, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use parking_lot::Mutex;

use super::codec::{self, Request, Response};

/// 连接级读写超时（防挂死连接占线程）。
const CONN_TIMEOUT: Duration = Duration::from_secs(64);

/// accept 轮询间隔：listener 置非阻塞后靠轮询 + 退出标志自检——阻塞 `accept()`
/// 无法被外部唤醒，句柄 drop 时就收不回线程与监听端口。
const ACCEPT_POLL: Duration = Duration::from_millis(100);

/// handler 签名：`fn(&Request) -> io::Result<Response>`。
pub type HandleFn = Arc<dyn Fn(&Request) -> io::Result<Response> + Send + Sync>;

/// 注册表项：命名空间 + 协议名 + handler。
type HandlerEntry = (&'static str, &'static str, HandleFn);

/// 协议 handler 集合（调用方注入；服务器自身不感知具体协议）。
#[derive(Clone)]
pub struct Handlers {
    inner: Arc<Mutex<Vec<HandlerEntry>>>,
}

impl Handlers {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// 注册 handler（namespace, value, 处理函数）。
    pub fn register(&self, ns: &'static str, value: &'static str, handle: HandleFn) {
        self.inner.lock().push((ns, value, handle));
    }

    /// 查找匹配 handler；未找到 → 默认 255 未知错误。
    pub fn dispatch(&self, req: &Request) -> io::Result<Response> {
        let handlers = self.inner.lock();
        let matched = handlers
            .iter()
            .find(|(ns, value, _)| *ns == req.namespace && *value == req.value)
            .map(|(_, _, f)| f.clone());
        drop(handlers);
        match matched {
            Some(f) => f(req),
            None => Ok(Response::fail(
                codec::STATUS_UNKNOWN_ERROR,
                b"Requested protocol hasn't been implemented.".to_vec(),
            )),
        }
    }
}

impl Default for Handlers {
    fn default() -> Self {
        Self::new()
    }
}

/// 运行中的信息服务器句柄。
///
/// `start` 返回它，调用方**必须持有**：drop（或 [`InfoServer::shutdown`]）会置退出
/// 标志并 join accept 线程，随即释放监听端口。若丢弃句柄（旧实现的 bug），每次
/// 「开启→停止」都会泄漏一个阻塞线程 + `0.0.0.0:<port>`，之后所有 host 会话的默认
/// 端口探测都会失败并退化为随机端口，残留服务器还会用旧玩家表应答。
pub struct InfoServer {
    port: u16,
    quit: Arc<AtomicBool>,
    handle: Option<thread::JoinHandle<()>>,
}

impl InfoServer {
    /// 实际监听端口（传入端口被占用时回退到的随机端口真实值）。
    pub fn port(&self) -> u16 {
        self.port
    }

    /// 显式停止：置退出标志并 join accept 线程（幂等）。
    pub fn shutdown(&mut self) {
        self.quit.store(true, Ordering::SeqCst);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for InfoServer {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// 启动信息服务器。返回的句柄**必须被持有**：drop（或 `shutdown`）即停止服务器
/// 并释放监听端口；`port()` 给出实际监听端口（传入端口被占用时回退随机）。
pub fn start(handlers: Handlers, port: u16) -> io::Result<InfoServer> {
    let listener = TcpListener::bind(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, port))?;
    listener.set_nonblocking(true)?;
    let actual = listener.local_addr()?.port();
    let quit = Arc::new(AtomicBool::new(false));
    let quit_thread = Arc::clone(&quit);
    let handle = thread::spawn(move || {
        while !quit_thread.load(Ordering::SeqCst) {
            match listener.accept() {
                Ok((stream, _)) => {
                    // 非阻塞 listener 在 Windows 上会把 accept 出的连接也置非阻塞；
                    // 显式恢复阻塞模式，连接线程才能做带超时的阻塞读写。
                    if stream.set_nonblocking(false).is_err() {
                        continue;
                    }
                    let handlers = handlers.clone();
                    thread::spawn(move || {
                        let _ = handle_connection(stream, &handlers);
                    });
                }
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                    thread::sleep(ACCEPT_POLL);
                }
                Err(e) => {
                    eprintln!("[online] Scaffolding 服务器 accept 失败: {e}");
                    thread::sleep(ACCEPT_POLL);
                }
            }
        }
    });
    Ok(InfoServer {
        port: actual,
        quit,
        handle: Some(handle),
    })
}

/// 处理单个连接：循环读请求 → 分发 → 写响应，直到 IO 错误/超时。
fn handle_connection(mut stream: TcpStream, handlers: &Handlers) -> io::Result<()> {
    stream.set_read_timeout(Some(CONN_TIMEOUT))?;
    stream.set_write_timeout(Some(CONN_TIMEOUT))?;
    loop {
        let req = codec::read_request(&mut stream)?;
        let resp = handlers.dispatch(&req)?;
        codec::write_response(&mut stream, &resp)?;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dispatch_unknown_returns_255() {
        let handlers = Handlers::new();
        let req = Request {
            namespace: "c".into(),
            value: "nonexistent".into(),
            body: vec![],
        };
        let resp = handlers.dispatch(&req).unwrap();
        assert_eq!(resp.status, codec::STATUS_UNKNOWN_ERROR);
    }

    #[test]
    fn dispatch_registered() {
        let handlers = Handlers::new();
        handlers.register(
            "c",
            "ping",
            Arc::new(|req: &Request| Ok(Response::ok(req.body.clone()))),
        );
        let req = Request {
            namespace: "c".into(),
            value: "ping".into(),
            body: b"hello".to_vec(),
        };
        let resp = handlers.dispatch(&req).unwrap();
        assert_eq!(resp.status, 0);
        assert_eq!(resp.body, b"hello");
    }

    #[test]
    fn server_start_binds_port() {
        let handlers = Handlers::new();
        let server = start(handlers, 0).expect("服务器应启动");
        let port = server.port();
        assert!(port > 0);
        // 验证端口确实在监听
        let conn = TcpStream::connect(("127.0.0.1", port));
        assert!(conn.is_ok(), "服务器应可连接");
    }

    /// 回归：句柄 drop 后 accept 线程退出、端口立即可被重新占用。
    /// 旧实现丢弃句柄，端口与阻塞线程一起泄漏，导致后续会话默认端口探测失败。
    #[test]
    fn server_shutdown_releases_port() {
        let port = {
            let server = start(Handlers::new(), 0).expect("服务器应启动");
            server.port()
        }; // drop → shutdown
        let rebound = TcpListener::bind(("0.0.0.0", port));
        assert!(rebound.is_ok(), "停止后端口应可被重新占用");
    }

    #[test]
    fn server_echo_roundtrip() {
        let handlers = Handlers::new();
        handlers.register(
            "c",
            "ping",
            Arc::new(|req: &Request| Ok(Response::ok(req.body.clone()))),
        );
        let server = start(handlers, 0).unwrap();
        let mut stream = TcpStream::connect(("127.0.0.1", server.port())).unwrap();
        codec::write_request(&mut stream, "c", "ping", b"abc").unwrap();
        let resp = codec::read_response(&mut stream).unwrap();
        assert_eq!(resp.status, 0);
        assert_eq!(resp.body, b"abc");
        // 未知协议 → 255
        codec::write_request(&mut stream, "c", "nope", b"").unwrap();
        let resp = codec::read_response(&mut stream).unwrap();
        assert_eq!(resp.status, codec::STATUS_UNKNOWN_ERROR);
    }
}
