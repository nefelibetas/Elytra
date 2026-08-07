// ── 进度上报基础设施：EmitLoop 主循环 + 收尾心跳 + set_* 状态助手 ──
// 三个任务（run_task / run_files_task / run_repair_task）共用，唯一依赖方向：任务主体 → 本模块。

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tauri::{AppHandle, Emitter};

use super::{emit, global, update_and_emit, TaskProgress};

/// AtomicBool → u32（completed 计数用）。
fn b2u(b: &AtomicBool) -> u32 {
    if b.load(Ordering::Relaxed) {
        1
    } else {
        0
    }
}

/// 下载任务共享的进度上报循环：读原子计数器 → 更新分步/总进度 → emit。
/// run_task（完整下载）与 run_repair_task（补全）共用，消除整段复制。
/// 字段由兄弟模块（runner/repair）构造时填充，故全部 pub。
pub struct EmitLoop {
    pub app: AppHandle,
    pub cancel: Arc<AtomicBool>,
    pub progress: Arc<Mutex<TaskProgress>>,
    pub jar_done: Arc<AtomicU32>,
    pub lib_done: Arc<AtomicU32>,
    pub asst_done: Arc<AtomicU32>,
    pub natives_done: Arc<AtomicU32>,
    pub jar_bytes: Arc<AtomicU64>,
    pub jar_total: Arc<AtomicU64>,
    pub bytes: Arc<AtomicU64>,
    /// 各下载/安装步骤索引（None = 任务无该步骤）
    pub jar_step: Option<usize>,
    pub lib_step: Option<usize>,
    pub asst_step: Option<usize>,
    pub natives_step: Option<usize>,
    /// 收尾安装步骤完成标志（EmitLoop 覆盖阶段 B 全程，这些步骤的进度计入 completed）
    pub cfg_done: Arc<AtomicBool>,
    pub libs_done: Arc<AtomicBool>,
    pub optifine_done: Arc<AtomicBool>,
    pub fabric_done: Arc<AtomicBool>,
    pub lib_pending: u32,
    pub asst_pending: u32,
    pub natives_pending: u32,
    /// 初始已完成数（完整下载 = 预扫描已存在文件；补全 = 0）
    pub initial_completed: u32,
    pub total_files: u32,
    /// 字节维度：检测完的已存在字节与清单总字节（bytes 为 0 时进度回退文件数公式）
    pub initial_bytes: u64,
    pub total_bytes: u64,
    /// jar 总大小兜底（version.json 的 size；content-length 缺失时用）
    pub jar_total_fallback: u64,
}

