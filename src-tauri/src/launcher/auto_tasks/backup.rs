//! 内置动作：备份存档到 `versions/{id}/back/saves/{时间戳}/`，按时间戳目录保留最近 `keep` 份；排除游戏运行期锁文件与临时文件。

use std::path::{Path, PathBuf};

use super::model::BackupScope;

/// 备份根目录：`versions/{id}/back/saves/`。
pub fn backup_root(game_dir: &Path) -> PathBuf {
    game_dir.join("back").join("saves")
}

/// 源存档目录：`versions/{id}/saves/`。
pub fn saves_dir(game_dir: &Path) -> PathBuf {
    game_dir.join("saves")
}

/// 备份结果（供进度与提示）。
#[derive(Debug, Clone, PartialEq)]
pub struct BackupOutcome {
    pub worlds: usize,
    pub files: usize,
    /// 本次清理掉的旧 timepoint 数量。
    pub removed: usize,
    pub dir: PathBuf,
}

/// 排除项：锁文件与临时文件。
fn is_excluded(name: &str) -> bool {
    name == "session.lock" || name.ends_with(".lock") || name.ends_with(".tmp")
}

/// `saves/` 下的全部世界名（排序）。
pub fn all_worlds(game_dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = crate::util::dir_entries(&saves_dir(game_dir), true)
        .into_iter()
        .map(|(name, _, _)| name)
        .collect();
    names.sort();
    names
}

/// 本次要备份的世界（按范围过滤）。
fn selected_worlds(game_dir: &Path, scope: &BackupScope) -> Vec<String> {
    let mut worlds = all_worlds(game_dir);
    if let BackupScope::Selected { worlds: want } = scope {
        worlds.retain(|w| want.iter().any(|s| s == w));
    }
    worlds
}

/// 递归统计待复制文件数（跳过排除项），用于进度分母。
fn count_files(dir: &Path) -> usize {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    let mut n = 0;
    for e in entries.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        if is_excluded(&name) {
            continue;
        }
        if e.path().is_dir() {
            n += count_files(&e.path());
        } else {
            n += 1;
        }
    }
    n
}

/// 递归复制目录树（跳过排除项），每复制一个文件回调一次进度。
fn copy_tree(
    src: &Path,
    dst: &Path,
    copied: &mut usize,
    total: usize,
    on_step: &mut dyn FnMut(usize, usize),
) -> Result<(), String> {
    std::fs::create_dir_all(dst).map_err(common_err_ctx_key!(
        "errors.file.createDir",
        "创建备份子目录失败"
    ))?;
    let entries = std::fs::read_dir(src)
        .map_err(common_err_ctx_key!("errors.file.read", "读取存档目录失败"))?;
    for e in entries.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        if is_excluded(&name) {
            continue;
        }
        let from = e.path();
        let to = dst.join(&name);
        if from.is_dir() {
            copy_tree(&from, &to, copied, total, on_step)?;
        } else {
            std::fs::copy(&from, &to)
                .map_err(common_err_ctx_key!("errors.file.copy", "复制存档文件失败"))?;
            *copied += 1;
            on_step(*copied, total);
        }
    }
    Ok(())
}

/// 清理旧 timepoint：只保留按名字（= 时间）排序后最新的 `keep` 份。返回删除数量。
fn cleanup_old(root: &Path, keep: u32) -> usize {
    let keep = keep.max(1) as usize;
    let mut dirs: Vec<String> = crate::util::dir_entries(root, true)
        .into_iter()
        .map(|(name, _, _)| name)
        .filter(|name| !name.ends_with(".tmp"))
        .collect();
    dirs.sort();
    let mut removed = 0;
    while dirs.len() > keep {
        let victim = dirs.remove(0);
        if std::fs::remove_dir_all(root.join(&victim)).is_ok() {
            removed += 1;
        }
    }
    removed
}

