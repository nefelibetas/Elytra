use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub enum GcType {
    PreferZgc,
    PreferGenerationalZgc,
    StandardG1Gc,
    TunedG1Gc,
    Custom,
}

impl Default for GcType {
    fn default() -> Self {
        Self::TunedG1Gc
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub enum AuthMode {
    Offline {
        username: String,
    },
    Online {
        access_token: String,
        uuid: String,
        xuid: String,
    },
}

impl Default for AuthMode {
    fn default() -> Self {
        Self::Offline {
            username: String::new(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub enum MemoryMode {
    Auto,
    Custom(u32),
}

impl Default for MemoryMode {
    fn default() -> Self {
        Self::Auto
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LauncherConfig {
    pub memory: MemoryMode,

    pub window_width: u32,
    pub window_height: u32,

    pub jvm_args: Vec<String>,
    pub game_args: Vec<String>,

    pub pre_launch_cmd: String,
}

impl Default for LauncherConfig {
    fn default() -> Self {
        Self {
            memory: MemoryMode::default(),
            window_width: 1280,
            window_height: 720,
            jvm_args: vec![
                "-XX:-OmitStackTraceInFastThrow".into(),
                "-Djdk.lang.Process.allowAmbiguousCommands=True".into(),
                "-Dfml.ignoreInvalidMinecraftCertificates=True".into(),
                "-Dfml.ignorePatchDiscrepancies=True".into(),
            ],
            game_args: vec![],
            pre_launch_cmd: String::new(),
        }
    }
}
