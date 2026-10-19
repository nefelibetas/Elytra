//! ── 安装任务持久化：任务快照 SQLite 存储，崩溃/重启不丢任务 ──
//! "恢复" = 用快照 JobSpec 重跑（下载任务天然幂等：文件级 sha1 + 已存在跳过 + 残缺目录复用）；生命周期：spawn 写 running → 终态经 TaskManager::fail/cancel 收口 → 崩溃残留 running → 启动 task::init() 标 interrupted。
//! 库文件：`config_dir()/tasks.db`（Win/Linux exe 旁 `Elytra/`，macOS `~/Library/Application Support/Elytra/`）。

use std::sync::LazyLock;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{Row, SqlitePool};

use crate::task::model::{DownloadFilesTask, DownloadTask, ModpackInstallSpec};
use crate::task::TaskStatus;
use crate::util::{block_on, config_dir};

/// 任务恢复凭证：重建任务所需的最小信息（各任务一）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum JobSpec {
    Download(DownloadTask),
    Files(DownloadFilesTask),
    Repair {
        version_dir_name: String,
    },
    InstallJdk {
        major: u32,
    },
    /// 整合包安装（单卡片全流程；恢复 = 重跑整卡，各环节幂等）
    ModpackInstall(ModpackInstallSpec),
}

impl JobSpec {
    /// 任务类型描述（前端恢复询问展示）。
    pub fn kind_label(&self) -> &'static str {
        match self {
            JobSpec::Download(_) => "版本下载/安装",
            JobSpec::Files(_) => "文件下载",
            JobSpec::Repair { .. } => "版本补全",
            JobSpec::InstallJdk { .. } => "JDK 安装",
            JobSpec::ModpackInstall(_) => "整合包安装",
        }
    }
}

/// 任务快照状态（与 TaskStatus serde 一致的小写字符串）。
fn status_str(status: TaskStatus) -> &'static str {
    match status {
        TaskStatus::Pending => "pending",
        TaskStatus::Running => "running",
        TaskStatus::Done => "done",
        TaskStatus::Error => "error",
        TaskStatus::Cancelled => "cancelled",
        TaskStatus::Paused => "paused",
    }
}

/// 供前端展示的中断任务条目。
#[derive(Debug, Clone, Serialize)]
pub struct InterruptedJob {
    pub id: String,
    pub name: String,
    pub kind: String,
    /// 快照最后更新时间（unix 秒）
    pub updated_at: i64,
}

fn db_path() -> std::path::PathBuf {
    config_dir().join("tasks.db")
}

/// 同步执行异步 DB 操作（复用 util::block_on；低频、阻塞毫秒级）；block_on 内不得再嵌套 block_on（tokio 禁止 runtime 内启动 runtime）。
fn pool() -> Result<&'static SqlitePool, String> {
    POOL.as_ref().map_err(|e| {
        // 池初始化失败仅留痕一次；DB 不可用不影响任务执行，仅停用快照/崩溃恢复。
        static LOGGED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
        if !LOGGED.swap(true, std::sync::atomic::Ordering::Relaxed) {
            eprintln!("[task-store] 任务快照数据库不可用，进度快照与崩溃恢复停用: {e}");
        }
        e.clone()
    })
}

