// 文件系统相关的前端统一入口（组件不直接 invoke）。
import { invoke } from "@tauri-apps/api/core";

/** 用系统文件管理器打开目录；路径不存在时后端返回错误，由调用方决定是否忽略。 */
export async function openFolder(path: string): Promise<void> {
  await invoke("open_folder", { path });
}
