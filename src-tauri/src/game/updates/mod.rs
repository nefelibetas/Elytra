//! 本地 Mod 扫描与更新检查（Modrinth 官方 API）。
//! 链路：mod jar → sha1 → `/v2/version_file/{sha1}` 识别项目 → `/v2/project/{id}/version`
//! （按 MC 版本 + loader 过滤）取最新版，版本号不同即可更新。
//! 仅支持 Modrinth；CurseForge 无公开 hash 查询通道，命中不了静默跳过。
//! 子模块：`scan` 目录扫描、`identify` 项目识别与元数据补全、`update` 更新/替换/删除。

mod identify;
mod scan;
mod update;

pub use identify::{
    identify_one_file, list_dir_detailed, list_mods_detailed, ModFileDetail, ModListResult,
    ResourceListResult,
};
pub use scan::{scan_dir_files, scan_mod_files, LocalModFile, ScanFilter};
pub use update::{
    check_updates, delete_mod_file, delete_resource_file, toggle_mod_file, update_mod_file,
    ModUpdate,
};
