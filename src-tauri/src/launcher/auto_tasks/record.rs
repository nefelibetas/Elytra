//! 内置动作：记录 mod 清单 JSON（纯本地扫描，不触网）。
//! 输出覆盖制 `mods.json`（快照本次启动的 mod 及 sha1），便于换整合包/更新后回溯。

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;

use crate::downloader::hash;
use crate::game::updates::scan_mod_files;

/// 清单 schema 版本（后续加字段时递增）。
const SCHEMA: u32 = 1;

#[derive(Debug, Serialize)]
struct ModRecord {
    file: String,
    size: u64,
    modified: u64,
    /// 空字符串表示读取失败（文件损坏/权限）。
    sha1: String,
}

#[derive(Debug, Serialize)]
struct ModsSnapshot {
    schema: u32,
    recorded_at: u64,
    instance: String,
    dir: String,
    mods: Vec<ModRecord>,
}

/// 记录输出目录：`{config_dir}/records/{version_id}/`。
pub fn records_dir(config_dir: &Path, version_id: &str) -> PathBuf {
    config_dir.join("records").join(version_id)
}

/// 清单文件路径（崩溃包会顺带收录它）。
pub fn mods_json_path(config_dir: &Path, version_id: &str) -> PathBuf {
    records_dir(config_dir, version_id).join("mods.json")
}

/// 扫描 mods 目录并写入清单（覆盖制）。返回写入路径。
pub fn record_mods(
    config_dir: &Path,
    version_id: &str,
    game_dir: &Path,
) -> Result<PathBuf, String> {
    let mods_dir = game_dir.join("mods");
    let mods: Vec<ModRecord> = scan_mod_files(&mods_dir)
        .into_iter()
        .map(|f| {
            let path = mods_dir.join(&f.file_name);
            let sha1 = std::fs::File::open(&path)
                .ok()
                .and_then(hash::reader_sha1_hex)
                .unwrap_or_default();
            ModRecord {
                file: f.file_name,
                size: f.size,
                modified: f.last_modified,
                sha1,
            }
        })
        .collect();

    let snapshot = ModsSnapshot {
        schema: SCHEMA,
        recorded_at: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
        instance: version_id.to_string(),
        dir: mods_dir.display().to_string(),
        mods,
    };

    let out = mods_json_path(config_dir, version_id);
    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent).map_err(common_err_ctx!("创建记录目录失败"))?;
    }
    let text =
        serde_json::to_string_pretty(&snapshot).map_err(common_err_ctx!("序列化 mod 清单失败"))?;
    std::fs::write(&out, text).map_err(common_err_ctx!("写入 mod 清单失败"))?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn tmp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("elytra-autotasks-record-{name}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn record_writes_mods_with_sha1() {
        let config = tmp_dir("cfg");
        let game = tmp_dir("game");
        let mods = game.join("mods");
        fs::create_dir_all(&mods).unwrap();
        fs::write(mods.join("a.jar"), b"abc").unwrap();
        fs::write(mods.join("b.jar"), b"").unwrap();
        fs::write(mods.join("notes.txt"), b"ignore me").unwrap();

        let out = record_mods(&config, "1.20.1-fabric", &game).unwrap();
        assert_eq!(out, mods_json_path(&config, "1.20.1-fabric"));
        let text = fs::read_to_string(&out).unwrap();
        let v: serde_json::Value = serde_json::from_str(&text).unwrap();

        assert_eq!(v["schema"], 1);
        assert_eq!(v["instance"], "1.20.1-fabric");
        let mods = v["mods"].as_array().unwrap();
        assert_eq!(mods.len(), 2, "只记录 .jar，txt 忽略");
        assert_eq!(mods[0]["file"], "a.jar");
        assert_eq!(
            mods[0]["sha1"], "a9993e364706816aba3e25717850c26c9cd0d89d",
            "sha1 应为文件内容摘要"
        );
        assert_eq!(
            mods[1]["sha1"], "da39a3ee5e6b4b0d3255bfef95601890afd80709",
            "空文件 sha1"
        );
    }

    #[test]
    fn record_empty_mods_dir_still_writes() {
        let config = tmp_dir("cfg2");
        let game = tmp_dir("game2");
        let out = record_mods(&config, "inst", &game).unwrap();
        let v: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&out).unwrap()).unwrap();
        assert_eq!(v["mods"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn record_overwrites_previous() {
        let config = tmp_dir("cfg3");
        let game = tmp_dir("game3");
        fs::create_dir_all(game.join("mods")).unwrap();
        fs::write(game.join("mods/only.jar"), b"1").unwrap();
        record_mods(&config, "inst", &game).unwrap();
        fs::remove_file(game.join("mods/only.jar")).unwrap();
        record_mods(&config, "inst", &game).unwrap();
        let v: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(mods_json_path(&config, "inst")).unwrap())
                .unwrap();
        assert_eq!(v["mods"].as_array().unwrap().len(), 0, "覆盖制：无残留");
    }
}
