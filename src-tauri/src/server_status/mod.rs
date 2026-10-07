//! 服务器状态探测：Minecraft Java 版 SLP（Server List Ping，1.7+）与基岩版
//! RakNet UNCONNECTED_PING。
//!
//! - Java：TCP 握手（协议版本 -1）+ status 请求 → JSON（版本/在线人数/MOTD/玩家样例/图标）。
//! - Bedrock：UDP 一发一收（UNCONNECTED_PING → UNCONNECTED_PONG），载荷为分号分隔字段。
//!
//! 全部超时包裹，避免探测不可达服务器时挂起命令线程。
//! 协议实现见 [`api`]。

mod api;

pub use api::{
    parse_address, ping_bedrock, ping_java, ServerStatus, BEDROCK_DEFAULT_PORT, JAVA_DEFAULT_PORT,
};
