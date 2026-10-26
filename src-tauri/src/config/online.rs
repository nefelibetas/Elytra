use serde::{Deserialize, Serialize};

/// 默认协调节点：`https://etnode.zkitefly.eu.org/node{1,2}`（社区节点，明确禁中转，与默认仅直连契合）。
/// 官方 `node.invalid` / `node.invalid` 已下线；节点随时变动，用户可在设置中删除/添加。
pub const DEFAULT_PUBLIC_NODES: &[&str] = &[
    "https://etnode.zkitefly.eu.org/node1",
    "https://etnode.zkitefly.eu.org/node2",
];

/// 联机配置（persist section "online"）。
#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq, ts_rs::TS)]
#[ts(export_to = "config/")]
pub struct OnlineConfig {
    /// 协调节点列表（easytier peer URI，如 `tcp://1.2.3.4:11010`）。host/guest 连同一批节点虚拟网内互发现；
    /// 跨网联机必需；空列表 = 仅同网直连（guest 经房间码直连 host 局域网 IP）。
    #[serde(default)]
    pub public_nodes: Vec<String>,
    /// 自定义联机广播名（guest 端 MC 局域网标签页显示名，支持 § 格式码）；
    /// None/空 = 自动（用房间码携带的 host 存档名 MOTD，探测不到回退默认文案）。
    #[serde(default)]
    pub advertise_motd: Option<String>,
    /// 联机昵称（协议 c:player_ping 的 name 字段；空 = 系统用户名）。
    #[serde(default)]
    pub nickname: Option<String>,
    /// 是否允许打洞失败时经公共节点中继（默认 false = 仅直连，不占用公共节点带宽）。
    #[serde(default)]
    pub allow_relay: bool,
}

impl OnlineConfig {
    pub fn with_defaults() -> Self {
        Self {
            public_nodes: DEFAULT_PUBLIC_NODES.iter().map(|s| s.to_string()).collect(),
            advertise_motd: None,
            nickname: None,
            allow_relay: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_use_direct_only_and_live_nodes() {
        let cfg = OnlineConfig::with_defaults();
        assert!(!cfg.allow_relay, "默认仅直连");
        assert_eq!(cfg.public_nodes.len(), 2, "默认只内置两个社区节点");
        assert!(
            cfg.public_nodes
                .iter()
                .all(|n| n.contains("etnode.zkitefly.eu.org")),
            "默认节点应只含 zkitefly 两个"
        );
    }
}
