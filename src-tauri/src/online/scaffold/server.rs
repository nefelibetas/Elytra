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
//! 读写，对齐参考实现 Terracotta）。guest 数据面（`join` 心跳/协商）不走本模块，
//! 直接用 codec async API 直连中心。

use std::io;
use std::net::{Ipv4Addr, SocketAddrV4, TcpListener, TcpStream};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use parking_lot::Mutex;

use super::codec::{self, Request, Response};

/// 连接级读写超时（防挂死连接占线程）。
const CONN_TIMEOUT: Duration = Duration::from_secs(64);

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

/// 启动信息服务器。返回实际监听端口（`port` 被占用时回退 0 随机）。
pub fn start(handlers: Handlers, port: u16) -> io::Result<u16> {
    let listener = TcpListener::bind(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, port))?;
    listener.set_nonblocking(false)?;
    let actual = listener.local_addr()?.port();
    thread::spawn(move || {
        for stream in listener.incoming() {
            match stream {
                Ok(stream) => {
                    let handlers = handlers.clone();
                    thread::spawn(move || {
                        let _ = handle_connection(stream, &handlers);
                    });
                }
                Err(e) => {
                    eprintln!("[online] Scaffolding 服务器 accept 失败: {e}");
                }
            }
        }
    });
    Ok(actual)
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
        let port = start(handlers, 0).expect("服务器应启动");
        assert!(port > 0);
        // 验证端口确实在监听
        let conn = TcpStream::connect(("127.0.0.1", port));
        assert!(conn.is_ok(), "服务器应可连接");
    }

    #[test]
    fn server_echo_roundtrip() {
        let handlers = Handlers::new();
        handlers.register(
            "c",
            "ping",
            Arc::new(|req: &Request| Ok(Response::ok(req.body.clone()))),
        );
        let port = start(handlers, 0).unwrap();
        let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
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
