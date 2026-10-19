// ── 进度上报基础设施：EmitLoop + 收尾心跳 + set_* 状态助手（依赖方向：任务主体 → 本模块）──

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use super::emit::EventSink;
use parking_lot::Mutex;

use super::{emit, global, update_and_emit, StepStatus, TaskProgress, TaskStatus};

fn b2u(b: &AtomicBool) -> u32 {
    if b.load(Ordering::Relaxed) {
        1
    } else {
        0
    }
}

/// EMA 速度采样器：差分 + 指数平滑（0.6/0.4，≥300ms 采样窗）；停滞 >4s 或速度 <1KB/s 时 ETA 置 None；各处心跳共用。
pub(crate) struct SpeedSampler {
    last_bytes: u64,
    last_sample: Instant,
    last_progress_at: Instant,
    speed_ema: f64,
}

impl SpeedSampler {
    pub fn new() -> Self {
        Self {
            last_bytes: 0,
            last_sample: Instant::now(),
            last_progress_at: Instant::now(),
            speed_ema: 0.0,
        }
    }

    pub fn sample(&mut self, cur_bytes: u64, total_bytes: u64) -> (f64, Option<u64>) {
        let now = Instant::now();
        if cur_bytes > self.last_bytes {
            self.last_progress_at = now;
        }
        let dt = now.duration_since(self.last_sample).as_secs_f64();
        if dt >= 0.3 && cur_bytes > self.last_bytes {
            let instant = (cur_bytes - self.last_bytes) as f64 / dt;
            self.speed_ema = if self.speed_ema <= 0.0 {
                instant
            } else {
                self.speed_ema * 0.6 + instant * 0.4
            };
            self.last_bytes = cur_bytes;
            self.last_sample = now;
        }
        let remaining = total_bytes.saturating_sub(cur_bytes);
        let eta = if self.speed_ema >= 1024.0
            && remaining > 0
            && now.duration_since(self.last_progress_at) <= Duration::from_secs(4)
        {
            Some((remaining as f64 / self.speed_ema).ceil() as u64)
        } else {
            None
        };
        (self.speed_ema, eta)
    }
}

/// 下载任务共享的进度上报循环：读计数器 → 更新分步/总进度 → emit；字段由 runner/repair 构造时填充，故全部 pub。
pub struct EmitLoop {
    pub sink: Arc<dyn EventSink>,
    pub cancel: Arc<AtomicBool>,
    pub progress: Arc<Mutex<TaskProgress>>,
    pub jar_done: Arc<AtomicU32>,
    pub lib_done: Arc<AtomicU32>,
    pub asst_done: Arc<AtomicU32>,
    pub natives_done: Arc<AtomicU32>,
    pub jar_bytes: Arc<AtomicU64>,
    pub jar_total: Arc<AtomicU64>,
    pub bytes: Arc<AtomicU64>,
    pub lib_bytes: Arc<AtomicU64>,
    pub asst_bytes: Arc<AtomicU64>,
    pub natives_bytes: Arc<AtomicU64>,
    pub lib_pending_bytes: u64,
    pub asst_pending_bytes: u64,
    pub natives_pending_bytes: u64,
    pub loader_libs_bytes: Arc<AtomicU64>,
    pub loader_libs_total: Arc<AtomicU64>,
    pub optifine_bytes: Arc<AtomicU64>,
    pub optifine_total: Arc<AtomicU64>,
    pub fabric_bytes: Arc<AtomicU64>,
    pub fabric_total: Arc<AtomicU64>,
    pub jar_step: Option<usize>,
    pub lib_step: Option<usize>,
    pub asst_step: Option<usize>,
    pub natives_step: Option<usize>,
    pub loader_libs_step: Option<usize>,
    pub optifine_step: Option<usize>,
    pub fabric_step: Option<usize>,
    /// 收尾安装步骤完成标志（EmitLoop 覆盖阶段 B，这些步骤进度计入 completed）
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
    pub initial_bytes: u64,
    pub total_bytes: u64,
    /// jar 大小兜底（version.json 的 size 或 content-length 缺失时用）
    pub jar_total_fallback: u64,
}

