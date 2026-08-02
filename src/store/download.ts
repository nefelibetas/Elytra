import { reactive } from "vue"
import { listen } from "@tauri-apps/api/event"
import { invoke } from "@tauri-apps/api/core"

export interface StepInfo {
  name: string
  status: string
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
  current_phase: string
  steps: StepInfo[]
  error?: string | null
}

export const dlState = reactive({
  tasks: [] as TaskItem[],
  previousRoute: "/",
  _seenTasks: {} as Record<string, boolean>,

  get activeTasks(): TaskItem[] {
    return this.tasks.filter(t => t.status === "running" || t.status === "pending")
  },

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
    this.tasks.push(task)
  },

  updateTask(taskId: string, patch: Partial<TaskItem>) {
    const t = this.tasks.find(t => t.taskId === taskId)
    if (t) Object.assign(t, patch)
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
  ) {
    const taskId = await invoke<string>("start_download", {
      versionId,
      versionUrl,
      mcVersion,
      mainLoader: mainLoader || null,
      mainLoaderVersion: mainLoaderVersion || null,
      optifinePatch: optifinePatch || null,
      versionDirName: versionDirName || null,
    })
    this.addTask({
      taskId,
      name: "",
      status: "pending",
      progress: 0,
      speedBps: 0,
      completedFiles: 0,
      totalFiles: 0,
      currentPhase: "等待中",
      steps: [],
      previousRoute: prevRoute,
    })
    return taskId
  },

  async cancelTask(taskId: string) {
    await invoke("cancel_task", { taskId })
  },

  async removeTaskById(taskId: string) {
    await invoke("remove_task", { taskId })
    const idx = this.tasks.findIndex(t => t.taskId === taskId)
    if (idx >= 0) this.tasks.splice(idx, 1)
  },

  onTaskDone: null as ((taskName: string, prevRoute: string) => void) | null,
})

// ── Event listener ──────────────────────────

let listening = false

export function ensureListen() {
  if (listening) return
  listening = true
  listen<ProgressPayload>("task-progress", (ev) => {
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
      dlState.tasks[idx].name = p.name
      dlState.tasks[idx].status = p.status as TaskItem["status"]
      dlState.tasks[idx].progress = p.progress
      dlState.tasks[idx].speedBps = p.speed_bps
      dlState.tasks[idx].completedFiles = p.completed_files
      dlState.tasks[idx].totalFiles = p.total_files
      dlState.tasks[idx].currentPhase = p.current_phase
      dlState.tasks[idx].steps = p.steps
      dlState.tasks[idx].error = p.error ?? undefined
    } else if (p.status !== "done") {
      // 后端直接创建的任务（如补全文件）不在前端列表中：首次事件自动注册，
      // 保证下载管理 FAB 与进度可见。
      dlState.addTask({
        taskId: p.task_id,
        name: p.name,
        status: p.status as TaskItem["status"],
        progress: p.progress,
        speedBps: p.speed_bps,
        completedFiles: p.completed_files,
        totalFiles: p.total_files,
        currentPhase: p.current_phase,
        steps: p.steps,
        error: p.error ?? undefined,
        previousRoute: "/",
      })
    }
  })
}
