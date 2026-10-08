//! ModLoader 安装的阶段事件与钩子类型。

/// ModLoader 安装的阶段事件（runner 钩子：推进步骤状态/置完成标志）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoaderEvent {
    /// 合并 JSON 已写入
    ConfigDone,
    /// 本体（fabric libs / installer→jar）已完成
    LibsDone,
}

/// 阶段钩子：安装过程中在对应事件点同步调用（非 async，直接闭包）。
/// Send + Sync：钩子闭包会穿过 async task 边界（跨线程）。
pub type LoaderHook<'a> = &'a (dyn Fn(LoaderEvent) + Send + Sync);