static POOL: LazyLock<Result<SqlitePool, String>> = LazyLock::new(|| {
    let path = db_path();
    block_on(async {
        // 直接以路径连接（Windows 绝对路径用 URL 形式会被误拆 host/盘符）
        let opts = SqliteConnectOptions::new()
            .filename(&path)
            .create_if_missing(true)
            // 多连接并发写：sqlite 文件锁等待而非立即 BUSY 报错
            .busy_timeout(std::time::Duration::from_secs(5));
        let pool = SqlitePoolOptions::new()
            // sqlx 陷阱：单连接 + 查询超时取消时连接可能不回池 → 池耗尽后 acquire 超时静默失败；多连接稀释风险，acquire 显式超时防无限等待。
            .max_connections(4)
            .acquire_timeout(std::time::Duration::from_secs(2))
            .connect_with(opts)
            .await
            .map_err(common_err_ctx!("打开任务数据库失败"))?;
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS install_jobs (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                spec_json TEXT NOT NULL,
                status TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            )",
        )
        .execute(&pool)
        .await
        .map_err(common_err_ctx!("初始化任务表失败"))?;
        Ok::<SqlitePool, String>(pool)
    })
});

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// 写入任务快照（INSERT OR REPLACE），spawn 时调用；DB 故障静默忽略。
pub fn save_snapshot(id: &str, name: &str, spec: &JobSpec, status: TaskStatus) {
    let Ok(pool) = pool() else {
        return;
    };
    let spec_json = match serde_json::to_string(spec) {
        Ok(s) => s,
        Err(_) => return,
    };
    let st = status_str(status);
    let ts = now();
    let id_owned = id.to_string();
    let name_owned = name.to_string();
    block_on(async move {
        // DB 写带 2s 超时（同 update_status）：sqlite 锁异常时静默放弃
        let _ = tokio::time::timeout(
            std::time::Duration::from_secs(2),
            sqlx::query(
                "INSERT OR REPLACE INTO install_jobs (id, name, spec_json, status, created_at, updated_at)
                 VALUES (?, ?, ?, ?, ?, ?)",
            )
            .bind(id_owned)
            .bind(name_owned)
            .bind(spec_json)
            .bind(st)
            .bind(ts)
            .bind(ts)
            .execute(pool),
        )
        .await;
    });
}

/// 更新快照状态（TaskManager::fail/cancel 调用）；DB 写 2s 超时，异常静默放弃不阻塞 cancel/fail。
pub fn update_status(id: &str, status: TaskStatus) {
    let Ok(pool) = pool() else {
        return;
    };
    let st = status_str(status);
    let ts = now();
    let id_owned = id.to_string();
    block_on(async move {
        let _ = tokio::time::timeout(
            std::time::Duration::from_secs(2),
            sqlx::query("UPDATE install_jobs SET status = ?, updated_at = ? WHERE id = ?")
                .bind(st)
                .bind(ts)
                .bind(id_owned)
                .execute(pool),
        )
        .await;
    });
}

/// 启动时把残留 running 标为 interrupted，供 `list_interrupted_tasks` 拉取询问恢复。
pub fn mark_crashed_as_interrupted() {
    let Ok(pool) = pool() else {
        return;
    };
    let ts = now();
    block_on(async move {
        // DB 写带 2s 超时（同 update_status）：启动路径不被 sqlite 锁卡住
        let _ = tokio::time::timeout(
            std::time::Duration::from_secs(2),
            sqlx::query(
                "UPDATE install_jobs SET status = 'interrupted', updated_at = ? WHERE status = 'running'",
            )
            .bind(ts)
            .execute(pool),
        )
        .await;
    });
}

/// 列出中断（running/interrupted/paused）快照，按更新时间倒序；rows 先物化出 block_on（不嵌套 runtime）。
pub fn list_interrupted() -> Vec<InterruptedJob> {
    let Ok(pool) = pool() else {
        return Vec::new();
    };
    let rows: Vec<(String, String, i64)> = block_on(async move {
        let rows = sqlx::query(
            "SELECT id, name, updated_at FROM install_jobs
             WHERE status IN ('running', 'interrupted', 'paused')
             ORDER BY updated_at DESC",
        )
        .fetch_all(pool)
        .await
        .unwrap_or_default();
        rows.into_iter()
            .map(|r| {
                (
                    r.get::<String, _>("id"),
                    r.get::<String, _>("name"),
                    r.get::<i64, _>("updated_at"),
                )
            })
            .collect()
    });
    rows.into_iter()
        .map(|(id, name, updated_at)| InterruptedJob {
            id: id.clone(),
            name,
            kind: kind_of(&id).unwrap_or_default(),
            updated_at,
        })
        .collect()
}

/// 加载任务快照（恢复用）；同时返回名称。
pub fn load_spec(id: &str) -> Option<(String, JobSpec)> {
    let Ok(pool) = pool() else {
        return None;
    };
    let id_owned = id.to_string();
    block_on(async move {
        let row = sqlx::query("SELECT name, spec_json FROM install_jobs WHERE id = ?")
            .bind(id_owned)
            .fetch_optional(pool)
            .await
            .ok()??;
        let name: String = row.get("name");
        let spec_json: String = row.get("spec_json");
        let spec: JobSpec = serde_json::from_str(&spec_json).ok()?;
        Some((name, spec))
    })
}

