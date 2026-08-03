// 认证状态单例（Launch / VersionSetup 共用）。
// 收敛原先两处手写的 authMode localStorage 键、get_ms_account、get_last_offline_username 契约。
import { ref } from "vue";
import { invoke } from "@tauri-apps/api/core";

export type AuthMode = "premium" | "offline";

export const AUTH_MODE_KEY = "elytra.authMode";

export interface MsAccountInfo {
  uuid: string;
  name: string;
  access_token: string;
  refresh_token: string;
  xuid: string;
  expires_at: number;
}

export const authMode = ref<AuthMode>(
  localStorage.getItem(AUTH_MODE_KEY) === "offline" ? "offline" : "premium",
);
export const msAccount = ref<MsAccountInfo | null>(null);
export const username = ref("");

export function setAuthMode(mode: AuthMode) {
  authMode.value = mode;
  localStorage.setItem(AUTH_MODE_KEY, mode);
}

export async function loadMsAccount() {
  try {
    msAccount.value = await invoke<MsAccountInfo | null>("get_ms_account");
  } catch {
    msAccount.value = null;
  }
}

/** 读取最后一次离线用户名（与正版隔离）；用户名已填写时不覆盖。 */
export async function loadLastUsername() {
  try {
    const last = await invoke<string>("get_last_offline_username");
    if (last && !username.value) username.value = last;
  } catch {
    // ignore
  }
}
