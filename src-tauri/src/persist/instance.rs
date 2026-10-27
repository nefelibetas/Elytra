//! 实例级配置读写：`versions/{id}/.elytra-instance.toml`。
//!
//! 按 section 独立生效：有 `[auto_tasks]` 用实例值，否则回退全局；有 `[launch]` 用实例启动快照，
//! 否则回退全局。两段都缺失时删除文件。

use std::path::{Path, PathBuf};

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

use crate::launcher::auto_tasks::ctx::INSTANCE_FILE;
use crate::launcher::auto_tasks::model::AutoTasks;
use crate::launcher::model::InstanceLaunchOptions;

/// 串行化实例文件的读改写（自动任务/启动参数可能由不同页面同时保存）。
static INSTANCE_LOCK: Mutex<()> = Mutex::new(());

/// 实例配置文件路径。
pub fn instance_path(mc_dir: &Path, version_id: &str) -> PathBuf {
    mc_dir.join("versions").join(version_id).join(INSTANCE_FILE)
}

#[derive(Debug, Default, Deserialize, Serialize)]
struct InstanceFile {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    auto_tasks: Option<AutoTasks>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    launch: Option<InstanceLaunchOptions>,
}

fn read_instance_file(path: &Path) -> Option<InstanceFile> {
    let text = std::fs::read_to_string(path).ok()?;
    toml::from_str::<InstanceFile>(&text).ok()
}

fn write_instance_file(path: &Path, file: &InstanceFile) -> Result<(), String> {
    if file.auto_tasks.is_none() && file.launch.is_none() {
        if path.exists() {
            std::fs::remove_file(path).map_err(common_err_ctx_key!(
                "errors.file.delete",
                "删除实例配置失败"
            ))?;
        }
        return Ok(());
    }
    let text = toml::to_string(file).map_err(common_err_ctx_key!(
        "errors.config.serializeFailed",
        "序列化实例配置失败"
    ))?;
    let tmp = path.with_file_name(format!("{INSTANCE_FILE}.tmp"));
    std::fs::write(&tmp, text)
        .map_err(common_err_ctx_key!("errors.file.write", "写入实例配置失败"))?;
    std::fs::rename(&tmp, path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        keyed_msg!("errors.config.write", "保存实例配置失败\n{e}")
    })
}

/// 读取实例自动任务；缺段/损坏返回 `None`（回退全局）。
pub fn read_auto_tasks(mc_dir: &Path, version_id: &str) -> Option<AutoTasks> {
    read_instance_file(&instance_path(mc_dir, version_id))?.auto_tasks
}

/// 写入实例自动任务；`None` = 删除该段并回退全局（保留启动参数段）。
pub fn write_auto_tasks(
    mc_dir: &Path,
    version_id: &str,
    auto_tasks: Option<&AutoTasks>,
) -> Result<(), String> {
    let _guard = INSTANCE_LOCK.lock();
    let path = instance_path(mc_dir, version_id);
    let mut file = read_instance_file(&path).unwrap_or_default();
    file.auto_tasks = auto_tasks.cloned();
    write_instance_file(&path, &file)
}

/// 读取实例启动参数；缺段/损坏返回 `None`（回退全局）。
pub fn read_launch(mc_dir: &Path, version_id: &str) -> Option<InstanceLaunchOptions> {
    read_instance_file(&instance_path(mc_dir, version_id))?.launch
}

