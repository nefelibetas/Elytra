//! Scaffolding 信息协议帧编解码（TCP，大端序）。
//!
//! 请求：`[u8 类型长度][类型 "ns:value"][u32 请求体长度][请求体]`
//! 响应：`[u8 状态][u32 响应体长度][响应体]`
//!
//! 状态：0 = 成功；[32, 64) = 协议定义错误；255 = 未知错误（响应体 UTF8 详情）。
//! 类型：`{namespace}:{value}`，namespace/value 仅小写字母、下划线、数字 ASCII。
//!
//! blocking 版本（`read_request`/`write_request`/`read_response`/`write_response`）
//! 主要供测试与本地转发场景使用；运行时走 async 版本。

#![allow(dead_code)]

use std::io::{self, Read, Write};

/// 标准协议命名空间（community）。
pub const NS_COMMUNITY: &str = "c";

/// 响应状态：成功。
pub const STATUS_OK: u8 = 0;
/// 响应状态：未知错误（响应体为 UTF8 错误详情）。
pub const STATUS_UNKNOWN_ERROR: u8 = 255;

/// 请求帧。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    pub namespace: String,
    pub value: String,
    pub body: Vec<u8>,
}

impl Request {
    /// 完整类型串 `ns:value`。
    pub fn kind(&self) -> String {
        format!("{}:{}", self.namespace, self.value)
    }
}

/// 响应帧。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Response {
    pub status: u8,
    pub body: Vec<u8>,
}

impl Response {
    pub fn ok(body: Vec<u8>) -> Self {
        Self {
            status: STATUS_OK,
            body,
        }
    }
    pub fn fail(status: u8, body: Vec<u8>) -> Self {
        Self {
            status,
            body,
        }
    }
    /// 是否成功。
    pub fn is_ok(&self) -> bool {
        self.status == STATUS_OK
    }
}

/// 从流中读一个请求（blocking）。
pub fn read_request<R: Read>(r: &mut R) -> io::Result<Request> {
    let mut kind_len = [0u8; 1];
    r.read_exact(&mut kind_len)?;
    let kind_len = kind_len[0] as usize;
    if kind_len == 0 || kind_len > 255 {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "请求类型长度非法"));
    }
    let mut kind = vec![0u8; kind_len];
    r.read_exact(&mut kind)?;
    let kind = String::from_utf8(kind)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("类型非 UTF8: {e}")))?;
    let (namespace, value) = split_kind(&kind)?;
    let mut body_len = [0u8; 4];
    r.read_exact(&mut body_len)?;
    let body_len = u32::from_be_bytes(body_len) as usize;
    let mut body = vec![0u8; body_len];
    r.read_exact(&mut body)?;
    Ok(Request {
        namespace: namespace.to_string(),
        value: value.to_string(),
        body,
    })
}

/// 写一个请求到流（blocking）。
pub fn write_request<W: Write>(w: &mut W, namespace: &str, value: &str, body: &[u8]) -> io::Result<()> {
    let kind = format!("{namespace}:{value}");
    let kind_len = kind.len();
    if kind_len == 0 || kind_len > 255 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "请求类型长度非法",
        ));
    }
    w.write_all(&[kind_len as u8])?;
    w.write_all(kind.as_bytes())?;
    w.write_all(&(body.len() as u32).to_be_bytes())?;
    w.write_all(body)?;
    w.flush()
}

/// 从流中读一个响应（blocking）。
pub fn read_response<R: Read>(r: &mut R) -> io::Result<Response> {
    let mut status = [0u8; 1];
    r.read_exact(&mut status)?;
    let mut body_len = [0u8; 4];
    r.read_exact(&mut body_len)?;
    let body_len = u32::from_be_bytes(body_len) as usize;
    let mut body = vec![0u8; body_len];
    r.read_exact(&mut body)?;
    Ok(Response {
        status: status[0],
        body,
    })
}

/// 写一个响应到流（blocking）。
pub fn write_response<W: Write>(w: &mut W, response: &Response) -> io::Result<()> {
    w.write_all(&[response.status])?;
    w.write_all(&(response.body.len() as u32).to_be_bytes())?;
    w.write_all(&response.body)?;
    w.flush()
}

/// 从 async 流中读一个请求（tokio AsyncRead）。
pub async fn read_request_async<R>(r: &mut R) -> io::Result<Request>
where
    R: tokio::io::AsyncRead + Unpin,
{
    use tokio::io::AsyncReadExt;
    let mut kind_len = [0u8; 1];
    r.read_exact(&mut kind_len).await?;
    let kind_len = kind_len[0] as usize;
    if kind_len == 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "请求类型长度非法",
        ));
    }
    let mut kind = vec![0u8; kind_len];
    r.read_exact(&mut kind).await?;
    let kind = String::from_utf8(kind)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("类型非 UTF8: {e}")))?;
    let (namespace, value) = split_kind(&kind)?;
    let mut body_len = [0u8; 4];
    r.read_exact(&mut body_len).await?;
    let body_len = u32::from_be_bytes(body_len) as usize;
    let mut body = vec![0u8; body_len];
    r.read_exact(&mut body).await?;
    Ok(Request {
        namespace: namespace.to_string(),
        value: value.to_string(),
        body,
    })
}

