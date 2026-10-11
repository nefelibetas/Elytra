// ── 进度上报基础设施：EmitLoop 主循环 + 收尾心跳 + set_* 状态助手 ──
// 三个任务（run_task / run_files_task / run_repair_task）共用，唯一依赖方向：任务主体 → 本模块。

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use super::emit::EventSink;

use super::{emit, global, update_and_emit, StepStatus, TaskProgress, TaskStatus};

/// AtomicBool → u32（completed 计数用）。
fn b2u(b: &AtomicBool) -> u32 {
    if b.load(Ordering::Relaxed) {
        1
    } else {
        0
    }
}

/// EMA 速度采样器：相邻采样差分 + 指数平滑（0.6/0.4，≥300ms 采样窗），
/// 停滞（>4s 无推进）或速度过低（<1KB/s）时 ETA 置 None。
/// 供 EmitLoop / spawn_tail_loop / spawn_heartbeat / jdk 心跳共用，消除四处手写重复。
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

    /// 喂入当前累计字节与总字节，返回 `(speed_bps, eta_seconds)`。
    /// 采样窗 ≥300ms 且字节有推进才更新 EMA；停滞 >4s 或速度 <1KB/s 时 ETA 为 None。
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

/// 下载任务共享的进度上报循环：读原子计数器 → 更新分步/总进度 → emit。
/// run_task（完整下载）与 run_repair_task（补全）共用，消除整段复制。
/// 字段由兄弟模块（runner/repair）构造时填充，故全部 pub。
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
    /// 分步字节：libs/assets/natives 各自步骤内下载字节增量（分子）
    pub lib_bytes: Arc<AtomicU64>,
    pub asst_bytes: Arc<AtomicU64>,
    pub natives_bytes: Arc<AtomicU64>,
    /// 分步字节分母：需下载部分的总字节（0 时回退文件数公式）
    pub lib_pending_bytes: u64,
    pub asst_pending_bytes: u64,
    pub natives_pending_bytes: u64,
    /// 安装步骤字节：LoaderLibs/OptiFine/FabricApi（total 运行时填充 content-length；
    /// Fabric 场景预填 pending 字节）
    pub loader_libs_bytes: Arc<AtomicU64>,
    pub loader_libs_total: Arc<AtomicU64>,
    pub optifine_bytes: Arc<AtomicU64>,
    pub optifine_total: Arc<AtomicU64>,
    pub fabric_bytes: Arc<AtomicU64>,
    pub fabric_total: Arc<AtomicU64>,
    /// 各下载/安装步骤索引（None = 任务无该步骤）
    pub jar_step: Option<usize>,
    pub lib_step: Option<usize>,
    pub asst_step: Option<usize>,
    pub natives_step: Option<usize>,
    pub loader_libs_step: Option<usize>,
    pub optifine_step: Option<usize>,
    pub fabric_step: Option<usize>,
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
    /// 进度上报主循环（run_task / run_repair_task 共用）。
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
                // 字节维度：initial（已存在）+ jar 当前字节 + libs/assets 下载增量
                let completed_bytes = self.initial_bytes + jb + self.bytes.load(Ordering::Relaxed);

                let (done, progress_snapshot) = {
                    let mut p = self.progress.lock();
                    // 终态（error/cancelled/收尾 done）事件由 set_error/set_cancelled/任务收尾
                    // 独占发出；此处不再重复 emit（否则前端会收到第二次终态事件）。
                    if p.status != TaskStatus::Running {
                        break;
                    }

                    // 实时速度：字节增量差分 + EMA 平滑（SpeedSampler 统一采样/ETA）
                    let cur_bytes = self.bytes.load(Ordering::Relaxed) + jb;
                    let (speed_bps, eta) = sampler.sample(cur_bytes, self.total_bytes);
                    p.speed_bps = speed_bps;
                    p.eta_seconds = eta;
                    p.completed_files = completed;
                    p.completed_bytes = completed_bytes;
                    // 最终总数（含 natives/安装步骤）每 tick 写回，剩余文件不会变负
                    p.total_files = self.total_files;
                    // current_phase：第一个未完成（running）的步骤——各并行路完成时
                    // 自行 step_done，此判断实时跟随真实活动
                    if let Some(st) = p.steps.iter().find(|s| s.status == StepStatus::Running) {
                        p.current_phase = st.name.clone();
                    }

                    // 分步进度：client.jar 字节级（total 未知时用完成标志），其余按字节/文件数
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
                    // libs/assets/natives：字节优先（pending 字节已知时），否则回退文件数
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
                    // 安装步骤：单文件/多文件字节（total 运行时填充，0 时保持 0 直到完成跳 100%）
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

                    // 快照克隆后锁外 emit：emit（tauri IPC 分发到 webview）若因前端
                    // 未及时消费而阻塞，不持有 progress 锁——否则取消（cancel 的
                    // try_lock）与任务收尾全被卡住，表现为"取消失效/任务卡死"。
                    (
                        // 自然退出：计数到顶，或取消（终态事件仍由 set_cancelled/收尾发出）
                        completed >= self.total_files || self.cancel.load(Ordering::SeqCst),
                        p.clone(),
                    )
                };
                // 队列线程派发（emit 阻塞不占本任务/心跳线程）
                emit(self.sink.as_ref(), &progress_snapshot);
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
/// 自终止：status 离开 `TaskStatus::Running`（done/error/cancelled）即退出，无需外部 abort。
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
                    // ETA 基于带基线的 completed 字节：sampler 内部用 (总 - 当前增量) 计算，
                    // 传入按基线换算后的总字节即可等价。
                    let total_for_eta = p
                        .total_bytes
                        .saturating_add(base_done)
                        .saturating_sub(base_cnt);
                    let (speed_bps, eta) = sampler.sample(cur, total_for_eta);
                    p.speed_bps = speed_bps;
                    p.eta_seconds = eta;
                    // 快照克隆后锁外 emit（同 EmitLoop：emit 阻塞不占 progress 锁）
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

