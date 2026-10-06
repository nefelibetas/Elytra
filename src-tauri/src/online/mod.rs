//! 联机领域层：Scaffolding 协议 + EasyTier 库内嵌虚拟局域网。
//!
//! 协议：房间码 `U/XXXX-XXXX-XXXX-XXXX`（base34 + 模 7 校验）明文映射网络名/密钥；
//! 联机中心 = host 虚拟网节点（hostname `scaffolding-mc-server-{port}`）；信息协议为
//! 标准命名空间 `c`（ping / protocols / server_port / player_ping / player_profiles_list）。
//! 流程：guest 入网 → 发现中心 → 心跳协商 → server_port → 每 5s 心跳。
//!
//! 数据面（no_tun + smoltcp，无虚拟网卡）：信息协议与游戏流量均走 `data_plane_tcp_*`
//! ——host 侧 bind 虚拟 IP 并经 proxy NAT 转到 `127.0.0.1:<端口>`，guest 侧 connect
//! 虚拟 IP；`c:server_port` 返回虚拟网内可达的游戏端口 entry。建网时一次配好
//! 白名单 `[scaffold_port, entry]`（ACL 管数据面入站）。
//! MC 侧：host 用 `mc_discover` 组播检测游戏端口；guest 用 `mc_advertise` 广播模拟
//! 局域网服务器，绕过 MC 26.2 对"手动输入 IP"的离线会话校验。
//!
//! 事件：经 `EMIT_TX` 队列派发 `online-state`（载荷 [`OnlineSnapshot`]，含玩家列表）。
//!
//! 实现划分：[`service`]（单例与共享状态）、[`host`]、[`guest`]、
//! [`model`]（状态模型与协议常量）、[`session`]（easytier 会话）、
//! [`scaffold`]（协议编解码 / 房间码 / 玩家表 / 信息服务器）、
//! [`mc_discover`] / [`mc_advertise`]（MC 组播侧）。

mod guest;
mod host;
mod mc_advertise;
mod mc_discover;
mod model;
mod scaffold;
mod service;
mod session;

pub use model::{OnlineSnapshot, OnlineStatus};
pub use scaffold::codec;
pub use scaffold::profiles::{Profile, ProfileKind};
pub use scaffold::room::{create_room, parse_room_code, RoomCredential};
pub use session::{EasyTierSession, RouteSnapshot, SessionConfig, SessionRole};

pub use guest::join;
pub use host::start_host;
pub use service::{status, stop};
