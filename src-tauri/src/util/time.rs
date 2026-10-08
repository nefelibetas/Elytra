//! 文件元数据时间转换。

/// 文件最后修改时间（unix 秒），读取失败回退 0（不存在/无权限/时钟异常均不致命）。
/// version.rs / mod_updates(identify,scan) 三处共用，消除重复的 metadata 时间链。
pub fn modified_secs(meta: &std::fs::Metadata) -> u64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
