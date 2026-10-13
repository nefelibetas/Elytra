//! 实例级配置读写：`versions/{id}/.elytra-instance.toml`，一期只承载 `[auto_tasks]`
//! （其余启动选项仍走全局 section）。**文件不存在 = 用全局**；存在则完全采用自己的自动任务。

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::launcher::auto_tasks::ctx::INSTANCE_FILE;
use crate::launcher::auto_tasks::model::AutoTasks;

/// 实例配置文件路径。
pub fn instance_path(mc_dir: &Path, version_id: &str) -> PathBuf {
    mc_dir.join("versions").join(version_id).join(INSTANCE_FILE)
}

#[derive(Deserialize)]
struct InstanceFile {
    #[serde(default)]
    auto_tasks: AutoTasks,
}

#[derive(Serialize)]
struct InstanceFileRef<'a> {
    auto_tasks: &'a AutoTasks,
}

/// 读取实例自动任务；文件缺失/损坏返回 `None`（回退全局）。
pub fn read_auto_tasks(mc_dir: &Path, version_id: &str) -> Option<AutoTasks> {
    let text = std::fs::read_to_string(instance_path(mc_dir, version_id)).ok()?;
    toml::from_str::<InstanceFile>(&text).ok().map(|f| f.auto_tasks)
}

/// 写入实例自动任务；`None` = 删除实例文件（该实例回退全局）。
pub fn write_auto_tasks(mc_dir: &Path, version_id: &str, auto_tasks: Option<&AutoTasks>) -> Result<(), String> {
    let path = instance_path(mc_dir, version_id);
    let Some(auto_tasks) = auto_tasks else {
        if path.exists() {
            std::fs::remove_file(&path).map_err(common_err_ctx!("删除实例配置失败"))?;
        }
        return Ok(());
    };
    let text = toml::to_string(&InstanceFileRef { auto_tasks })
        .map_err(common_err_ctx!("序列化实例配置失败"))?;
    let tmp = path.with_file_name(format!("{INSTANCE_FILE}.tmp"));
    std::fs::write(&tmp, text).map_err(common_err_ctx!("写入实例配置失败"))?;
    std::fs::rename(&tmp, &path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        format!("保存实例配置失败\n{e}")
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::launcher::auto_tasks::model::{BackupScope, AutoTaskAction, AutoTaskStep, AutoTaskFailure};
    use std::fs;

    fn tmp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("elytra-persist-instance-{name}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("versions/inst")).unwrap();
        dir
    }

    fn sample_auto_tasks() -> AutoTasks {
        AutoTasks {
            pre_launch: vec![AutoTaskStep {
                enabled: true,
                action: AutoTaskAction::BackupSaves {
                    scope: BackupScope::All,
                    keep: 3,
                },
            }],
            post_exit: vec![AutoTaskStep {
                enabled: true,
                action: AutoTaskAction::RecordMods,
            }],
            on_crash: vec![AutoTaskStep {
                enabled: true,
                action: AutoTaskAction::PackCrashLogs,
            }],
            pre_on_failure: AutoTaskFailure::Continue,
        }
    }

    #[test]
    fn missing_file_returns_none() {
        let mc = tmp_dir("missing");
        assert!(read_auto_tasks(&mc, "inst").is_none());
        assert!(!instance_path(&mc, "inst").is_file());
    }

    #[test]
    fn write_then_read_roundtrip() {
        let mc = tmp_dir("roundtrip");
        let auto_tasks = sample_auto_tasks();
        write_auto_tasks(&mc, "inst", Some(&auto_tasks)).unwrap();
        assert!(instance_path(&mc, "inst").is_file());
        assert_eq!(read_auto_tasks(&mc, "inst").unwrap(), auto_tasks);
        // 文件应落在实例目录内
        assert!(instance_path(&mc, "inst").exists());
    }

    #[test]
    fn write_none_removes_file() {
        let mc = tmp_dir("remove");
        write_auto_tasks(&mc, "inst", Some(&sample_auto_tasks())).unwrap();
        write_auto_tasks(&mc, "inst", None).unwrap();
        assert!(!instance_path(&mc, "inst").exists());
        assert!(read_auto_tasks(&mc, "inst").is_none());
        // 幂等：再删一次也不报错
        write_auto_tasks(&mc, "inst", None).unwrap();
    }

    #[test]
    fn corrupt_file_falls_back_to_none() {
        let mc = tmp_dir("corrupt");
        fs::write(instance_path(&mc, "inst"), "auto_tasks = = broken").unwrap();
        assert!(read_auto_tasks(&mc, "inst").is_none(), "损坏不应 panic，按未配置处理");
    }

    #[test]
    fn leaves_no_tmp_behind() {
        let mc = tmp_dir("tmp");
        write_auto_tasks(&mc, "inst", Some(&sample_auto_tasks())).unwrap();
        assert!(!instance_path(&mc, "inst").with_file_name(".elytra-instance.toml.tmp").exists());
    }
}
