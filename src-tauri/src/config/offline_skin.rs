use serde::{Deserialize, Serialize};

/// 离线皮肤类型（对齐 PCL2 的五种离线皮肤设置）
#[derive(Serialize, Deserialize, Default, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum OfflineSkinType {
    /// 随机：按用户名哈希确定性选择 Steve/Alex
    #[default]
    Random,
    Steve,
    Alex,
    /// 正版玩家：显示指定正版玩家名的皮肤
    Player,
    /// 自定义：本地皮肤文件
    Custom,
}

/// 自定义皮肤模型（仅对 3D 渲染有意义，头像头部纹理相同）
#[derive(Serialize, Deserialize, Default, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum OfflineSkinModel {
    #[default]
    Steve,
    Alex,
    /// 启动器决定（暂按 Steve 处理）
    Auto,
}

/// 离线皮肤配置
#[derive(Serialize, Deserialize, Default, Clone, PartialEq)]
pub struct OfflineSkinConfig {
    pub skin_type: OfflineSkinType,
    /// skin_type = Player 时：正版玩家名
    pub player_name: String,
    /// skin_type = Custom 时：本地皮肤 PNG 路径
    pub custom_path: String,
    /// skin_type = Custom 时：模型
    pub custom_model: OfflineSkinModel,
}