impl EmitLoop {
    pub fn spawn(self) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut sampler = SpeedSampler::new();
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
                let completed_bytes = self.initial_bytes + jb + self.bytes.load(Ordering::Relaxed);

                let (done, progress_snapshot) = {
                    let mut p = self.progress.lock();
                    // 终态事件由 set_error/set_cancelled/收尾独占发出，此处不重复 emit
                    if p.status != TaskStatus::Running {
                        break;
                    }

                    let cur_bytes = self.bytes.load(Ordering::Relaxed) + jb;
                    let (speed_bps, eta) = sampler.sample(cur_bytes, self.total_bytes);
                    p.speed_bps = speed_bps;
                    p.eta_seconds = eta;
                    p.completed_files = completed;
                    p.completed_bytes = completed_bytes;
                    // 总数每 tick 写回（含 natives/安装步骤），剩余文件不会变负
                    p.total_files = self.total_files;
                    if let Some(st) = p.steps.iter().find(|s| s.status == StepStatus::Running) {
                        p.current_phase = st.name.clone();
                    }

                    if let Some(i) = self.jar_step {
                        if let Some(st) = p.steps.get_mut(i) {
                            if st.status == StepStatus::Running {
                                let jar_total = if js > 0 { js } else { self.jar_total_fallback };
                                st.progress = if jar_total > 0 {
                                    (jb as f64 / jar_total as f64 * 100.0).min(100.0)
                                } else {
                                    (jd as f64) * 100.0
                                };
                            }
                        }
                    }
                    let pct_of =
                        |done: u64, total: u64| (done as f64 / total as f64 * 100.0).min(100.0);
                    if let Some(i) = self.lib_step {
                        if let Some(st) = p.steps.get_mut(i) {
                            if st.status == StepStatus::Running {
                                st.progress = if self.lib_pending_bytes > 0 {
                                    pct_of(
                                        self.lib_bytes.load(Ordering::Relaxed),
                                        self.lib_pending_bytes,
                                    )
                                } else if self.lib_pending > 0 {
                                    (ld as f64 / self.lib_pending as f64 * 100.0).min(100.0)
                                } else {
                                    100.0
                                };
                            }
                        }
                    }
                    if let Some(i) = self.asst_step {
                        if let Some(st) = p.steps.get_mut(i) {
                            if st.status == StepStatus::Running {
                                st.progress = if self.asst_pending_bytes > 0 {
                                    pct_of(
                                        self.asst_bytes.load(Ordering::Relaxed),
                                        self.asst_pending_bytes,
                                    )
                                } else if self.asst_pending > 0 {
                                    (ad as f64 / self.asst_pending as f64 * 100.0).min(100.0)
                                } else {
                                    100.0
                                };
                            }
                        }
                    }
                    if let Some(i) = self.natives_step {
                        if let Some(st) = p.steps.get_mut(i) {
                            if st.status == StepStatus::Running {
                                st.progress = if self.natives_pending_bytes > 0 {
                                    pct_of(
                                        self.natives_bytes.load(Ordering::Relaxed),
                                        self.natives_pending_bytes,
                                    )
                                } else if self.natives_pending > 0 {
                                    (nd as f64 / self.natives_pending as f64 * 100.0).min(100.0)
                                } else {
                                    100.0
                                };
                            }
                        }
                    }
                    // 安装步骤：按字节（total 运行时填充，0 时保持 0 直至完成跳 100%）
                    if let Some(i) = self.loader_libs_step {
                        if let Some(st) = p.steps.get_mut(i) {
                            if st.status == StepStatus::Running {
                                let t = self.loader_libs_total.load(Ordering::Relaxed);
                                if t > 0 {
                                    st.progress =
                                        pct_of(self.loader_libs_bytes.load(Ordering::Relaxed), t);
                                }
                            }
                        }
                    }
                    if let Some(i) = self.optifine_step {
                        if let Some(st) = p.steps.get_mut(i) {
                            if st.status == StepStatus::Running {
                                let t = self.optifine_total.load(Ordering::Relaxed);
                                if t > 0 {
                                    st.progress =
                                        pct_of(self.optifine_bytes.load(Ordering::Relaxed), t);
                                }
                            }
                        }
                    }
                    if let Some(i) = self.fabric_step {
                        if let Some(st) = p.steps.get_mut(i) {
                            if st.status == StepStatus::Running {
                                let t = self.fabric_total.load(Ordering::Relaxed);
                                if t > 0 {
                                    st.progress =
                                        pct_of(self.fabric_bytes.load(Ordering::Relaxed), t);
                                }
                            }
                        }
                    }

                    // 总进度 = 已完成字节/总字节（bytes 为 0 回退文件数占比）；封顶 99.9，100% 只由收尾显式设置。
                    p.progress = if self.total_bytes > 0 {
                        (completed_bytes as f64 / self.total_bytes as f64 * 100.0).min(99.9)
                    } else if self.total_files > 0 {
                        (completed as f64 / self.total_files as f64 * 100.0).min(99.9)
                    } else {
                        0.0
                    };

                    // 快照克隆后锁外 emit：emit 阻塞时不持 progress 锁，否则取消/收尾卡住。
                    (
                        // 自然退出：计数到顶或取消（终态由 set_cancelled/收尾发出）
                        completed >= self.total_files || self.cancel.load(Ordering::SeqCst),
                        p.clone(),
                    )
                };
                emit(self.sink.as_ref(), &progress_snapshot);
                if done {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(150)).await;
            }
        })
    }
}

