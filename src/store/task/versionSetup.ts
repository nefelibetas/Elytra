// 版本管理页（VersionSetup）的 IPC 封装：导出启动脚本/整合包、版本级服务器直连配置。
// 组件不直接 invoke（与其余 store 一致）。
import { invoke } from "@tauri-apps/api/core";

/** 导出启动脚本（BAT/SH）到指定路径。 */
export async function exportLaunchScript(payload: {
  savePath: string;
  versionDirName: string;
  authMode: "online" | "offline";
  username: string;
  accessToken: string | null;
  uuid: string | null;
  xuid: string | null;
}): Promise<void> {
  await invoke("export_launch_script", payload);
}

/** 导出整合包（标准 MRPACK；存档不打包）。 */
export async function exportModpack(versionName: string, savePath: string): Promise<void> {
  await invoke("export_modpack", { versionName, savePath });
}

/** 版本级服务器直连配置（快速进服 --server/--port，per-version 存储）。 */
export interface VersionServerConfig {
  server: string | null;
  port: number | null;
}

export async function loadVersionServer(versionName: string): Promise<VersionServerConfig> {
  return await invoke<VersionServerConfig>("get_version_server", { versionName });
}

export async function saveVersionServer(
  versionName: string,
  server: string | null,
  port: number | null,
): Promise<void> {
  await invoke("save_version_server", { versionName, server, port });
}