/// 写一个请求到 async 流（tokio AsyncWrite）。
pub async fn write_request_async<W>(w: &mut W, namespace: &str, value: &str, body: &[u8]) -> io::Result<()>
where
    W: tokio::io::AsyncWrite + Unpin,
{
    use tokio::io::AsyncWriteExt;
    let kind = format!("{namespace}:{value}");
    let kind_len = kind.len();
    if kind_len == 0 || kind_len > 255 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "请求类型长度非法",
        ));
    }
    w.write_all(&[kind_len as u8]).await?;
    w.write_all(kind.as_bytes()).await?;
    w.write_all(&(body.len() as u32).to_be_bytes()).await?;
    w.write_all(body).await?;
    w.flush().await
}

/// 从 async 流中读一个响应（tokio AsyncRead）。
pub async fn read_response_async<R>(r: &mut R) -> io::Result<Response>
where
    R: tokio::io::AsyncRead + Unpin,
{
    use tokio::io::AsyncReadExt;
    let mut status = [0u8; 1];
    r.read_exact(&mut status).await?;
    let mut body_len = [0u8; 4];
    r.read_exact(&mut body_len).await?;
    let body_len = u32::from_be_bytes(body_len) as usize;
    let mut body = vec![0u8; body_len];
    r.read_exact(&mut body).await?;
    Ok(Response {
        status: status[0],
        body,
    })
}

/// 写一个响应到 async 流（tokio AsyncWrite）。
pub async fn write_response_async<W>(w: &mut W, response: &Response) -> io::Result<()>
where
    W: tokio::io::AsyncWrite + Unpin,
{
    use tokio::io::AsyncWriteExt;
    w.write_all(&[response.status]).await?;
    w.write_all(&(response.body.len() as u32).to_be_bytes()).await?;
    w.write_all(&response.body).await?;
    w.flush().await
}

/// 拆分 `ns:value` 类型串。协议要求 namespace/value 仅小写字母、下划线、数字。
fn split_kind(kind: &str) -> io::Result<(&str, &str)> {
    let Some((ns, value)) = kind.split_once(':') else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "请求类型缺少 ':' 分隔",
        ));
    };
    if ns.is_empty() || value.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "请求类型为空段",
        ));
    }
    if !ns.bytes().all(is_allowed_kind_char) || !value.bytes().all(is_allowed_kind_char) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "请求类型含非法字符",
        ));
    }
    Ok((ns, value))
}

fn is_allowed_kind_char(b: u8) -> bool {
    b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_'
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_roundtrip() {
        let req = Request {
            namespace: "c".into(),
            value: "ping".into(),
            body: vec![1, 2, 3, 4],
        };
        let mut buf = Vec::new();
        write_request(&mut buf, &req.namespace, &req.value, &req.body).unwrap();
        let mut cur = io::Cursor::new(&buf);
        let parsed = read_request(&mut cur).unwrap();
        assert_eq!(parsed, req);
    }

    #[test]
    fn response_roundtrip() {
        let resp = Response {
            status: 0,
            body: vec![0x12, 0x34],
        };
        let mut buf = Vec::new();
        write_response(&mut buf, &resp).unwrap();
        let mut cur = io::Cursor::new(&buf);
        assert_eq!(read_response(&mut cur).unwrap(), resp);
    }

    #[test]
    fn empty_body_roundtrip() {
        let mut buf = Vec::new();
        write_request(&mut buf, "c", "server_port", &[]).unwrap();
        let mut cur = io::Cursor::new(&buf);
        let parsed = read_request(&mut cur).unwrap();
        assert_eq!(parsed.namespace, "c");
        assert_eq!(parsed.value, "server_port");
        assert!(parsed.body.is_empty());
    }

    #[test]
    fn kind_too_long_rejected() {
        let ns = "a".repeat(300);
        let mut buf = Vec::new();
        assert!(write_request(&mut buf, &ns, "x", &[]).is_err());
    }

    #[test]
    fn invalid_kind_rejected() {
        // 大写/特殊字符 → 拒绝
        let mut cur = io::Cursor::new(b"\x05C:ping\x00\x00\x00\x00".as_slice());
        assert!(read_request(&mut cur).is_err());
    }

    #[test]
    fn truncated_stream_rejected() {
        let mut cur = io::Cursor::new(b"\x05c:pi".as_slice());
        assert!(read_request(&mut cur).is_err());
    }

    #[test]
    fn u32_be_body_len() {
        // 帧 = [1 类型长度][类型 "c:x" 3][4 长度]：长度偏移 4
        let mut buf = Vec::new();
        write_request(&mut buf, "c", "x", &vec![0u8; 2000]).unwrap();
        assert_eq!(&buf[4..8], &[0x00, 0x00, 0x07, 0xD0]);
    }
}
