import { reactive } from "vue"
import { listen } from "@tauri-apps/api/event"
import { EVT_TASK_PROGRESS } from "./events"
import { invoke } from "@tauri-apps/api/core"

export type StepStatus = "pending" | "running" | "done";

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
  completedFiles: number
  totalFiles: number
  completedBytes: number
  totalBytes: number
  currentPhase: string
  steps: StepInfo[]
  error?: string
  previousRoute: string
}

interface ProgressPayload {
  task_id: string
  name: string
  status: string
  progress: number
  speed_bps: number
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

export const dlState = reactive({
  tasks: [] as TaskItem[],

  /** FAB 显示条件：有进行中/等待中任务，或任务失败（点击查看错误详情）。 */
  get anyActive(): boolean {
    return this.tasks.some(t =>
      t.status === "running" || t.status === "pending" || t.status === "error",
    )
  },

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

  get remainingFiles(): number {
    return this.tasks
      .filter(t => t.status === "running")
      .reduce((s, t) => s + (t.totalFiles - t.completedFiles), 0)
  },

  addTask(task: TaskItem) {
    const idx = this.tasks.findIndex(t => t.taskId === task.taskId)
    if (idx >= 0) {
      // 竞态：startTask 的 invoke 返回与首个 task-progress 事件谁先到不确定；
      // 事件已自动注册（同 taskId）时合并，保留调用方传入的 previousRoute
      this.tasks[idx] = { ...task, ...this.tasks[idx], previousRoute: task.previousRoute }
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
    prevRoute: string,
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
      completedFiles: 0,
      totalFiles: 0,
      completedBytes: 0,
      totalBytes: 0,
      currentPhase: "等待中",
      steps: [],
      previousRoute: prevRoute,
    })
    return taskId
  },

  async cancelTask(taskId: string) {
    await invoke("cancel_task", { taskId })
  },

  onTaskDone: null as ((taskName: string, prevRoute: string) => void) | null,
  /** 任务失败回调：弹出错误 dialog + 跳回来源页（失败资源由后端清理）。 */
  onTaskError: null as ((taskName: string, error: string, prevRoute: string) => void) | null,
})

// ── Event listener ──────────────────────────

let listening = false

export function ensureListen() {
  if (listening) return
  listening = true
  listen<ProgressPayload>(EVT_TASK_PROGRESS, (ev) => {
    const p = ev.payload
    const idx = dlState.tasks.findIndex(t => t.taskId === p.task_id)
    if (idx >= 0) {
      if (p.status === "done") {
        // 任务完成：从前端清理（触发完成通知后移除）
        const prev = dlState.tasks[idx]?.previousRoute;
        dlState.onTaskDone?.(p.name || "下载任务", prev || "/");
        dlState.removeTask(p.task_id);
        return
      }
      if (p.status === "error") {
        // 任务失败：弹错误 dialog + 清理卡片并退出下载管理（残留由后端清理）
        const prev = dlState.tasks[idx]?.previousRoute;
        dlState.onTaskError?.(p.name || "下载任务", p.error || "未知错误", prev || "/");
        dlState.removeTask(p.task_id);
        return
      }
      dlState.tasks[idx].name = p.name
      dlState.tasks[idx].status = p.status as TaskItem["status"]
      dlState.tasks[idx].progress = p.progress
      dlState.tasks[idx].speedBps = p.speed_bps
      dlState.tasks[idx].completedFiles = p.completed_files
      dlState.tasks[idx].totalFiles = p.total_files
      dlState.tasks[idx].completedBytes = p.completed_bytes
      dlState.tasks[idx].totalBytes = p.total_bytes
      dlState.tasks[idx].currentPhase = p.current_phase
      dlState.tasks[idx].steps = p.steps
      dlState.tasks[idx].error = p.error ?? undefined
    } else if (p.status !== "done" && p.status !== "cancelled" && p.status !== "error") {
      // 后端直接创建的任务（如补全文件）不在前端列表中：首次事件自动注册，
      // 保证下载管理 FAB 与进度可见。
      // done/cancelled/error 均为终态：done 与 error 走上方移除逻辑；cancelled 由取消
      // 流程（cancelTask + removeTask）主动清理。终态事件重复到达（如 EmitLoop
      // 退出的最后一轮）时卡片已移除，不应重新注册（否则幽灵复活）。
      dlState.addTask({
        taskId: p.task_id,
        name: p.name,
        status: p.status as TaskItem["status"],
        progress: p.progress,
        speedBps: p.speed_bps,
        completedFiles: p.completed_files,
        totalFiles: p.total_files,
        completedBytes: p.completed_bytes,
        totalBytes: p.total_bytes,
        currentPhase: p.current_phase,
        steps: p.steps,
        error: p.error ?? undefined,
        previousRoute: "/",
      })
    }
  })
}
