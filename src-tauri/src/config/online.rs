use serde::{Deserialize, Serialize};

/// 默认协调节点（备用）：首次使用联机功能时的预填值，开箱即用。
/// `tcp://node.invalid:11010` 为社区公共服务器（阿里云），2026-08 实测可达且支持打洞；
/// 社区节点可能随时变动，用户在设置里可修改/清空（保存后以用户值为准）。
/// easytier 官方公共节点（node.invalid 等）已下线（DNS NXDOMAIN），不再内置。
pub const DEFAULT_PUBLIC_NODES: &[&str] = &["tcp://node.invalid:11010"];

/// 联机配置（persist section "online"）。
#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq)]
pub struct OnlineConfig {
    /// 协调节点列表（easytier peer URI，如 `tcp://1.2.3.4:11010`）。
    /// host/guest 连同一批节点虚拟网内互发现；跨网（非同一局域网）联机必需。
    /// 支持多行多节点（easytier 并行连接，任一可用即可组网，天然主备）。
    /// 空列表 = 仅同网直连（guest 经房间码直连 host 局域网 IP）。
    #[serde(default)]
    pub public_nodes: Vec<String>,
}

impl OnlineConfig {
    /// 从 persist 读取。从未保存过配置 → 默认备用节点（开箱即用跨网）；
    /// 保存过（含清空）→ 用户值。
    pub fn from_persist() -> Self {
        match crate::persist::read::<OnlineConfig>(crate::constants::SECT_ONLINE) {
            Ok(cfg) => cfg,
            Err(_) => Self {
                public_nodes: DEFAULT_PUBLIC_NODES.iter().map(|s| s.to_string()).collect(),
            },
        }
    }

    /// 写入 persist。
    pub fn save(&self) -> Result<(), String> {
        crate::persist::save::<OnlineConfig>(crate::constants::SECT_ONLINE, self)
            .map_err(|e| format!("保存联机配置失败\n{e}"))
    }
}
