// 第三方皮肤站（authlib-injector）预设配置的 IPC 封装。
import { invoke } from "@tauri-apps/api/core";

import type { ExternalConfig } from "../../types/auth/ExternalConfig";
import type { ExternalServer } from "../../types/auth/ExternalServer";
export type { ExternalConfig, ExternalServer };

/** 读取第三方站点预设（空列表时后端注入内置预设）。 */
export async function loadExternalConfig(): Promise<ExternalConfig> {
  return await invoke<ExternalConfig>("get_third_party_config");
}

export async function saveExternalConfig(config: ExternalConfig): Promise<void> {
  await invoke("save_third_party_config", { config });
}
