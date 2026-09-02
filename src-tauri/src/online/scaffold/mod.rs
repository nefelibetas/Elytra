//! Scaffolding 联机协议（Scaffolding-MC/Scaffolding-MC）。
//!
//! 模块划分：
//! - [`room`]：房间码生成/解析（base34 + 模 7 + 明文映射网络名/密钥）
//! - [`codec`]：信息协议 TCP 帧编解码（大端序）
//! - [`identity`]：machine_id 持久化
//! - [`profiles`]：玩家表（profile 模型 + host 在线维护）
//! - [`server`]：host 信息服务器（handler 分发；数据面驱动在 `crate::online::mod`）
//!
//! 参考：协议 README（MIT） + Terracotta 实现行为（AGPL，仅参考不复制）。

pub mod codec;
pub mod identity;
pub mod profiles;
pub mod room;
pub mod server;
