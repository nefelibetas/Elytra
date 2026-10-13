// 组件不直接 invoke，统一经此入口。
import { invoke } from "@tauri-apps/api/core";

/** 用系统文件管理器打开目录；路径不存在时后端报错，由调用方决定是否忽略。 */
export async function openFolder(path: string): Promise<void> {
  await invoke("open_folder", { path });
}