impl EmitLoop {
    pub fn spawn(self) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut last_bytes: u64 = 0;
            let mut last_sample = Instant::now();
            let mut speed_ema: f64 = 0.0;
            loop {
                let jd = self.jar_done.load(Ordering::Relaxed);
                let jb = self.jar_bytes.load(Ordering::Relaxed);
                let js = self.jar_total.load(Ordering::Relaxed);
                let ld = self.lib_done.load(Ordering::Relaxed);
                let ad = self.asst_done.load(Ordering::Relaxed);
                let nd = self.natives_done.load(Ordering::Relaxed);
                let flags = b2u(&self.cfg_done)
                    + b2u(&self.libs_done)
                    + b2u(&self.optifine_done)
                    + b2u(&self.fabric_done);
                let completed = self.initial_completed + jd + ld + ad + nd + flags;
                // 字节维度：initial（已存在）+ jar 当前字节 + libs/assets 下载增量
                let completed_bytes = self.initial_bytes + jb + self.bytes.load(Ordering::Relaxed);

                let done = {
                    let mut p = self.progress.lock().unwrap();

                    // 实时速度：字节增量差分 + EMA 平滑（每轮采样，间隔 >=300ms 才更新）
                    let cur_bytes = self.bytes.load(Ordering::Relaxed) + jb;
                    let now = Instant::now();
                    let dt = now.duration_since(last_sample).as_secs_f64();
                    if dt >= 0.3 && cur_bytes >= last_bytes {
                        let instant = (cur_bytes - last_bytes) as f64 / dt;
                        speed_ema = if speed_ema <= 0.0 {
                            instant
                        } else {
                            speed_ema * 0.6 + instant * 0.4
                        };
                        last_bytes = cur_bytes;
                        last_sample = now;
                    }
                    p.speed_bps = speed_ema;
                    p.completed_files = completed;
                    p.completed_bytes = completed_bytes;
                    // 最终总数（含 natives/安装步骤）每 tick 写回，剩余文件不会变负
                    p.total_files = self.total_files;
                    // current_phase：第一个未完成（running）的步骤——各并行路完成时
                    // 自行 step_done，此判断实时跟随真实活动
                    if let Some(st) = p.steps.iter().find(|s| s.status == "running") {
                        p.current_phase = st.name.clone();
                    }

                    // 分步进度：client.jar 字节级（total 未知时用完成标志），其余按完成数
                    if let Some(i) = self.jar_step {
                        if let Some(st) = p.steps.get_mut(i) {
                            if st.status == "running" {
                                let jar_total = if js > 0 { js } else { self.jar_total_fallback };
                                st.progress = if jar_total > 0 {
                                    (jb as f64 / jar_total as f64 * 100.0).min(100.0)
                                } else {
                                    (jd as f64) * 100.0
                                };
                            }
                        }
                    }
                    if let Some(i) = self.lib_step {
                        if let Some(st) = p.steps.get_mut(i) {
                            if st.status == "running" {
                                st.progress = if self.lib_pending > 0 {
                                    (ld as f64 / self.lib_pending as f64 * 100.0).min(100.0)
                                } else {
                                    100.0
                                };
                            }
                        }
                    }
                    if let Some(i) = self.asst_step {
                        if let Some(st) = p.steps.get_mut(i) {
                            if st.status == "running" {
                                st.progress = if self.asst_pending > 0 {
                                    (ad as f64 / self.asst_pending as f64 * 100.0).min(100.0)
                                } else {
                                    100.0
                                };
                            }
                        }
                    }
                    if let Some(i) = self.natives_step {
                        if let Some(st) = p.steps.get_mut(i) {
                            if st.status == "running" {
                                st.progress = if self.natives_pending > 0 {
                                    (nd as f64 / self.natives_pending as f64 * 100.0).min(100.0)
                                } else {
                                    100.0
                                };
                            }
                        }
                    }

                    // 总进度 = 已完成字节 / 总字节（与检测阶段 set_completed 同公式，
                    // 无缝衔接：检测完即 initial/total，下载中继续单调涨；大文件按字节占比，
                    // 不再出现"剩余 1 个 jar 却卡 99%"）。
                    // bytes 为 0 时回退文件数占比（repair 等无字节来源场景）。
                    // 封顶 99.9：done 前永不显示 100%，真正的 100% 由任务收尾显式设置。
                    p.progress = if self.total_bytes > 0 {
                        (completed_bytes as f64 / self.total_bytes as f64 * 100.0).min(99.9)
                    } else if self.total_files > 0 {
                        (completed as f64 / self.total_files as f64 * 100.0).min(99.9)
                    } else {
                        0.0
                    };

                    let _ = self.app.emit(crate::events::TASK_PROGRESS, &*p);
                    // 自然退出：计数到顶，或取消，或任务已离开 running（done/error/cancelled）
                    completed >= self.total_files
                        || self.cancel.load(Ordering::SeqCst)
                        || p.status != "running"
                };
                if done {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(150)).await;
            }
        })
    }
}