/// 收尾阶段心跳：EmitLoop 在并行下载结束时 abort，此间读 bytes 增量恢复速度/字节进度；status 离开 Running 即自终止，无需外部 abort。
pub fn spawn_tail_loop(
    sink: Arc<dyn EventSink>,
    progress: &Arc<Mutex<TaskProgress>>,
    bytes: Arc<AtomicU64>,
) -> tauri::async_runtime::JoinHandle<()> {
    tauri::async_runtime::spawn({
        let sink = sink.clone();
        let progress = progress.clone();
        async move {
            let mut tick = tokio::time::interval(Duration::from_millis(150));
            let mut sampler = SpeedSampler::new();
            let mut base_done: u64 = 0;
            let mut base_cnt: u64 = 0;
            let mut initialized = false;
            loop {
                tick.tick().await;
                let cur = bytes.load(Ordering::Relaxed);
                let snapshot = {
                    let mut p = progress.lock();
                    if p.status != TaskStatus::Running {
                        return;
                    }
                    if !initialized {
                        base_done = p.completed_bytes;
                        base_cnt = cur;
                        initialized = true;
                    }
                    p.completed_bytes = base_done + cur.saturating_sub(base_cnt);
                    // ETA 基线换算：total + base_done - base_cnt 与 sampler 内部等价
                    let total_for_eta = p
                        .total_bytes
                        .saturating_add(base_done)
                        .saturating_sub(base_cnt);
                    let (speed_bps, eta) = sampler.sample(cur, total_for_eta);
                    p.speed_bps = speed_bps;
                    p.eta_seconds = eta;
                    Some(p.clone())
                };
                if let Some(p) = snapshot {
                    emit(sink.as_ref(), &p);
                }
            }
        }
    })
}

pub fn cancelled(cancel: &Arc<AtomicBool>) -> bool {
    cancel.load(Ordering::SeqCst)
}

pub fn set_phase(
    sink: &dyn EventSink,
    progress: &Arc<Mutex<TaskProgress>>,
    status: Option<TaskStatus>,
    phase: &str,
    step: usize,
    step_status: StepStatus,
) {
    update_and_emit(sink, progress, |p| {
        if let Some(s) = status {
            p.status = s;
        }
        p.current_phase = phase.into();
        if let Some(st) = p.steps.get_mut(step) {
            st.status = step_status;
        }
    });
}

