//! 联机状态模型与协议常量。

use std::time::Duration;

use super::scaffold::profiles::Profile;

/// 端口发现轮询间隔（游戏每 1.5s 广播一次组播，1s 轮询足够快）。
pub(super) const DISCOVER_POLL: Duration = Duration::from_secs(1);

/// Scaffolding 信息服务器默认端口（参考 Terracotta；占用时回退随机）。
pub(super) const DEFAULT_SCAFFOLD_PORT: u16 = 13448;

/// 协议前缀：中心 hostname。
pub(super) const SCAFFOLD_HOSTNAME_PREFIX: &str = "scaffolding-mc-server-";

/// 联机状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OnlineStatus {
    /// 未联机。
    Idle,
    /// host 虚拟网已建、等待游戏开放局域网（未拿到游戏端口）。
    Waiting,
    /// host 联机中（已检测到游戏端口，代理生效）。
    Hosting,
    /// guest 已加入。
    Joining,
}

impl OnlineStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Waiting => "waiting",
            Self::Hosting => "hosting",
            Self::Joining => "joining",
        }
    }
}

/// 联机状态快照：事件载荷 + 状态查询返回值（前端单一数据源）。
#[derive(Debug, Clone, serde::Serialize)]
pub struct OnlineSnapshot {
    pub status: &'static str,
    pub room_code: Option<String>,
    /// guest 本地转发端口（游戏内连 127.0.0.1:<port>，经"局域网"标签页进入）；非 guest 为 None。
    pub local_port: Option<u16>,
    /// 玩家列表（HOST + GUEST；协议 c:player_profiles_list）。
    pub players: Vec<Profile>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hostname_prefix_matches() {
        assert!("scaffolding-mc-server-13448".starts_with(SCAFFOLD_HOSTNAME_PREFIX));
        assert!("scaffolding-mc-server-77844".starts_with(SCAFFOLD_HOSTNAME_PREFIX));
        assert!(!"scaffolding-mc-other".starts_with(SCAFFOLD_HOSTNAME_PREFIX));
    }
}