/// 收尾阶段进度心跳：EmitLoop 在并行下载结束时 abort，natives/ModLoader/OptiFine/Fabric API
/// 期间读 bytes_counter 增量恢复速度与字节进度（避免显示"速度 0 / 进度冻结"）。
/// 自终止：status 离开 "running"（done/error/cancelled）即退出，无需外部 abort。
pub fn spawn_tail_loop(
    app: &AppHandle,
    progress: &Arc<Mutex<TaskProgress>>,
    bytes: Arc<AtomicU64>,
) -> tauri::async_runtime::JoinHandle<()> {
    tauri::async_runtime::spawn({
        let app = app.clone();
        let progress = progress.clone();
        async move {
            let mut tick = tokio::time::interval(Duration::from_millis(150));
            let mut last_bytes: u64 = 0;
            let mut last_sample = Instant::now();
            let mut speed_ema: f64 = 0.0;
            let mut base_done: u64 = 0;
            let mut base_cnt: u64 = 0;
            let mut initialized = false;
            loop {
                tick.tick().await;
                let cur = bytes.load(Ordering::Relaxed);
                let now = Instant::now();
                let dt = now.duration_since(last_sample).as_secs_f64();
                if dt >= 0.3 && cur >= last_bytes {
                    let instant = (cur - last_bytes) as f64 / dt;
                    speed_ema = if speed_ema <= 0.0 {
                        instant
                    } else {
                        speed_ema * 0.6 + instant * 0.4
                    };
                    last_bytes = cur;
                    last_sample = now;
                }
                let mut p = progress.lock().unwrap();
                if p.status != "running" {
                    return;
                }
                if !initialized {
                    base_done = p.completed_bytes;
                    base_cnt = cur;
                    initialized = true;
                }
                p.speed_bps = speed_ema;
                p.completed_bytes = base_done + cur.saturating_sub(base_cnt);
                emit(&app, &p);
            }
        }
    })
}

pub fn cancelled(cancel: &Arc<AtomicBool>) -> bool {
    cancel.load(Ordering::SeqCst)
}

pub fn set_phase(
    app: &AppHandle,
    progress: &Arc<Mutex<TaskProgress>>,
    status: &str,
    phase: &str,
    step: usize,
    step_status: &str,
) {
    update_and_emit(app, progress, |p| {
        if !status.is_empty() {
            p.status = status.into();
        }
        p.current_phase = phase.into();
        if let Some(st) = p.steps.get_mut(step) {
            st.status = step_status.into();
        }
    });
}

pub fn step_done(app: &AppHandle, progress: &Arc<Mutex<TaskProgress>>, idx: usize) {
    update_and_emit(app, progress, |p| {
        if let Some(st) = p.steps.get_mut(idx) {
            st.status = "done".into();
            st.progress = 100.0;
        }
    });
}

/// 总进度统一公式：优先字节占比（方案 A：全程字节，大文件不再卡 99%），
/// bytes 为 0 时回退文件数占比（repair 等无字节来源的场景）。
/// 封顶 99.9：100% 只由任务收尾显式设置（见 run_task/repair 的 done 分支）。
pub fn update_progress(p: &mut TaskProgress) {
    p.progress = if p.total_bytes > 0 {
        (p.completed_bytes as f64 / p.total_bytes as f64 * 100.0).min(99.9)
    } else if p.total_files > 0 {
        (p.completed_files as f64 / p.total_files as f64 * 100.0).min(99.9)
    } else {
        0.0
    };
}

pub fn set_total(app: &AppHandle, progress: &Arc<Mutex<TaskProgress>>, files: u32, bytes: u64) {
    update_and_emit(app, progress, |p| {
        p.total_files = files;
        p.total_bytes = bytes;
        update_progress(p);
    });
}

pub fn set_completed(app: &AppHandle, progress: &Arc<Mutex<TaskProgress>>, files: u32, bytes: u64) {
    update_and_emit(app, progress, |p| {
        p.completed_files = files;
        p.completed_bytes = bytes;
        update_progress(p);
    });
}

/// 收尾步骤的文件计数已由 EmitLoop 的 completed 公式（natives 计数 + 完成标志）承担，
/// bump_completed 已删除。
pub fn set_error(app: &AppHandle, progress: &Arc<Mutex<TaskProgress>>, msg: String) {
    update_and_emit(app, progress, |p| {
        p.status = "error".into();
        p.error = Some(msg);
    });
    // 移除任务注册（前端收到 error 事件后清理卡片）。
    // 不整删下载产物：半截/0B 文件由 sha1 校验与空响应防护在重试时自愈，
    // 已下载内容保留可复用（含 loader 依赖的原版版本 JSON）。
    if let Ok(p) = progress.try_lock() {
        global().fail(&p.task_id);
    }
}

pub fn set_cancelled(app: &AppHandle, progress: &Arc<Mutex<TaskProgress>>) {
    update_and_emit(app, progress, |p| {
        p.status = "cancelled".into();
        p.current_phase = "已取消".into();
    });
}
