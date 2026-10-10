// 启动器日志（exe 旁 Elytra/launcher.log）的读取与目录打开。
import { invoke } from "@tauri-apps/api/core";

/** 读取启动器日志全文（文件尚未产生内容时返回空串）。 */
export async function loadLauncherLog(): Promise<string> {
  return await invoke<string>("get_launcher_log");
}

/** 打开启动器日志所在目录。 */
export async function openLauncherLogDir(): Promise<void> {
  await invoke("open_launcher_log_dir");
}
