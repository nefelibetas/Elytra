//! Scaffolding 联机协议（Scaffolding-MC/Scaffolding-MC）。
//! 模块：`room` 房间码生成/解析（base34 + 模 7）、`codec` 信息协议 TCP 帧编解码（大端序）、`identity` machine_id 持久化、`profiles` 玩家表、`server` host 信息服务器（handler 分发）。

pub mod codec;
pub mod identity;
pub mod profiles;
pub mod room;
pub mod server;
