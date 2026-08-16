import { reactive, ref } from "vue"
import { listen } from "@tauri-apps/api/event"
import { EVT_TASK_PROGRESS, EVT_TASK_WAITING_USER } from "./events"
import { invoke } from "@tauri-apps/api/core"
import { showToast } from "../utils/toast"
import { errMsg } from "../utils/error"

export type StepStatus = "pending" | "running" | "done";

/** 任务种类标记（前端私有，用于终态回调分流；仅特殊任务设置）。 */
export type TaskKind = "jdk";

export interface StepInfo {
  name: string
  status: StepStatus
  progress: number
}

export interface TaskItem {
  taskId: string
  name: string
  status: "pending" | "running" | "done" | "error" | "cancelled"
  progress: number
  speedBps: number
  /** 预计剩余秒数；停滞或速度过低时为 null */
  etaSeconds: number | null
  completedFiles: number
  totalFiles: number
  completedBytes: number
  totalBytes: number
  currentPhase: string
  steps: StepInfo[]
  error?: string
  /** 任务种类（JDK 安装等特殊任务；版本/文件下载不设置） */
  kind?: TaskKind
}

export interface ProgressPayload {
  task_id: string
  name: string
  status: string
  progress: number
  speed_bps: number
  eta_seconds: number | null
  completed_files: number
  total_files: number
  completed_bytes: number
  total_bytes: number
  current_phase: string
  steps: StepInfo[]
  error?: string | null
}

// ── 任务入参（镜像 Rust task 模块；字段名必须 snake_case，tauri 键名严格匹配）──

export interface LoaderChoice {
  kind: "fabric" | "forge" | "neoforge" | "quilt"
  version: string
}

export interface FabricApi {
  url: string
  version: string
}

export interface DownloadTask {
  version_id: string
  version_url: string
  mc_version: string
  loader: LoaderChoice | null
  optifine_patch: string | null
  version_dir_name: string | null
  fabric_api: FabricApi | null
}

export type DownloadFileKind = "mod" | "resourcepack" | "datapack" | "shader";

export interface DownloadFileTask {
  version_dir_name: string
  kind: DownloadFileKind
  url: string
  filename: string
}

export interface DownloadFileItem {
  kind: DownloadFileKind
  url: string
  filename: string
}

export interface DownloadFilesTask {
  version_dir_name: string
  files: DownloadFileItem[]
}

/** 任务资源名映射：taskId → 资源名（社区资源单文件下载注册，完成提示用）。
 *  模块级（非 dlState 内）避免响应式开销；done 事件消费后即删除。 */
export const taskNames = new Map<string, string>();

/** 注册任务资源名：提交下载后调用，任务完成提示显示 "下载任务[资源名]完成"。 */
export function registerTaskName(taskId: string, name: string) {
  taskNames.set(taskId, name);
}

/** 版本下载任务 → 安装目录名（下载完成自动选中该版本作为后续 Mod 下载目标；
 *  done 事件消费后删除）。 */
export const versionTargets = new Map<string, string>();

/** 注册版本下载目标：startTask 提交后调用。 */
export function registerVersionTarget(taskId: string, dirName: string) {
  versionTargets.set(taskId, dirName);
}

