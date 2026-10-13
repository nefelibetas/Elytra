use serde::{Deserialize, Serialize};

/// 多节点并行连接（easytier 任一可用即可组网，天然主备；不可达自动跳过）：`tcp://node.invalid:11010`
/// 社区公共服务器（阿里云）、`tcp://node.invalid:11010` / `node.invalid:54321`
/// easytier 官方/公共节点、`https://etnode.zkitefly.eu.org/node{1,2}` 社区节点（HTTP discovery）；节点随时变动，用户可在设置改/清空。
pub const DEFAULT_PUBLIC_NODES: &[&str] = &[
    "tcp://node.invalid:11010",
    "tcp://node.invalid:11010",
    "tcp://node.invalid:54321",
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
}

impl OnlineConfig {
    pub fn with_defaults() -> Self {
        Self {
            public_nodes: DEFAULT_PUBLIC_NODES.iter().map(|s| s.to_string()).collect(),
            advertise_motd: None,
            nickname: None,
        }
    }
}
