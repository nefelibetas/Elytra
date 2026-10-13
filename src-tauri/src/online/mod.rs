//! 联机领域层：Scaffolding 协议 + EasyTier 库内嵌虚拟局域网（非子进程）。
//! 协议：房间码 `U/XXXX-XXXX-XXXX-XXXX`（base34 + 模 7 校验）明文映射网络名/密钥；联机中心 = host 虚拟网节点
//! （hostname `scaffolding-mc-server-{port}`）；信息协议命名空间 `c`（ping/protocols/server_port/player_ping/player_profiles_list）。
//! 数据面（no_tun + smoltcp，无虚拟网卡）：信息协议与游戏流量均走 `data_plane_tcp_*`——host bind 虚拟 IP 经 proxy NAT 转
//! `127.0.0.1:<端口>`、guest connect 虚拟 IP，建网配白名单 ACL；MC 侧 host 用 `mc_discover` 检测游戏端口、guest 用
//! `mc_advertise` 模拟局域网服务器绕过 MC 26.2 会话校验；事件经 `EMIT_TX` 派发 `online-state`（载荷 [`OnlineSnapshot`]，含玩家列表）。

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
