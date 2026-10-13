import { reactive, ref } from "vue"
import { listen } from "@tauri-apps/api/event"
import { TASK_PROGRESS, TASK_WAITING_USER } from "../app/events";
import type { StepStatus, StepInfo, ProgressPayload, WaitingUserPayload } from "../app/events";
// payload 类型定义在 events.ts，re-export 保持既有 import 路径。
export type { StepStatus, StepInfo, ProgressPayload, WaitingUserPayload };
import { invoke } from "@tauri-apps/api/core"
import { showToast } from "../../utils/toast"
import { errMsg } from "../../utils/error"

export type TaskKind = "jdk";

export interface TaskItem {
  taskId: string
  name: string
  status: "pending" | "running" | "done" | "error" | "cancelled" | "paused"
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
  kind?: TaskKind
}

// ── 任务入参（ts-rs 生成物，勿手写镜像；snake_case 与 Rust 一致）──

import type { DownloadTask } from "../../types/task/DownloadTask";
import type { FabricApi } from "../../types/task/FabricApi";
import type { LoaderChoice } from "../../types/task/LoaderChoice";
export type { DownloadTask, FabricApi, LoaderChoice };
export type { DownloadFileKind } from "../../types/task/DownloadFileKind";

/** taskId → 资源名（社区单文件下载注册；done 消费后删）。 */
export const taskNames = new Map<string, string>();

export function registerTaskName(taskId: string, name: string) {
  taskNames.set(taskId, name);
}

/** taskId → 安装目录名（下载完成后自动选中为 Mod 下载目标；done 消费后删）。 */
export const versionTargets = new Map<string, string>();

export function registerVersionTarget(taskId: string, dirName: string) {
  versionTargets.set(taskId, dirName);
}

/** taskId → 完成回调（消费后即删）：特殊任务经统一监听接续自身流程。 */
export const taskDoneHooks = new Map<string, () => void>();

/** 注册任务完成钩子：任务 done 时回调一次（error/cancel 不触发且清除）。 */
export function registerTaskDoneHook(taskId: string, onDone: () => void) {
  taskDoneHooks.set(taskId, onDone);
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

  /** 聚合剩余时间：总剩余字节 ÷ 总速度；速度过低（<1KB/s，停滞）时 null。 */
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
      // 竞态：startTask 返回与首个 progress 事件先后不定，同 taskId 合并。
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
    await this.adoptTask(taskId, { name: "" })
    return taskId
  },

  /** 统一任务接管：预注册卡片 + 极快终态竞态校正。invoke 返回与首个 progress 事件先后不定，同 taskId 合并；
   * 终态事件先到被忽略致卡片残留 pending 时，后端查不到即移除卡片（terminalTaskIds 防重复提示）。 */
  async adoptTask(
    taskId: string,
    init: Partial<TaskItem> & { name: string },
    silentFastExit = false,
  ) {
    this.addTask({
      taskId,
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
      ...init,
    })
    const exists = await invoke<boolean>("task_exists", { taskId })
    if (!exists) {
      const alreadyHandled = terminalTaskIds.has(taskId)
      terminalTaskIds.add(taskId)
      this.removeTask(taskId)
      if (!alreadyHandled && !silentFastExit) {
        showToast("任务已结束（可能极快完成或失败）")
      }
    }
  },

  async cancelTask(taskId: string) {
    // 主动取消即终态：先标 terminal，防迟到事件让卡片复活/重建。
    terminalTaskIds.add(taskId)
    await invoke("cancel_task", { taskId })
    // 取消不走 done 分发，钩子就地清除防泄漏
    taskDoneHooks.delete(taskId)
  },

  /** 暂停：停下载、保留目录与快照，卡片本地标 paused；后端收尾 emit 的 cancelled 事件对 paused 任务忽略。 */
  async pauseTask(taskId: string) {
    await invoke("pause_task", { taskId })
    const idx = this.tasks.findIndex(t => t.taskId === taskId)
    if (idx >= 0) this.tasks[idx].status = "paused"
  },

  /** 继续：后端读快照重跑（已完成文件 sha1 跳过），新任务经事件自动注册；移除旧卡片。 */
  async resumePausedTask(taskId: string) {
    await invoke("resume_task", { taskId })
    this.removeTask(taskId)
  },

  /** 放弃：删快照（重启不再提示恢复）+ 移除卡片。 */
  async dismissPausedTask(taskId: string) {
    try { await invoke("dismiss_interrupted_task", { taskId }) } catch { /* 快照已删则忽略 */ }
    this.removeTask(taskId)
  },

  /** 任务完成回调：第一参为完成提示文案；第三参为版本下载任务的安装目录名（下载完成自动选中该版本）。 */
  onTaskDone: null as ((message: string, kind?: TaskKind, versionDir?: string) => void) | null,
  /** 任务失败回调：弹出错误 dialog（失败资源由后端清理）。 */
  onTaskError: null as ((taskName: string, error: string) => void) | null,
})

