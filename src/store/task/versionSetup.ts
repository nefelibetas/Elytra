// 版本管理页（VersionSetup）的 IPC 封装：导出启动脚本/整合包、版本级服务器直连配置（组件不直接 invoke）。
import { invoke } from "@tauri-apps/api/core";
import type { InstanceLaunchOptions } from "../../types/launcher/InstanceLaunchOptions";
import type { InstanceLaunchState } from "../../types/launcher/InstanceLaunchState";

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

export type ModpackExportFormat = "mrpack" | "curseforge";

export interface ModpackExportResult {
  format: ModpackExportFormat;
  matched: number;
  bundled: number;
}

export async function exportModpack(
  versionName: string,
  savePath: string,
  format: ModpackExportFormat,
): Promise<ModpackExportResult> {
  return await invoke<ModpackExportResult>("export_modpack", { versionName, savePath, format });
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

/** 实例级启动参数（全局快照 + 实例快照 + 实际生效值）。 */
export function getInstanceLaunchOptions(versionId: string): Promise<InstanceLaunchState> {
  return invoke<InstanceLaunchState>("get_instance_launch_options", { versionId });
}

/** 保存实例启动参数；null = 删除 [launch] 段并回退全局。 */
export function saveInstanceLaunchOptions(
  versionId: string,
  options: InstanceLaunchOptions | null,
): Promise<void> {
  return invoke("save_instance_launch_options", { versionId, options });
}