export const dlState = reactive({
  tasks: [] as TaskItem[],

  get totalProgress(): number {
    const running = this.tasks.filter(t => t.status === "running")
    if (running.length === 0) return 0
    return running.reduce((s, t) => s + t.progress, 0) / running.length
  },

  get totalSpeedBps(): number {
    return this.tasks
      .filter(t => t.status === "running")
      .reduce((s, t) => s + t.speedBps, 0)
  },

  /** 聚合预计剩余时间：总剩余字节 ÷ 总速度；总速度过低（<1KB/s，停滞）时 null。 */
  get totalEtaSeconds(): number | null {
    const running = this.tasks.filter(t => t.status === "running")
    if (running.length === 0) return null
    const remain = running.reduce(
      (s, t) => s + Math.max(0, t.totalBytes - t.completedBytes),
      0,
    )
    const speed = running.reduce((s, t) => s + t.speedBps, 0)
    if (remain <= 0 || speed < 1024) return null
    return Math.ceil(remain / speed)
  },

  get remainingFiles(): number {
    return this.tasks
      .filter(t => t.status === "running")
      .reduce((s, t) => s + (t.totalFiles - t.completedFiles), 0)
  },

  addTask(task: TaskItem) {
    const idx = this.tasks.findIndex(t => t.taskId === task.taskId)
    if (idx >= 0) {
      // 竞态：startTask 的 invoke 返回与首个 task-progress 事件谁先到不确定；
      // 事件已自动注册（同 taskId）时合并
      this.tasks[idx] = { ...task, ...this.tasks[idx] }
      return
    }
    this.tasks.push(task)
  },

  removeTask(taskId: string) {
    const idx = this.tasks.findIndex(t => t.taskId === taskId)
    if (idx >= 0) this.tasks.splice(idx, 1)
  },

  async startTask(
    versionId: string,
    versionUrl: string,
    mcVersion: string,
    mainLoader?: string,
    mainLoaderVersion?: string,
    optifinePatch?: string,
    versionDirName?: string,
    fabricApiUrl?: string,
    fabricApiVersion?: string,
  ) {
    // 统一入口：散参组装成 DownloadTask 单对象提交（字段名 snake_case）
    const task: DownloadTask = {
      version_id: versionId,
      version_url: versionUrl,
      mc_version: mcVersion,
      loader:
        mainLoader && mainLoaderVersion
          ? { kind: mainLoader as LoaderChoice["kind"], version: mainLoaderVersion }
          : null,
      optifine_patch: optifinePatch || null,
      version_dir_name: versionDirName || null,
      fabric_api:
        fabricApiUrl && fabricApiVersion
          ? { url: fabricApiUrl, version: fabricApiVersion }
          : null,
    }
    const taskId = await invoke<string>("start_download", { task })
    this.addTask({
      taskId,
      name: "",
      status: "pending",
      progress: 0,
      speedBps: 0,
      etaSeconds: null,
      completedFiles: 0,
      totalFiles: 0,
      completedBytes: 0,
      totalBytes: 0,
      currentPhase: "等待中",
      steps: [],
    })
    // 竞态校正：任务可能极快终态（error/done 事件先于预注册到达被忽略，
    // 卡片以 pending 残留且不再有终态事件）。终态任务已从后端移除，
    // 查不到即已结束——移除卡片。
    // 终态事件已处理（terminalTaskIds 命中）时不重复提示；事件丢失的
    // 场景只做中性提示（极快失败或完成都有可能，不弹错误 dialog 误报）。
    const exists = await invoke<boolean>("task_exists", { taskId })
    if (!exists) {
      const alreadyHandled = terminalTaskIds.has(taskId)
      terminalTaskIds.add(taskId)
      this.removeTask(taskId)
      if (!alreadyHandled) {
        showToast("任务已结束（可能极快完成或失败）")
      }
    }
    return taskId
  },

  async cancelTask(taskId: string) {
    await invoke("cancel_task", { taskId })
  },

  /** 任务完成回调：第一参为完成提示文案；第三参为版本下载任务的安装目录名
   *  （下载完成自动选中该版本，供 Mod 下载目标使用） */
  onTaskDone: null as ((message: string, kind?: TaskKind, versionDir?: string) => void) | null,
  /** 任务失败回调：弹出错误 dialog（失败资源由后端清理）。 */
  onTaskError: null as ((taskName: string, error: string) => void) | null,
})

// ── Event listener ──────────────────────────

let listening = false

// 已收到终态事件/已移除的任务 id：防迟到的 running 事件让已移除卡片"幽灵复活"
const terminalTaskIds = new Set<string>()

export function ensureListen() {
  if (listening) return
  listening = true
  listen<WaitingUserPayload>(EVT_TASK_WAITING_USER, (ev) => {
    // 批量文件下载失败挂起：弹窗询问 跳过/重试/取消
    waitingUser.value = ev.payload
  })
  listen<ProgressPayload>(EVT_TASK_PROGRESS, (ev) => {
    const p = ev.payload
    const idx = dlState.tasks.findIndex(t => t.taskId === p.task_id)
    if (idx >= 0) {
      if (p.status === "done") {
        // 任务完成：从前端清理（触发完成通知后移除）
        terminalTaskIds.add(p.task_id)
        const kind = dlState.tasks[idx]?.kind;
        // 社区资源单文件下载注册了资源名 → "下载任务[资源名]完成"；其余任务用原名
        const mapped = taskNames.get(p.task_id);
        taskNames.delete(p.task_id);
        const message = mapped
          ? `下载任务[${mapped}]完成`
          : `${p.name || "下载任务"} 已完成`;
        // 版本下载完成 → 携带安装目录名（App.vue 据此自动选中该版本）
        const vdir = versionTargets.get(p.task_id);
        versionTargets.delete(p.task_id);
        dlState.onTaskDone?.(message, kind, vdir);
        dlState.removeTask(p.task_id);
        return
      }
      if (p.status === "error") {
        // 任务失败：弹错误 dialog + 清理卡片（残留由后端清理）
        terminalTaskIds.add(p.task_id)
        dlState.onTaskError?.(p.name || "下载任务", p.error || "未知错误");
        dlState.removeTask(p.task_id);
        return
      }
      dlState.tasks[idx].name = p.name
      dlState.tasks[idx].status = p.status as TaskItem["status"]
      dlState.tasks[idx].progress = p.progress
      dlState.tasks[idx].speedBps = p.speed_bps
      dlState.tasks[idx].etaSeconds = p.eta_seconds
      dlState.tasks[idx].completedFiles = p.completed_files
      dlState.tasks[idx].totalFiles = p.total_files
      dlState.tasks[idx].completedBytes = p.completed_bytes
      dlState.tasks[idx].totalBytes = p.total_bytes
      dlState.tasks[idx].currentPhase = p.current_phase
      dlState.tasks[idx].steps = p.steps
      dlState.tasks[idx].error = p.error ?? undefined
    } else if (
      p.status !== "done" && p.status !== "cancelled" && p.status !== "error"
      && !terminalTaskIds.has(p.task_id)
    ) {
      // 后端直接创建的任务（如补全文件）不在前端列表中：首次事件自动注册，
      // 保证任务中心徽标与进度可见。
      // done/cancelled/error 均为终态：done 与 error 走上方移除逻辑；cancelled 由取消
      // 流程（cancelTask + removeTask）主动清理。终态事件重复到达（如 EmitLoop
      // 退出的最后一轮）时卡片已移除，不应重新注册（否则幽灵复活）。
      dlState.addTask({
        taskId: p.task_id,
        name: p.name,
        status: p.status as TaskItem["status"],
        progress: p.progress,
        speedBps: p.speed_bps,
        etaSeconds: p.eta_seconds,
        completedFiles: p.completed_files,
        totalFiles: p.total_files,
        completedBytes: p.completed_bytes,
        totalBytes: p.total_bytes,
        currentPhase: p.current_phase,
        steps: p.steps,
        error: p.error ?? undefined,
      })
    }
  })
}