/// 写入实例启动参数；`None` = 删除该段并回退全局（保留自动任务段）。
pub fn write_launch(
    mc_dir: &Path,
    version_id: &str,
    launch: Option<&InstanceLaunchOptions>,
) -> Result<(), String> {
    let _guard = INSTANCE_LOCK.lock();
    let path = instance_path(mc_dir, version_id);
    let mut file = read_instance_file(&path).unwrap_or_default();
    file.launch = launch.cloned();
    write_instance_file(&path, &file)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jdk::model::JdkSelection;
    use crate::launcher::auto_tasks::model::{
        AutoTaskAction, AutoTaskFailure, AutoTaskStep, BackupScope,
    };
    use crate::launcher::model::{GcType, MemoryMode, ProcessPriority, WindowSize};
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

    fn sample_launch() -> InstanceLaunchOptions {
        InstanceLaunchOptions {
            memory: MemoryMode::Custom(4096),
            min_memory: Some(1024),
            window_title: "Demo".into(),
            process_priority: ProcessPriority::High,
            window_size: WindowSize::Custom(1280, 720),
            disable_launch_wrapper: true,
            disable_lwjgl_unsafe_agent: true,
            pre_launch_memory_optimize: true,
            use_high_performance_gpu: true,
            game_language: "auto_zh_CN".into(),
            verify_ssl: false,
            jvm_args: vec!["-XX:+UseG1GC".into()],
            game_args: vec!["--demo".into()],
            gc_type: GcType::StandardG1Gc,
            java: JdkSelection::Manual {
                path: "C:\\Java\\jdk-21".into(),
            },
        }
    }

    #[test]
    fn missing_file_returns_none() {
        let mc = tmp_dir("missing");
        assert!(read_auto_tasks(&mc, "inst").is_none());
        assert!(read_launch(&mc, "inst").is_none());
        assert!(!instance_path(&mc, "inst").is_file());
    }

    #[test]
    fn write_then_read_roundtrip() {
        let mc = tmp_dir("roundtrip");
        write_auto_tasks(&mc, "inst", Some(&sample_auto_tasks())).unwrap();
        write_launch(&mc, "inst", Some(&sample_launch())).unwrap();
        assert_eq!(read_auto_tasks(&mc, "inst").unwrap(), sample_auto_tasks());
        let launch = read_launch(&mc, "inst").unwrap();
        assert_eq!(launch.memory, MemoryMode::Custom(4096));
        assert_eq!(launch.min_memory, Some(1024));
        assert_eq!(launch.jvm_args, vec!["-XX:+UseG1GC".to_string()]);
        assert!(matches!(launch.java, JdkSelection::Manual { .. }));
    }

    #[test]
    fn legacy_auto_tasks_only_file_still_works() {
        let mc = tmp_dir("legacy");
        fs::write(
            instance_path(&mc, "inst"),
            "[auto_tasks]\npre_launch = []\npost_exit = []\non_crash = []\n",
        )
        .unwrap();
        assert!(read_auto_tasks(&mc, "inst").is_some());
        assert!(read_launch(&mc, "inst").is_none());
    }

    #[test]
    fn clearing_one_section_keeps_the_other() {
        let mc = tmp_dir("sections");
        write_auto_tasks(&mc, "inst", Some(&sample_auto_tasks())).unwrap();
        write_launch(&mc, "inst", Some(&sample_launch())).unwrap();

        write_auto_tasks(&mc, "inst", None).unwrap();
        assert!(read_auto_tasks(&mc, "inst").is_none());
        assert!(read_launch(&mc, "inst").is_some());
        assert!(instance_path(&mc, "inst").is_file());

        write_launch(&mc, "inst", None).unwrap();
        assert!(!instance_path(&mc, "inst").exists());
    }

    #[test]
    fn write_none_on_empty_instance_removes_file() {
        let mc = tmp_dir("remove");
        write_auto_tasks(&mc, "inst", Some(&sample_auto_tasks())).unwrap();
        write_auto_tasks(&mc, "inst", None).unwrap();
        assert!(!instance_path(&mc, "inst").exists());
        assert!(read_auto_tasks(&mc, "inst").is_none());
        // 幂等
        write_auto_tasks(&mc, "inst", None).unwrap();
    }

    #[test]
    fn corrupt_file_falls_back_to_none() {
        let mc = tmp_dir("corrupt");
        fs::write(instance_path(&mc, "inst"), "auto_tasks = = broken").unwrap();
        assert!(read_auto_tasks(&mc, "inst").is_none());
        assert!(read_launch(&mc, "inst").is_none());
    }

    #[test]
    fn leaves_no_tmp_behind() {
        let mc = tmp_dir("tmp");
        write_launch(&mc, "inst", Some(&sample_launch())).unwrap();
        assert!(!instance_path(&mc, "inst")
            .with_file_name(format!("{INSTANCE_FILE}.tmp"))
            .exists());
    }
}
