use serde::{Deserialize, Serialize};

///  jdk信息
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct JdkInfo {
    pub path: String,
    pub version: String,
    pub vendor: String,
    pub bits: u8,
}

/// jdk配置, 含当前选中的jdk与已保存的jdk
#[derive(Debug, Serialize, Deserialize, Default)]
pub struct JdkConfig {
    #[serde(default)]
    pub jdk_selection: JdkSelection,
    pub discovered_jdks: Vec<JdkInfo>,
}

/// jdk选择模式：手动 + 自动, json扁平化处理
#[derive(Debug, Serialize, Deserialize, Default, Clone)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum JdkSelection {
    #[default]
    Auto,
    Manual {
        path: String,
    },
}