// ── 中断任务恢复（#6 一期）：上次进程退出未完成的任务 → 询问/恢复/忽略 ──

/** 后端中断任务条目（镜像 Rust task::store::InterruptedJob）。 */
export interface InterruptedJob {
  id: string;
  name: string;
  kind: string;
  updated_at: number;
}

/** task-waiting-user 事件 payload（镜像 Rust task::files::WaitingUserPayload）。 */
export interface WaitingUserPayload {
  task_id: string;
  file_name: string;
  error: string;
}

/** 挂起等待用户决策的文件下载任务（非 null 时前端弹窗）。 */
export const waitingUser = ref<WaitingUserPayload | null>(null);

/** 对挂起任务作出决策：skip=跳过该文件继续，retry=重试，cancel=取消任务。 */
export async function decideWaitingTask(action: "skip" | "retry" | "cancel") {
  const w = waitingUser.value;
  if (!w) return;
  waitingUser.value = null; // 先关弹窗（决策后任务侧继续/结束）
  try {
    await invoke("decide_waiting_task", { taskId: w.task_id, action });
  } catch (e) {
    showToast(errMsg(e));
  }
}

/** 上次未完成任务列表（App.vue 挂载时拉取；DownloadDetail 横幅展示）。 */
export const interruptedJobs = ref<InterruptedJob[]>([]);

/** 应用启动时检查：拉取上次未完成的任务（进程崩溃/重启残留）。 */
export async function checkInterrupted() {
  try {
    interruptedJobs.value = await invoke<InterruptedJob[]>("list_interrupted_tasks");
  } catch {
    interruptedJobs.value = [];
  }
}

/**
 * 恢复全部中断任务：逐个 resume_task（后端读快照重跑，已下载文件 sha1 命中自动跳过，
 * 等价断点续跑）。恢复的任务经 task-progress 事件自动注册进任务中心。
 */
export async function resumeAllInterrupted() {
  const jobs = [...interruptedJobs.value];
  let failed = 0;
  for (const j of jobs) {
    try {
      await invoke("resume_task", { taskId: j.id });
    } catch {
      failed++;
    }
  }
  interruptedJobs.value = [];
  if (failed === 0) {
    showToast(`已恢复 ${jobs.length} 个未完成的任务`);
  } else if (jobs.length > 0) {
    showToast(`恢复完成（${failed} 个失败，可能已过期）`);
  }
}

/** 忽略（放弃恢复）：删除后端快照，本次及后续启动不再提示。 */
export async function dismissInterrupted(jobId: string) {
  try {
    await invoke("dismiss_interrupted_task", { taskId: jobId });
  } catch {
    // 忽略失败不影响本地清理
  }
  interruptedJobs.value = interruptedJobs.value.filter((j) => j.id !== jobId);
}

/** 忽略全部中断任务（放弃恢复）：横幅一键清空，避免每个任务一个忽略按钮。 */
export async function dismissAllInterrupted() {
  const jobs = [...interruptedJobs.value];
  interruptedJobs.value = [];
  for (const j of jobs) {
    try {
      await invoke("dismiss_interrupted_task", { taskId: j.id });
    } catch {
      // 忽略失败不影响本地清理
    }
  }
}