pub fn set_total(sink: &dyn EventSink, progress: &Arc<Mutex<TaskProgress>>, files: u32, bytes: u64) {
    update_and_emit(sink, progress, |p| {
        p.total_files = files;
        p.total_bytes = bytes;
        update_progress(p);
    });
}

pub fn set_completed(sink: &dyn EventSink, progress: &Arc<Mutex<TaskProgress>>, files: u32, bytes: u64) {
    update_and_emit(sink, progress, |p| {
        p.completed_files = files;
        p.completed_bytes = bytes;
        update_progress(p);
    });
    // 注意：此处不 fail——set_completed 是进度更新（阶段 B 检测中/on_total 都多次调用），
    // 移除注册会让阶段 C 的取消失效（cancel 查不到任务）。任务注册由终态
    // （Done/Error/Cancelled 的 update_and_emit 后）统一 fail 收口。
}

/// 成功收口（唯一实现）：Done 终态事件 + 100% 进度 + 移除任务注册。
/// runner（完整下载）与 task/modpack（整合包）共用；`phase` 为收尾阶段文案。
pub fn set_done(sink: &dyn EventSink, progress: &Arc<Mutex<TaskProgress>>, phase: &str) {
    update_and_emit(sink, progress, |p| {
        p.status = TaskStatus::Done;
        p.progress = 100.0;
        p.completed_files = p.total_files;
        p.current_phase = phase.into();
    });
    // 终态统一移除后端任务注册（与 set_error/set_cancelled 一致；remove 同时清挂起登记）。
    // 先取 task_id 释放 progress 锁再调 remove（后者读 progress），防重入死锁。
    if let Some(p) = progress.try_lock() {
        let task_id = p.task_id.clone();
        drop(p);
        global().remove(&task_id);
    }
}

/// 失败收口：Error 终态事件 + 移除任务注册。
/// 若调用方此前用 `set_cleanup_path` 登记了清理路径（如本次新建的版本目录），
/// 在此统一取走并**后台**整删——同步 `remove_dir_all` 会阻塞任务线程（大目录数十秒）。
pub fn set_error(sink: &dyn EventSink, progress: &Arc<Mutex<TaskProgress>>, msg: String) {
    update_and_emit(sink, progress, |p| {
        p.status = TaskStatus::Error;
        p.error = Some(msg);
    });
    // 先取 task_id 释放 progress 锁再 take_cleanup_path（后者持 inner 锁），
    // 保持全模块 inner→progress 统一锁序。
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
    // 不整删下载产物：半截/0B 文件由 sha1 校验与空响应防护在重试时自愈，
    // 已下载内容保留可复用（含 loader 依赖的原版版本 JSON）。
    global().fail(&task_id);
}

pub fn set_cancelled(sink: &dyn EventSink, progress: &Arc<Mutex<TaskProgress>>) {
    // 取消收口：任务侧下载已停止，取走清理路径后台整删（大目录不阻塞收尾线程）。
    // 先取 task_id 释放 progress 锁再 take_cleanup_path（后者持 inner 锁），
    // 保持全模块 inner→progress 统一锁序，杜绝死锁环。
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
            // 整删带重试：Windows 文件占用（杀软扫描/刚关闭的句柄）常为瞬时状态，
            // 300ms 间隔重试至多 10 次（≈3s），保证取消后版本目录真正清空；
            // 目录不存在（未创建/已删）直接返回，避免无谓重试。
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
    // 终态统一移除后端任务注册
    if let Some(p) = progress.try_lock() {
        let task_id = p.task_id.clone();
        drop(p); // 先释放 progress 锁再 fail（fail 内部读 progress，防重入死锁）
        global().fail(&task_id);
    }
}

// ── 共享简易心跳（files / modpack 等顺序+块式任务复用）──