/// 删除快照（恢复/清理时）。
pub fn remove_snapshot(id: &str) {
    let Ok(pool) = pool() else {
        return;
    };
    let id_owned = id.to_string();
    let _ = block_on(async move {
        sqlx::query("DELETE FROM install_jobs WHERE id = ?")
            .bind(id_owned)
            .execute(pool)
            .await
    });
}

/// 任务类型标签（供 list 展示；按 spec 反序列化）。
fn kind_of(id: &str) -> Option<String> {
    load_spec(id).map(|(_, spec)| spec.kind_label().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::task::model::DownloadTask;

    /// 清理测试专用任务 id（避免污染其他测试）。
    fn cleanup(ids: &[&str]) {
        for id in ids {
            remove_snapshot(id);
        }
    }

    #[test]
    fn snapshot_roundtrip_and_status_flow() {
        let id = format!("test_snapshot_{}", std::process::id());
        cleanup(&[&id]);

        let task = DownloadTask {
            version_id: "1.20.4".into(),
            version_url: "https://example.com/1.20.4.json".into(),
            mc_version: "1.20.4".into(),
            loader: None,
            optifine_patch: None,
            version_dir_name: None,
            fabric_api: None,
        };
        save_snapshot(
            &id,
            "测试任务",
            &JobSpec::Download(task),
            TaskStatus::Running,
        );

        // running 可被列出
        let listed = list_interrupted();
        assert!(
            listed.iter().any(|j| j.id == id),
            "running 快照应出现在中断列表"
        );
        // load spec 还原
        let (name, spec) = load_spec(&id).expect("快照应可加载");
        assert_eq!(name, "测试任务");
        match spec {
            JobSpec::Download(t) => assert_eq!(t.version_id, "1.20.4"),
            other => panic!(
                "应还原为 Download 类型，实际 {:?}",
                std::mem::discriminant(&other)
            ),
        }
        // 终态后不再出现在中断列表
        update_status(&id, TaskStatus::Done);
        assert!(
            !list_interrupted().iter().any(|j| j.id == id),
            "done 后不应出现在中断列表"
        );
        cleanup(&[&id]);
    }

    #[test]
    fn mark_crashed_marks_running_as_interrupted() {
        let id = format!("test_crash_{}", std::process::id());
        cleanup(&[&id]);
        save_snapshot(
            &id,
            "崩溃现场",
            &JobSpec::Repair {
                version_dir_name: "26.2".into(),
            },
            TaskStatus::Running,
        );
        mark_crashed_as_interrupted();
        // interrupted 仍可列出（恢复询问依据）
        assert!(
            list_interrupted().iter().any(|j| j.id == id),
            "interrupted 快照应可列出供恢复"
        );
        cleanup(&[&id]);
    }

    /// tauri async command（worker 线程）内调用同步 store 操作不得 panic：block_on 在 worker 线程会炸 "Cannot start a runtime from within a runtime"。
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn store_ops_from_runtime_worker_no_panic() {
        let id = format!("test_worker_{}", std::process::id());
        let task = DownloadTask {
            version_id: "1.20.4".into(),
            version_url: "https://example.com/1.20.4.json".into(),
            mc_version: "1.20.4".into(),
            loader: None,
            optifine_patch: None,
            version_dir_name: None,
            fabric_api: None,
        };
        // spawn 的任务在 runtime worker 线程执行（真实 async command 场景）。
        tokio::task::spawn(async move {
            remove_snapshot(&id);
            save_snapshot(
                &id,
                "worker 现场",
                &JobSpec::Download(task),
                TaskStatus::Running,
            );
            assert!(load_spec(&id).is_some(), "worker 线程应可写入并读回快照");
            update_status(&id, TaskStatus::Done);
            remove_snapshot(&id);
        })
        .await
        .expect("spawn 的任务应正常结束（不得 panic）");
    }
}
