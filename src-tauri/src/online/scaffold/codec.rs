//! Scaffolding 信息协议帧编解码（TCP，大端序）。
//!
//! 请求：`[u8 类型长度][类型 "ns:value"][u32 请求体长度][请求体]`
//! 响应：`[u8 状态][u32 响应体长度][响应体]`
//!
//! 状态：0 = 成功；[32, 64) = 协议定义错误；255 = 未知错误（响应体 UTF8 详情）。
//! 类型：`{namespace}:{value}`，namespace/value 仅小写字母、下划线、数字 ASCII。
//!
//! blocking 版本供 OS 监听服务器（`super::server`）与集成测试（`online_diag`）使用；
//! 运行时数据面（guest 心跳/协商）走 async 版本。两版本帧格式一致，测试据此
//! 验证跨实现互操作。

use std::io::{self, Read, Write};

use tokio::io::{AsyncReadExt, AsyncWriteExt};

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
}

/// 协议帧体长度上限（16 MiB）。
///
/// 长度字段是外部可控的裸 `u32`：若直接 `vec![0u8; body_len]` 分配，一条特制帧头
/// （长度填 `0xFFFFFFFF`）就能让进程尝试分配 4 GiB → 分配失败即 abort（远程 DoS）。
/// 本协议最大合法体量是玩家表快照（数十 KB 量级），16 MiB 留足余量。
pub const MAX_BODY_LEN: usize = 16 * 1024 * 1024;

/// 校验帧体长度（读请求 / 读响应共用）。
fn check_body_len(len: u32) -> io::Result<usize> {
    let len = len as usize;
    if len > MAX_BODY_LEN {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("帧体长度超限: {len} > {MAX_BODY_LEN}"),
        ));
    }
    Ok(len)
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
    let body_len = check_body_len(u32::from_be_bytes(body_len))?;
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
    let body_len = check_body_len(u32::from_be_bytes(body_len))?;
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
    let body_len = check_body_len(u32::from_be_bytes(body_len))?;
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
    let mut status = [0u8; 1];
    r.read_exact(&mut status).await?;
    let mut body_len = [0u8; 4];
    r.read_exact(&mut body_len).await?;
    let body_len = check_body_len(u32::from_be_bytes(body_len))?;
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

    /// 帧体长度必须在**分配之前**校验：长度字段是外部可控的裸 `u32`，
    /// 直接按声明值分配会让一条特制帧头把进程打到 abort（远程 DoS）。
    #[test]
    fn rejects_oversize_bodies_before_allocating() {
        // 请求：声明 4 GiB 帧体
        let mut buf = Vec::new();
        buf.push(3u8);
        buf.extend_from_slice(b"c:x");
        buf.extend_from_slice(&u32::MAX.to_be_bytes());
        let mut cur = io::Cursor::new(&buf);
        let err = read_request(&mut cur).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);

        // 响应：同样
        let mut buf = Vec::new();
        buf.push(0u8);
        buf.extend_from_slice(&u32::MAX.to_be_bytes());
        let mut cur = io::Cursor::new(&buf);
        let err = read_response(&mut cur).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);

        // 边界：恰好等于上限合法，多 1 字节即拒绝
        assert!(check_body_len(MAX_BODY_LEN as u32).is_ok());
        assert!(check_body_len(MAX_BODY_LEN as u32 + 1).is_err());
    }

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