pub fn step_done(sink: &dyn EventSink, progress: &Arc<Mutex<TaskProgress>>, idx: usize) {
    update_and_emit(sink, progress, |p| {
        if let Some(st) = p.steps.get_mut(idx) {
            st.status = StepStatus::Done;
            st.progress = 100.0;
        }
    });
}

/// 总进度统一公式：优先字节占比（大文件不卡 99%），bytes 为 0 时回退 [`steps_percent`]；封顶 99.9（100% 只由收尾显式设置）。
pub fn update_progress(p: &mut TaskProgress) {
    p.progress = if p.total_bytes > 0 {
        (p.completed_bytes as f64 / p.total_bytes as f64 * 100.0).min(99.9)
    } else {
        steps_percent(p)
    };
}

pub fn set_total(
    sink: &dyn EventSink,
    progress: &Arc<Mutex<TaskProgress>>,
    files: u32,
    bytes: u64,
) {
    update_and_emit(sink, progress, |p| {
        p.total_files = files;
        p.total_bytes = bytes;
        update_progress(p);
    });
}

pub fn set_completed(
    sink: &dyn EventSink,
    progress: &Arc<Mutex<TaskProgress>>,
    files: u32,
    bytes: u64,
) {
    update_and_emit(sink, progress, |p| {
        p.completed_files = files;
        p.completed_bytes = bytes;
        update_progress(p);
    });
    // 此处不 fail：set_completed 会多次调用，移除注册会让后续取消失效（注册由终态收口统一移除）。
}

/// 成功收口（唯一实现）：Done 终态事件 + 100% 进度 + 移除任务注册；`phase` 为收尾文案。
pub fn set_done(sink: &dyn EventSink, progress: &Arc<Mutex<TaskProgress>>, phase: &str) {
    update_and_emit(sink, progress, |p| {
        p.status = TaskStatus::Done;
        p.progress = 100.0;
        p.completed_files = p.total_files;
        p.current_phase = phase.into();
    });
    // 终态统一移除任务注册（remove 同清挂起登记）；先释放 progress 锁再 remove，防死锁。
    if let Some(p) = progress.try_lock() {
        let task_id = p.task_id.clone();
        drop(p);
        global().remove(&task_id);
    }
}

/// 失败收口：Error 终态事件 + 移除任务注册。此前 set_cleanup_path 登记过清理路径（如新建版本目录）则取走并后台整删——同步 remove_dir_all 会阻塞任务线程（大目录数十秒）。
pub fn set_error(sink: &dyn EventSink, progress: &Arc<Mutex<TaskProgress>>, msg: String) {
    update_and_emit(sink, progress, |p| {
        p.status = TaskStatus::Error;
        p.error = Some(msg);
    });
    // 先释放 progress 锁再 take_cleanup_path（后者持 inner 锁），保持 inner→progress 锁序。
    let task_id = progress.try_lock().map(|p| p.task_id.clone());
    let Some(task_id) = task_id else {
        return;
    };
    if let Some(path) = global().take_cleanup_path(&task_id) {
        tauri::async_runtime::spawn_blocking(move || {
            if let Err(e) = std::fs::remove_dir_all(&path) {
                eprintln!("[task] 失败清理目录失败\n{e}");
            }
        });
    }
    // 不整删下载产物：半截/0B 文件由重试 sha1 校验与空响应防护自愈，可复用。
    global().fail(&task_id);
}

