use serde::{Deserialize, Serialize};

use crate::jdk::model::JdkSelection;

#[derive(Debug, Clone, Default, Serialize, Deserialize, ts_rs::TS)]
#[ts(export_to = "launcher/")]
pub enum GcType {
    PreferZgc,
    PreferGenerationalZgc,
    StandardG1Gc,
    #[default]
    TunedG1Gc,
    Custom,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum AuthMode {
    Offline {
        username: String,
        /// 离线 UUID（按离线皮肤设置生成）
        uuid: String,
    },
    Online {
        username: String,
        access_token: String,
        uuid: String,
        xuid: String,
    },
    /// 第三方登录（authlib-injector / 皮肤站 / Yggdrasil 兼容服务器）；api_url = 验证服务器根地址。
    External {
        username: String,
        access_token: String,
        uuid: String,
        api_url: String,
    },
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export_to = "launcher/")]
pub enum MemoryMode {
    #[default]
    Auto,
    Custom(u32),
}

/// 游戏进程优先级（Windows 进程优先级类）。
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize, ts_rs::TS)]
#[ts(export_to = "launcher/")]
pub enum ProcessPriority {
    Low,
    /// 中（平衡），默认。
    #[default]
    Normal,
    High,
    RealTime,
}

#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize, ts_rs::TS)]
#[ts(export_to = "launcher/")]
pub enum WindowSize {
    /// 默认（不传 --width/--height，由游戏自己决定）。
    #[default]
    Default,
    Custom(u32, u32),
}

#[derive(Debug, Serialize, Deserialize, ts_rs::TS)]
#[ts(export_to = "launcher/")]
pub struct LaunchOptions {
    pub memory: MemoryMode,

    /// 最小堆 -Xms（MB）；None = 不设置。仅 Custom 模式生效（Auto 的 -Xmx 动态推测）。
    #[serde(default)]
    pub min_memory: Option<u32>,

    /// 游戏窗口标题；空 = 不改（不传 -Dminecraft.window.title）。
    #[serde(default)]
    pub window_title: String,

    #[serde(default)]
    pub process_priority: ProcessPriority,

    #[serde(default)]
    pub window_size: WindowSize,

    /// 禁用 Java Launch Wrapper（老版本 launchwrapper 主类 → client.main.Main）。
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

    /// 服务器直连：快速进服（--server/--port 追加到游戏参数末尾）。None = 不指定。
    #[serde(default)]
    pub server: Option<String>,

    /// 服务器直连端口（配合 server 使用；None = 不传 --port）。
    #[serde(default)]
    pub port: Option<u16>,

    /// 正版登录时验证 SSL 证书（默认开启；关闭会暴露 token 到中间人风险）。
    #[serde(default = "default_true")]
    pub verify_ssl: bool,

    pub jvm_args: Vec<String>,
    pub game_args: Vec<String>,

    /// 自动任务（pre_launch / on_crash / post_exit）。实例文件存在时由实例配置覆盖。
    #[serde(default)]
    pub auto_tasks: crate::launcher::auto_tasks::model::AutoTasks,

    #[serde(default)]
    pub gc_type: GcType,
}

fn default_true() -> bool {
    true
}

fn default_zh() -> String {
    "auto_zh_CN".into()
}

impl Default for LaunchOptions {
    fn default() -> Self {
        Self {
            memory: MemoryMode::default(),
            min_memory: None,
            window_title: String::new(),
            process_priority: ProcessPriority::default(),
            window_size: WindowSize::default(),
            disable_launch_wrapper: false,
            disable_lwjgl_unsafe_agent: false,
            pre_launch_memory_optimize: false,
            use_high_performance_gpu: true,
            game_language: "auto_zh_CN".into(),
            server: None,
            port: None,
            verify_ssl: true,
            jvm_args: vec![
                "-XX:-OmitStackTraceInFastThrow".into(),
                "-Djdk.lang.Process.allowAmbiguousCommands=True".into(),
                "-Dfml.ignoreInvalidMinecraftCertificates=True".into(),
                "-Dfml.ignorePatchDiscrepancies=True".into(),
            ],
            game_args: vec![],
            auto_tasks: Default::default(),
            gc_type: GcType::default(),
        }
    }
}

/// 实例级启动参数快照：只包含“可以按实例覆盖”的字段。
///
/// 不含 `server`/`port`（版本设置里的服务器直连独立存储、优先级更高）与 `auto_tasks`（独立 section）；
/// Java 选择随实例快照一起保存。
#[derive(Debug, Clone, Serialize, Deserialize, ts_rs::TS)]
#[ts(export_to = "launcher/")]
pub struct InstanceLaunchOptions {
    #[serde(default)]
    pub memory: MemoryMode,
    #[serde(default)]
    pub min_memory: Option<u32>,
    #[serde(default)]
    pub window_title: String,
    #[serde(default)]
    pub process_priority: ProcessPriority,
    #[serde(default)]
    pub window_size: WindowSize,
    #[serde(default)]
    pub disable_launch_wrapper: bool,
    #[serde(default)]
    pub disable_lwjgl_unsafe_agent: bool,
    #[serde(default)]
    pub pre_launch_memory_optimize: bool,
    #[serde(default = "default_true")]
    pub use_high_performance_gpu: bool,
    #[serde(default = "default_zh")]
    pub game_language: String,
    #[serde(default = "default_true")]
    pub verify_ssl: bool,
    #[serde(default)]
    pub jvm_args: Vec<String>,
    #[serde(default)]
    pub game_args: Vec<String>,
    #[serde(default)]
    pub gc_type: GcType,
    #[serde(default)]
    pub java: JdkSelection,
}

impl InstanceLaunchOptions {
    /// 从全局配置生成实例快照（启用“单独配置”时的初始值）。
    pub fn from_launch_options(options: &LaunchOptions, java: JdkSelection) -> Self {
        Self {
            memory: options.memory.clone(),
            min_memory: options.min_memory,
            window_title: options.window_title.clone(),
            process_priority: options.process_priority,
            window_size: options.window_size,
            disable_launch_wrapper: options.disable_launch_wrapper,
            disable_lwjgl_unsafe_agent: options.disable_lwjgl_unsafe_agent,
            pre_launch_memory_optimize: options.pre_launch_memory_optimize,
            use_high_performance_gpu: options.use_high_performance_gpu,
            game_language: options.game_language.clone(),
            verify_ssl: options.verify_ssl,
            jvm_args: options.jvm_args.clone(),
            game_args: options.game_args.clone(),
            gc_type: options.gc_type.clone(),
            java,
        }
    }

    /// 覆盖全局启动参数（Java 由 `env` 单独取用）。
    pub fn apply_to(&self, options: &mut LaunchOptions) {
        options.memory = self.memory.clone();
        options.min_memory = self.min_memory;
        options.window_title = self.window_title.clone();
        options.process_priority = self.process_priority;
        options.window_size = self.window_size;
        options.disable_launch_wrapper = self.disable_launch_wrapper;
        options.disable_lwjgl_unsafe_agent = self.disable_lwjgl_unsafe_agent;
        options.pre_launch_memory_optimize = self.pre_launch_memory_optimize;
        options.use_high_performance_gpu = self.use_high_performance_gpu;
        options.game_language = self.game_language.clone();
        options.verify_ssl = self.verify_ssl;
        options.jvm_args = self.jvm_args.clone();
        options.game_args = self.game_args.clone();
        options.gc_type = self.gc_type.clone();
    }
}