// ── Event listener ──────────────────────────

let listening = false

// 已收到终态/已移除的任务 id：防迟到 running 事件让卡片复活
const terminalTaskIds = new Set<string>()

export function ensureListen() {
  if (listening) return
  listening = true
  listen<WaitingUserPayload>(TASK_WAITING_USER, (ev) => {
    // 批量下载失败挂起：弹窗询问 跳过/重试/取消
    waitingUser.value = ev.payload
  })
  listen<ProgressPayload>(TASK_PROGRESS, (ev) => {
    const p = ev.payload
    const idx = dlState.tasks.findIndex(t => t.taskId === p.task_id)
    if (idx >= 0) {
      if (p.status === "done") {
        terminalTaskIds.add(p.task_id)
        const kind = dlState.tasks[idx]?.kind;
        const mapped = taskNames.get(p.task_id);
        taskNames.delete(p.task_id);
        const message = mapped
          ? `下载任务[${mapped}]完成`
          : `${p.name || "下载任务"} 已完成`;
        // 版本下载完成：携带安装目录名（App.vue 据此自动选中版本）
        const vdir = versionTargets.get(p.task_id);
        versionTargets.delete(p.task_id);
        dlState.onTaskDone?.(message, kind, vdir);
        dlState.removeTask(p.task_id);
        // 特殊任务（JDK 等）完成接续：此处分发，消费后即删
        const doneHook = taskDoneHooks.get(p.task_id);
        taskDoneHooks.delete(p.task_id);
        doneHook?.();
        return
      }
      if (p.status === "error") {
        // 任务失败：弹 dialog + 清理卡片
        terminalTaskIds.add(p.task_id)
        // 终态即清理资源名/版本目标映射，防条目泄漏
        taskNames.delete(p.task_id)
        versionTargets.delete(p.task_id)
        dlState.onTaskError?.(p.name || "下载任务", p.error || "未知错误");
        dlState.removeTask(p.task_id);
        taskDoneHooks.delete(p.task_id);
        return
      }
      // 暂停后端的收尾 cancelled 事件须忽略，保留 paused 卡片（见 pauseTask）
      if (p.status === "cancelled" && dlState.tasks[idx].status === "paused") return
      if (p.status === "cancelled") {
        // 用户取消：与 done/error 一致走终态清理，防卡片停留 cancelled 无入口。
        terminalTaskIds.add(p.task_id)
        taskNames.delete(p.task_id)
        versionTargets.delete(p.task_id)
        taskDoneHooks.delete(p.task_id)
        dlState.removeTask(p.task_id)
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

// ── 中断任务恢复：上次进程退出未完成的任务 → 询问/恢复/忽略 ──

/** 后端中断任务条目（镜像 Rust task::store::InterruptedJob）。 */
export interface InterruptedJob {
  id: string;
  name: string;
  kind: string;
  updated_at: number;
}

export const waitingUser = ref<WaitingUserPayload | null>(null);

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

export const interruptedJobs = ref<InterruptedJob[]>([]);

export async function checkInterrupted() {
  try {
    interruptedJobs.value = await invoke<InterruptedJob[]>("list_interrupted_tasks");
  } catch {
    interruptedJobs.value = [];
  }
}

/** 恢复全部中断任务：逐个 resume_task（后端读快照重跑，已完成文件 sha1 命中跳过）。 */
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

/** 忽略全部中断任务（清空横幅）。 */
export async function dismissAllInterrupted() {
  const jobs = [...interruptedJobs.value];
  interruptedJobs.value = [];
  for (const j of jobs) {
    try {
      await invoke("dismiss_interrupted_task", { taskId: j.id });
    } catch {
    }
  }
}

/** 缺失文件扫描结果（镜像 Rust commands/version.rs 的 MissingReport）。 */
export interface MissingReport {
  client_jar: boolean;
  libraries: number;
  assets: number;
  natives: number;
}

export async function scanMissingFiles(versionDirName: string): Promise<MissingReport> {
  return await invoke<MissingReport>("scan_missing_files", { versionDirName });
}

export async function submitMissingDownload(versionDirName: string): Promise<string> {
  return await invoke<string>("submit_missing_download", { versionDirName });
}
