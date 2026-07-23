use crate::config::jdk::model::JavaConfig;

pub fn read() -> JavaConfig {
    crate::persist::read("jdk").unwrap_or_default()
}

pub fn save(config: &JavaConfig) -> Result<(), String> {
    crate::persist::save("jdk", config).map_err(|e| e.to_string())
}