/// 备份选定世界到新的 timepoint 目录，随后清理超出的旧份数；`on_step(done, total)` 每复制完一个文件回调（total=0 表示无文件）。
pub fn backup_saves(
    game_dir: &Path,
    scope: &BackupScope,
    keep: u32,
    on_step: &mut dyn FnMut(usize, usize),
) -> Result<BackupOutcome, String> {
    let saves = saves_dir(game_dir);
    if !saves.is_dir() {
        bail_key!(
            "errors.launch.savesDirMissing",
            "未发现存档目录\n{}",
            saves.display()
        );
    }
    let worlds = selected_worlds(game_dir, scope);
    if worlds.is_empty() {
        bail_key!(
            "errors.launch.noWorldsToBackup",
            "没有可备份的世界\n{}（若存档在全局 saves/，说明该实例未启用版本隔离）",
            saves.display()
        );
    }
    let total: usize = worlds.iter().map(|w| count_files(&saves.join(w))).sum();
    let root = backup_root(game_dir);
    std::fs::create_dir_all(&root).map_err(common_err_ctx_key!(
        "errors.file.createDir",
        "创建备份目录失败"
    ))?;

    let stamp = crate::util::utc_stamp();
    let tmp = root.join(format!("{stamp}.tmp"));
    let final_dir = root.join(&stamp);
    if tmp.exists() {
        let _ = std::fs::remove_dir_all(&tmp);
    }
    if final_dir.exists() {
        // 同一秒内的第二次备份：直接复用该 timepoint（先清空，语义 = 覆盖该时刻）
        let _ = std::fs::remove_dir_all(&final_dir);
    }

    let mut copied = 0usize;
    let mut write_all = || -> Result<(), String> {
        for world in &worlds {
            copy_tree(
                &saves.join(world),
                &tmp.join(world),
                &mut copied,
                total,
                on_step,
            )?;
        }
        Ok(())
    };
    if let Err(e) = write_all() {
        let _ = std::fs::remove_dir_all(&tmp);
        return Err(e);
    }
    std::fs::rename(&tmp, &final_dir).map_err(|e| {
        let _ = std::fs::remove_dir_all(&tmp);
        keyed_msg!("errors.launch.backupFinishFailed", "完成备份失败\n{e}")
    })?;

    let removed = cleanup_old(&root, keep);
    Ok(BackupOutcome {
        worlds: worlds.len(),
        files: copied,
        removed,
        dir: final_dir,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn tmp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("elytra-autotasks-backup-{name}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write(path: &Path, content: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, content).unwrap();
    }

    fn seed_world(game_dir: &Path, world: &str) {
        write(&saves_dir(game_dir).join(world).join("level.dat"), "lvl");
        write(
            &saves_dir(game_dir).join(world).join("region/r.0.0.mca"),
            "mcа",
        );
        write(
            &saves_dir(game_dir).join(world).join("session.lock"),
            "lock",
        );
    }

    #[test]
    fn backup_excludes_session_lock() {
        let game = tmp_dir("excl");
        seed_world(&game, "world");
        let out = backup_saves(&game, &BackupScope::All, 5, &mut |_, _| {}).unwrap();
        assert_eq!(out.worlds, 1);
        assert_eq!(
            out.files, 2,
            "level.dat + region 文件，session.lock 应被排除"
        );
        assert!(!out.dir.join("world/session.lock").exists());
        assert!(out.dir.join("world/level.dat").exists());
        assert!(out.dir.join("world/region/r.0.0.mca").exists());
    }

    #[test]
    fn backup_scope_selected_skips_others() {
        let game = tmp_dir("scope");
        seed_world(&game, "keep");
        seed_world(&game, "skip");
        let scope = BackupScope::Selected {
            worlds: vec!["keep".into()],
        };
        let out = backup_saves(&game, &scope, 5, &mut |_, _| {}).unwrap();
        assert_eq!(out.worlds, 1);
        assert!(out.dir.join("keep/level.dat").exists());
        assert!(!out.dir.join("skip").exists());
    }

    #[test]
    fn backup_missing_saves_is_error() {
        let game = tmp_dir("missing");
        let err = backup_saves(&game, &BackupScope::All, 5, &mut |_, _| {}).unwrap_err();
        assert!(
            err.starts_with("@i18n:errors.launch.savesDirMissing"),
            "错误首行应为 i18n key: {err}"
        );
    }

    #[test]
    fn backup_reports_progress_to_total() {
        let game = tmp_dir("progress");
        seed_world(&game, "world");
        let mut last = (0usize, 0usize);
        backup_saves(&game, &BackupScope::All, 5, &mut |done, total| {
            last = (done, total);
        })
        .unwrap();
        assert_eq!(last, (2, 2), "进度终点应为 (文件数, 文件数)");
    }

    #[test]
    fn cleanup_keeps_newest_n_and_ignores_tmp() {
        let root = tmp_dir("cleanup");
        for name in [
            "20260101-000000",
            "20260102-000000",
            "20260103-000000",
            "20260104-000000",
            "20260105-000000",
            "20260106-000000",
        ] {
            fs::create_dir_all(root.join(name)).unwrap();
        }
        fs::create_dir_all(root.join("20260107-000000.tmp")).unwrap();
        let removed = cleanup_old(&root, 5);
        assert_eq!(removed, 1);
        assert!(!root.join("20260101-000000").exists(), "最旧的应被删除");
        assert!(root.join("20260106-000000").exists());
        assert!(root.join("20260107-000000.tmp").exists(), ".tmp 不参与清理");
    }

    #[test]
    fn backup_produces_tmp_free_result() {
        let game = tmp_dir("nottmp");
        seed_world(&game, "world");
        backup_saves(&game, &BackupScope::All, 5, &mut |_, _| {}).unwrap();
        let leftovers: Vec<String> = crate::util::dir_entries(&backup_root(&game), true)
            .into_iter()
            .map(|(n, _, _)| n)
            .filter(|n| n.ends_with(".tmp"))
            .collect();
        assert!(leftovers.is_empty(), "成功后不应残留 .tmp：{leftovers:?}");
    }

    #[test]
    fn all_worlds_sorted() {
        let game = tmp_dir("sorted");
        for w in ["bworld", "aworld", "cworld"] {
            fs::create_dir_all(saves_dir(&game).join(w)).unwrap();
        }
        assert_eq!(all_worlds(&game), vec!["aworld", "bworld", "cworld"]);
    }
}
