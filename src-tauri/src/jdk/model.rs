use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct JavaEnv {
    pub path: String,
    pub version: String,
    pub vendor: String,
    pub arch: String,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct JavaConfig {
    pub active_jdk: Option<JavaEnv>,
    pub discovered_jdks: Vec<JavaEnv>,
}
