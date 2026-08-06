use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct JdkInfo {
    pub path: String,
    pub version: String,
    pub vendor: String,
    pub bits: u8,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct JdkConfig {
    #[serde(default)]
    pub jdk_selection: JdkSelection,
    pub discovered_jdks: Vec<JdkInfo>,
}

#[derive(Debug, Serialize, Deserialize, Default, Clone)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum JdkSelection {
    #[default]
    Auto,
    Manual {
        path: String,
    },
}
