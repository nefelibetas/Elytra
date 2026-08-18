pub mod assets;
pub mod extract;
pub mod fetcher;
pub mod hash;
pub mod library;
pub mod manifest;
pub mod mod_updates;
pub mod community;
pub mod mod_loader;
pub mod model;
pub mod progress;
pub mod reuse;
pub mod source;
pub mod version;
pub mod wiki_entry;

use std::path::Path;

/// 解析版本目录名：`versions/{id}` 冲突时追加 `_{n}`。
/// 目录存在但缺 client jar（上次下载中断）→ 复用目录重新下载，避免残留残缺版本。
pub fn resolve_version_dir(mc: &Path, version_id: &str) -> (String, std::path::PathBuf) {
    let base = mc.join("versions").join(version_id);
    if !base.exists() {
        return (version_id.to_string(), base);
    }
    if !base.join(format!("{version_id}.jar")).exists() {
        return (version_id.to_string(), base);
    }
    for n in 1.. {
        let name = format!("{}_{}", version_id, n);
        let dir = mc.join("versions").join(&name);
        if !dir.exists() {
            return (name, dir);
        }
    }
    unreachable!()
}
