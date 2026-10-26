// 启动器更新检查：只查 GitHub Releases latest，提示新版本并跳转下载页；不自动下载/安装。
import { ref } from "vue";
import { invoke } from "@tauri-apps/api/core";

export interface LauncherUpdateInfo {
  current_version: string;
  latest_version: string;
  update_available: boolean;
  url: string;
  notes: string;
  published_at: string | null;
}

export type UpdateCheckState = "idle" | "checking" | "available" | "latest" | "none";

export const updateState = ref<UpdateCheckState>("idle");
export const updateInfo = ref<LauncherUpdateInfo | null>(null);

export async function checkLauncherUpdate(): Promise<void> {
  if (updateState.value === "checking") return;
  updateState.value = "checking";
  try {
    const info = await invoke<LauncherUpdateInfo | null>("check_launcher_update");
    updateInfo.value = info;
    updateState.value = info ? (info.update_available ? "available" : "latest") : "none";
  } catch (e) {
    updateState.value = "idle";
    updateInfo.value = null;
    throw e;
  }
}