/// 当前运行步骤的分步进度来源（供不走 [`EmitLoop`] 的任务复用同一套展示逻辑）。
///
/// `EmitLoop` 是版本下载特化（jar/lib/asst/natives/loader/optifine 七八套步骤字段
/// 加初始扫描），整合包安装塞进去不划算，故抽这个轻量视图：心跳每拍据此填
/// `steps[idx].progress`。公式优先级：文件数（`files_total > 0`）→ 字节
/// （`step_total > 0`）→ 不更新（分母未知时保持前端现值，避免显示假的 0%）。
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
    /// 当前步骤百分比；无运行中步骤或分母未知时 `None`（调用方保持原值，
    /// 避免把"总量未知"显示成假的 0%）。
    pub fn percent(&self) -> Option<f64> {
        if self.idx.load(Ordering::Relaxed) == usize::MAX {
            return None;
        }
        let files_total = self.files_total.load(Ordering::Relaxed);
        if files_total > 0 {
            return percent(self.files.load(Ordering::Relaxed) as u64, files_total as u64);
        }
        percent(
            self.step_bytes.load(Ordering::Relaxed),
            self.step_total.load(Ordering::Relaxed),
        )
    }
}

/// 分步百分比（上限 100）；分母为 0 时 `None`。
fn percent(done: u64, total: u64) -> Option<f64> {
    (total > 0).then(|| (done as f64 / total as f64 * 100.0).min(100.0))
}

/// 心跳输入计数器束：完成文件数（进度分子）/ 总文件数 /
/// 累计字节（速度来源）/ 总量估算（可运行中增长，ETA 分母）/ 当前步骤进度（可选）。
pub struct HeartbeatStats {
    pub done_files: Arc<AtomicU32>,
    pub total_files: u32,
    pub bytes: Arc<AtomicU64>,
    pub est_total_bytes: Arc<AtomicU64>,
    /// 提供则每拍刷新 `steps[idx].progress`（整合包安装用；版本下载走 `EmitLoop`）。
    pub step: Option<StepRef>,
}

/// 150ms 共享心跳：刷新状态 Running、文件/字节进度、EMA 速度（0.6/0.4，
/// ≥300ms 采样窗）、ETA（速度 <1KB/s 或停滞 >4s 置 None）。
/// 进度百分比交由 `update_progress` 统一公式计算；返回句柄由调用方在
/// 任务收尾前 abort。语义对齐 files/modpack 原有内联实现。
pub fn spawn_heartbeat(
    sink: Arc<dyn EventSink>,
    progress: &Arc<Mutex<TaskProgress>>,
    stats: HeartbeatStats,
) -> tauri::async_runtime::JoinHandle<()> {
    let done_files = stats.done_files;
    let bytes = stats.bytes;
    let est = stats.est_total_bytes;
    let total_files = stats.total_files;
    let step = stats.step;
    tauri::async_runtime::spawn({
        let sink = sink.clone();
        let progress = progress.clone();
        async move {
            let mut tick = tokio::time::interval(Duration::from_millis(150));
            let mut sampler = SpeedSampler::new();
            loop {
                tick.tick().await;
                // 终态（done/error/cancelled）由任务侧收口发出——心跳不得覆写，
                // 否则 cancel 置位后心跳会把卡片重置回运行中（"取消后复活"根因）
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
                    // 分步进度（整合包等批量任务）：只改"运行中"的那一步，
                    // 已 done 的步骤不得被覆写回退
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
                    update_progress(p);
                });
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::task::StepInfo;

    /// 分步百分比：分母未知 → None（保持前端现值，不显示假的 0%）。
    #[test]
    fn percent_needs_a_denominator() {
        assert_eq!(percent(5, 0), None);
        assert_eq!(percent(0, 0), None);
        assert_eq!(percent(5, 10), Some(50.0));
        // 字节口径可能略超估算（实测 size 与 content-length 有出入）→ 封顶 100
        assert_eq!(percent(20, 10), Some(100.0));
    }

    /// 文件数分母优先于字节分母（批量步骤必须走文件口径）；无运行中步骤时恒 None。
    #[test]
    fn step_ref_prefers_file_denominator() {
        let step = StepRef {
            idx: Arc::new(AtomicUsize::new(3)),
            step_bytes: Arc::new(AtomicU64::new(1)),
            step_total: Arc::new(AtomicU64::new(100)),
            files: Arc::new(AtomicU32::new(1)),
            files_total: Arc::new(AtomicU32::new(4)),
        };
        // 字节口径会得出 1%、文件口径 25%——两者绝不能混用
        assert_eq!(step.percent(), Some(25.0));

        // 无文件分母时回退字节口径
        step.files_total.store(0, Ordering::Relaxed);
        assert_eq!(step.percent(), Some(1.0));

        // 无运行中步骤（idx = usize::MAX）→ None
        step.idx.store(usize::MAX, Ordering::Relaxed);
        assert_eq!(step.percent(), None);
    }

    /// 心跳每拍把当前步骤的进度写进 `steps[idx]`（整合包长步骤百分比的实际通路）。
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
        // 轮询等待而非固定 sleep：机器负载高时 150ms 心跳可能晚到，
        // 固定等待会让断言偶发失败（测试必须确定性地通过或失败）。
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
        assert_eq!(p.status, TaskStatus::Running, "心跳应把 Pending 置为 Running");
    }
}
