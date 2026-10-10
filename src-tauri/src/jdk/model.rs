use serde::{Deserialize, Serialize};

///  jdk信息
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct JdkInfo {
    pub path: String,
    pub version: String,
    pub vendor: String,
    pub bits: u8,
}

/// JDK 选择配置（`jdk` section）：当前选中的 JDK。
#[derive(Debug, Serialize, Deserialize, Default)]
pub struct JdkOptions {
    #[serde(default)]
    pub selection: JdkSelection,
}

/// 本机 JDK 扫描缓存（`jdk_scanned` section）：扫描/安装发现的 JDK 列表。
/// 由后端扫描/安装维护，前端只读。
#[derive(Debug, Serialize, Deserialize, Default)]
pub struct JdkScanCache {
    pub jdks: Vec<JdkInfo>,
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