pub fn set_cancelled(sink: &dyn EventSink, progress: &Arc<Mutex<TaskProgress>>) {
    // 取消收口：任务侧下载已停止，取走清理路径后台整删；先释放 progress 锁再 take_cleanup_path（后者持 inner 锁），保持 inner→progress 锁序。
    let cleanup = {
        if let Some(p) = progress.try_lock() {
            let task_id = p.task_id.clone();
            drop(p);
            global().take_cleanup_path(&task_id)
        } else {
            None
        }
    };
    if let Some(path) = cleanup {
        tauri::async_runtime::spawn_blocking(move || {
            // Windows 文件占用（杀软/刚关闭句柄）常为瞬时：300ms 间隔重试至多 10 次（≈3s）。
            for attempt in 0..10u32 {
                if !path.exists() {
                    return;
                }
                match std::fs::remove_dir_all(&path) {
                    Ok(_) => return,
                    Err(e) => {
                        if attempt >= 9 {
                            eprintln!(
                                "[task] 取消清理失败（重试 10 次后仍被占用）: {} → {e}",
                                path.display()
                            );
                        } else {
                            std::thread::sleep(Duration::from_millis(300));
                        }
                    }
                }
            }
        });
    }
    update_and_emit(sink, progress, |p| {
        p.status = TaskStatus::Cancelled;
        p.current_phase = "已取消".into();
    });
    if let Some(p) = progress.try_lock() {
        let task_id = p.task_id.clone();
        drop(p); // 先释放 progress 锁再 fail（fail 内部读 progress，防死锁）
        global().fail(&task_id);
    }
}

// ── 共享简易心跳（files / modpack 等顺序+块式任务复用）──

/// 当前运行步骤的分步进度来源（不走 [`EmitLoop`] 的任务复用）：文件数（files_total > 0）→ 字节（step_total > 0）→ 不更新（分母未知时保持现值，避免假的 0%）。
#[derive(Clone)]
pub struct StepRef {
    /// 当前步骤索引；`usize::MAX` = 无运行中步骤。
    pub idx: Arc<AtomicUsize>,
    pub step_bytes: Arc<AtomicU64>,
    pub step_total: Arc<AtomicU64>,
    pub files: Arc<AtomicU32>,
    pub files_total: Arc<AtomicU32>,
}

impl StepRef {
    /// 当前步骤百分比；无运行中步骤或分母未知 → None（调用方保持原值，避免假的 0%）。
    pub fn percent(&self) -> Option<f64> {
        if self.idx.load(Ordering::Relaxed) == usize::MAX {
            return None;
        }
        let files_total = self.files_total.load(Ordering::Relaxed);
        if files_total > 0 {
            return percent(
                self.files.load(Ordering::Relaxed) as u64,
                files_total as u64,
            );
        }
        percent(
            self.step_bytes.load(Ordering::Relaxed),
            self.step_total.load(Ordering::Relaxed),
        )
    }
}

/// 分步百分比（上限 100）；分母为 0 → None。
fn percent(done: u64, total: u64) -> Option<f64> {
    (total > 0).then(|| (done as f64 / total as f64 * 100.0).min(100.0))
}

/// 总进度的**步骤口径**：完成步骤数 / 总步骤数（封顶 99.9，100% 只由收尾显式设置）；批量任务（整合包安装）用它避免分母含不进分子累加的字节 → 低位卡死 + 收尾跳变。
pub fn steps_percent(p: &TaskProgress) -> f64 {
    if p.total_files > 0 {
        (p.completed_files as f64 / p.total_files as f64 * 100.0).min(99.9)
    } else {
        0.0
    }
}

/// 心跳输入计数器束：完成/总文件数、累计字节（速度）、总量估算（运行中可增长，ETA 分母）、百分比口径、当前步骤进度（可选）。
pub struct HeartbeatStats {
    pub done_files: Arc<AtomicU32>,
    pub total_files: u32,
    pub bytes: Arc<AtomicU64>,
    pub est_total_bytes: Arc<AtomicU64>,
    /// `true` = 总进度走步骤口径（[`steps_percent`]，分母 `total_files`）；`false` = [`update_progress`] 默认公式（优先字节占比，分母 `est_total_bytes`）。整合包安装**必须**用步骤口径：其 `est_bytes` 含永不进字节分子的部分（本地包体、overrides 解压、已存在文件），字节占比追不上分母 → 卡片停低位、收尾跳 100%；两个字节字段照常写入。
    pub percent_by_steps: bool,
    pub step: Option<StepRef>,
}

