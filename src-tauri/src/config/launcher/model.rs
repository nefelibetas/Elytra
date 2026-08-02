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
        /// 离线 UUID（按离线皮肤设置生成：匹配 Steve/Alex 模型或正版玩家真实 UUID）
        uuid: String,
    },
    Online {
        username: String,
        access_token: String,
        uuid: String,
        xuid: String,
    },
}

impl Default for AuthMode {
    fn default() -> Self {
        Self::Offline {
            username: String::new(),
            uuid: String::new(),
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

/// 游戏启动后启动器窗口的可见性行为。
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub enum LauncherVisibility {
    /// 保持不变（默认）。
    #[default]
    Keep,
    Minimize,
    Hide,
    /// 关闭启动器。
    Close,
}

/// 游戏进程优先级（Windows 进程优先级类）。
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub enum ProcessPriority {
    Low,
    /// 中（平衡），默认。
    #[default]
    Normal,
    High,
    RealTime,
}

/// 游戏窗口大小。
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub enum WindowSize {
    /// 默认（不传 --width/--height，由游戏自己决定）。
    #[default]
    Default,
    Custom(u32, u32),
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LauncherConfig {
    pub memory: MemoryMode,

    /// 游戏窗口标题；空 = 不改（不传 -Dminecraft.window.title）。
    #[serde(default)]
    pub window_title: String,

    #[serde(default)]
    pub launcher_visibility: LauncherVisibility,

    #[serde(default)]
    pub process_priority: ProcessPriority,

    #[serde(default)]
    pub window_size: WindowSize,

    /// 禁用 Java Launch Wrapper（老版本主类 net.minecraft.launchwrapper.Launch → 直接用 client.main.Main）。
    #[serde(default)]
    pub disable_launch_wrapper: bool,

    /// 禁用 LWJGL Unsafe Agent（不注入 -javaagent:lwjgl-unsafe-agent 类参数）。
    #[serde(default)]
    pub disable_lwjgl_unsafe_agent: bool,

    /// 启动游戏前进行内存优化（Windows 清理当前进程工作集）。
    #[serde(default)]
    pub pre_launch_memory_optimize: bool,

    /// 使用高性能显卡（Windows 注册表 DirectX UserGpuPreferences 指定独显）。
    #[serde(default = "default_true")]
    pub use_high_performance_gpu: bool,

    /// 游戏语言策略："auto_zh_CN" = 启动时自动设置中文（唯一选项），空 = 不干预。
    #[serde(default = "default_zh")]
    pub game_language: String,

    pub jvm_args: Vec<String>,
    pub game_args: Vec<String>,

    pub pre_launch_cmd: String,

    #[serde(default)]
    pub gc_type: GcType,
}

fn default_true() -> bool {
    true
}

fn default_zh() -> String {
    "auto_zh_CN".into()
}

impl Default for LauncherConfig {
    fn default() -> Self {
        Self {
            memory: MemoryMode::default(),
            window_title: String::new(),
            launcher_visibility: LauncherVisibility::default(),
            process_priority: ProcessPriority::default(),
            window_size: WindowSize::default(),
            disable_launch_wrapper: false,
            disable_lwjgl_unsafe_agent: false,
            pre_launch_memory_optimize: false,
            use_high_performance_gpu: true,
            game_language: "auto_zh_CN".into(),
            jvm_args: vec![
                "-XX:-OmitStackTraceInFastThrow".into(),
                "-Djdk.lang.Process.allowAmbiguousCommands=True".into(),
                "-Dfml.ignoreInvalidMinecraftCertificates=True".into(),
                "-Dfml.ignorePatchDiscrepancies=True".into(),
            ],
            game_args: vec![],
            pre_launch_cmd: String::new(),
            gc_type: GcType::default(),
        }
    }
}
