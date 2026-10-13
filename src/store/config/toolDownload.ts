// 工具箱多线程下载器状态机：任务列表（按 id 增量）+ 配置 + 操作命令；与 download.ts 启动器下载任务隔离（独立事件名 tooldl-progress），仅承载手动粘贴的外部 URL。
import { reactive } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { TOOL_DL_PROGRESS } from "../app/events";
import type { ToolDlSnapshot } from "../app/events";
import type { ToolDlConfig } from "../../types/config/ToolDlConfig";

export type { ToolDlSnapshot, ToolDlStatus } from "../app/events";

export const toolDlState = reactive({
  /** 全部任务（入队顺序；终态保留，由用户清除/移除） */
  tasks: [] as ToolDlSnapshot[],

  get activeCount(): number {
    return this.tasks.filter((t) => t.status === "downloading").length;
  },

  get queuedCount(): number {
    return this.tasks.filter((t) => t.status === "queued").length;
  },

  /** 全部活跃任务的总速度（字节/秒） */
  get totalSpeed(): number {
    return this.tasks
      .filter((t) => t.status === "downloading")
      .reduce((s, t) => s + t.speed, 0);
  },

  get finishedCount(): number {
    return this.tasks.filter(
      (t) => t.status === "done" || t.status === "error" || t.status === "cancelled",
    ).length;
  },
});

let listening = false;

/** 幂等注册 tooldl-progress 监听（模块级单次，路由往返不累积）。 */
export function ensureToolDlListen() {
  if (listening) return;
  listening = true;
  listen<ToolDlSnapshot>(TOOL_DL_PROGRESS, (ev) => {
    applySnapshot(ev.payload);
  });
}

function applySnapshot(snap: ToolDlSnapshot) {
  const idx = toolDlState.tasks.findIndex((t) => t.id === snap.id);
  if (idx >= 0) {
    toolDlState.tasks[idx] = snap;
  } else {
    toolDlState.tasks.push(snap);
  }
}

export async function loadToolDlTasks() {
  toolDlState.tasks = await invoke<ToolDlSnapshot[]>("tooldl_list");
}

export async function loadToolDlConfig(): Promise<ToolDlConfig> {
  return await invoke<ToolDlConfig>("tooldl_get_config");
}

export async function saveToolDlConfig(config: ToolDlConfig) {
  await invoke("tooldl_save_config", { config });
}

/** 入队一批链接（后端按行忽略空串，非 http(s) 直接报错）。 */
export async function enqueueToolDl(urls: string[], saveDir: string, segments: number) {
  await invoke<string[]>("tooldl_enqueue", { urls, saveDir, segments });
}

export async function pauseToolDl(id: string) {
  await invoke("tooldl_pause", { taskId: id });
}

export async function resumeToolDl(id: string) {
  await invoke("tooldl_resume", { taskId: id });
}

export async function cancelToolDl(id: string) {
  await invoke("tooldl_cancel", { taskId: id });
}

export async function removeToolDl(id: string) {
  await invoke("tooldl_remove", { taskId: id });
}

export async function clearFinishedToolDl() {
  await invoke("tooldl_clear_finished");
}

export async function openToolDlFolder(dest: string) {
  const dir = dest.replace(/[\\/][^\\/]*$/, "");
  await invoke("open_folder", { path: dir });
}

/** 剩余时间（秒）：速度过低（<1KB/s）或总量未知时 null。 */
export function toolDlEta(task: ToolDlSnapshot): number | null {
  if (task.status !== "downloading" || task.total <= 0 || task.speed < 1024) return null;
  const remain = task.total - task.downloaded;
  return remain > 0 ? Math.ceil(remain / task.speed) : null;
}