/// 150ms 共享心跳：刷新状态 Running、文件/字节进度、EMA 速度（0.6/0.4，≥300ms 采样窗）、ETA（<1KB/s 或停滞 >4s 置 None）；百分比交由 update_progress 计算，句柄由调用方收尾前 abort。
pub fn spawn_heartbeat(
    sink: Arc<dyn EventSink>,
    progress: &Arc<Mutex<TaskProgress>>,
    stats: HeartbeatStats,
) -> tauri::async_runtime::JoinHandle<()> {
    let done_files = stats.done_files;
    let bytes = stats.bytes;
    let est = stats.est_total_bytes;
    let total_files = stats.total_files;
    let percent_by_steps = stats.percent_by_steps;
    let step = stats.step;
    tauri::async_runtime::spawn({
        let sink = sink.clone();
        let progress = progress.clone();
        async move {
            let mut tick = tokio::time::interval(Duration::from_millis(150));
            let mut sampler = SpeedSampler::new();
            loop {
                tick.tick().await;
                // 终态由任务侧收口发出，心跳不得覆写（否则 cancel 置位后卡片会复活）
                match progress.lock().status {
                    TaskStatus::Pending => {
                        if let Some(mut p) = progress.try_lock() {
                            p.status = TaskStatus::Running;
                        }
                    }
                    TaskStatus::Running => {}
                    _ => return,
                }
                let cur = bytes.load(Ordering::Relaxed);
                let total_bytes = est.load(Ordering::Relaxed);
                let (speed_bps, eta) = sampler.sample(cur, total_bytes);
                update_and_emit(sink.as_ref(), &progress, |p| {
                    p.status = TaskStatus::Running;
                    p.completed_files = done_files.load(Ordering::Relaxed);
                    p.total_files = total_files;
                    p.completed_bytes = cur;
                    p.total_bytes = total_bytes;
                    p.speed_bps = speed_bps;
                    p.eta_seconds = eta;
                    // 分步进度（整合包等批量任务）：只改"运行中"那一步，已 done 不得回退
                    if let Some(step) = &step {
                        let idx = step.idx.load(Ordering::Relaxed);
                        if idx != usize::MAX {
                            if let Some(pct) = step.percent() {
                                if let Some(st) = p.steps.get_mut(idx) {
                                    if st.status == StepStatus::Running {
                                        st.progress = pct;
                                    }
                                }
                            }
                        }
                    }
                    if percent_by_steps {
                        p.progress = steps_percent(p);
                    } else {
                        update_progress(p);
                    }
                });
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::task::StepInfo;

    #[test]
    fn percent_needs_a_denominator() {
        assert_eq!(percent(5, 0), None);
        assert_eq!(percent(0, 0), None);
        assert_eq!(percent(5, 10), Some(50.0));
        // 字节口径可能略超估算 → 封顶 100
        assert_eq!(percent(20, 10), Some(100.0));
    }

    #[test]
    fn step_ref_prefers_file_denominator() {
        let step = StepRef {
            idx: Arc::new(AtomicUsize::new(3)),
            step_bytes: Arc::new(AtomicU64::new(1)),
            step_total: Arc::new(AtomicU64::new(100)),
            files: Arc::new(AtomicU32::new(1)),
            files_total: Arc::new(AtomicU32::new(4)),
        };
        // 字节口径 1% vs 文件口径 25%，两者不能混用
        assert_eq!(step.percent(), Some(25.0));

        step.files_total.store(0, Ordering::Relaxed);
        assert_eq!(step.percent(), Some(1.0));

        // 无运行中步骤（idx = usize::MAX）→ None
        step.idx.store(usize::MAX, Ordering::Relaxed);
        assert_eq!(step.percent(), None);
    }

    #[tokio::test]
    async fn heartbeat_fills_running_step_progress() {
        struct NullSink;
        impl EventSink for NullSink {
            fn send(&self, _event: &'static str, _payload: serde_json::Value) {}
        }

        let progress = Arc::new(Mutex::new(TaskProgress {
            task_id: "t1".into(),
            name: "整合包".into(),
            status: TaskStatus::Pending,
            progress: 0.0,
            speed_bps: 0.0,
            eta_seconds: None,
            completed_files: 0,
            total_files: 2,
            completed_bytes: 0,
            total_bytes: 0,
            current_phase: "游戏主文件".into(),
            steps: vec![
                StepInfo {
                    name: "游戏主文件".into(),
                    status: StepStatus::Running,
                    progress: 0.0,
                },
                StepInfo {
                    name: "依赖库".into(),
                    status: StepStatus::Pending,
                    progress: 0.0,
                },
            ],
            error: None,
        }));
        let step_bytes = Arc::new(AtomicU64::new(0));
        let handle = spawn_heartbeat(
            Arc::new(NullSink),
            &progress,
            HeartbeatStats {
                done_files: Arc::new(AtomicU32::new(0)),
                total_files: 2,
                bytes: Arc::clone(&step_bytes),
                est_total_bytes: Arc::new(AtomicU64::new(0)),
                percent_by_steps: false,
                step: Some(StepRef {
                    idx: Arc::new(AtomicUsize::new(0)),
                    step_bytes: Arc::clone(&step_bytes),
                    step_total: Arc::new(AtomicU64::new(200)),
                    files: Arc::new(AtomicU32::new(0)),
                    files_total: Arc::new(AtomicU32::new(0)),
                }),
            },
        );

        step_bytes.store(50, Ordering::Relaxed);
        // 轮询等待而非固定 sleep：心跳可能晚到，固定等待会偶发失败
        let deadline = Instant::now() + Duration::from_secs(3);
        while Instant::now() < deadline {
            if progress.lock().steps[0].progress >= 25.0 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        handle.abort();

        let p = progress.lock();
        assert_eq!(p.steps[0].progress, 25.0, "运行中步骤应按字节刷新");
        assert_eq!(p.steps[1].progress, 0.0, "未运行的步骤不得被改写");
        assert_eq!(
            p.status,
            TaskStatus::Running,
            "心跳应把 Pending 置为 Running"
        );
    }

    /// `percent_by_steps`：总进度按完成步骤数，**忽略** `total_bytes`（整合包 est 含永不回传的字节，故同一组计数在字节口径下会给出不同的数字）。
    #[tokio::test]
    async fn heartbeat_percent_by_steps_ignores_byte_denominator() {
        struct NullSink;
        impl EventSink for NullSink {
            fn send(&self, _event: &'static str, _payload: serde_json::Value) {}
        }
        let progress = Arc::new(Mutex::new(TaskProgress {
            task_id: "t1".into(),
            name: "整合包：愚者".into(),
            status: TaskStatus::Running,
            progress: 0.0,
            speed_bps: 0.0,
            eta_seconds: None,
            completed_files: 0,
            total_files: 0,
            completed_bytes: 0,
            total_bytes: 0,
            current_phase: "依赖库".into(),
            steps: vec![],
            error: None,
        }));
        let bytes = Arc::new(AtomicU64::new(100));
        let est = Arc::new(AtomicU64::new(800));
        let handle = spawn_heartbeat(
            Arc::new(NullSink),
            &progress,
            HeartbeatStats {
                done_files: Arc::new(AtomicU32::new(3)),
                total_files: 8,
                bytes: Arc::clone(&bytes),
                est_total_bytes: Arc::clone(&est),
                percent_by_steps: true,
                step: None,
            },
        );
        let deadline = Instant::now() + Duration::from_secs(3);
        while Instant::now() < deadline {
            if progress.lock().progress > 0.0 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        handle.abort();

        let p = progress.lock();
        assert_eq!(p.progress, 37.5, "步骤口径：3/8 步");
        assert_eq!(p.total_bytes, 800, "字节总量仍照常写入（速度/聚合 ETA 用）");
    }
}
