// 认证状态单例（Launch / VersionSetup 共用）；收敛 authMode 等 localStorage 键。
import { ref, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { MS_LOGIN_COMPLETE } from "../app/events";
import { showError } from "../app/alert";
import { t } from "../../i18n";
import { avatarState, renderPremiumAvatar, renderExternalAvatar } from "../auth/avatar";
import type { MsAccount as MsAccountInfo } from "../../types/auth/MsAccount";
import type { ExternalAccount as ExternalAccountInfo } from "../../types/auth/ExternalAccount";
import type { ExternalProfile } from "../../types/auth/ExternalProfile";
export type { MsAccountInfo, ExternalAccountInfo, ExternalProfile };

export type AuthMode = "premium" | "offline" | "external";

export const AUTH_MODE_KEY = "elytra.authMode";

/** 持久化的认证模式；旧版本存 "thirdparty"，读取时按 "external" 兼容。 */
const storedAuthMode = localStorage.getItem(AUTH_MODE_KEY);
export const authMode = ref<AuthMode>(
  storedAuthMode === "offline"
    ? "offline"
    : storedAuthMode === "external" || storedAuthMode === "thirdparty"
      ? "external"
      : "premium",
);
/** 当前第三方账号（列表末尾 = 最近登录/切换）。 */
export const externalAccount = ref<ExternalAccountInfo | null>(null);
export const externalAccounts = ref<ExternalAccountInfo[]>([]);
export const msAccount = ref<MsAccountInfo | null>(null);
/** 全部已保存正版账号（登录时间升序，末尾 = 当前）。 */
export const msAccounts = ref<MsAccountInfo[]>([]);
/** 离线账号历史用户名（最近使用升序，末尾 = 当前） */
export const offlineAccounts = ref<string[]>([]);
export const username = ref("");

watch(msAccount, () => {
  avatarState.avatarError = false;
  void renderPremiumAvatar(msAccount.value?.uuid ?? "");
});

watch(externalAccount, () => {
  avatarState.avatarError = false;
  void renderExternalAvatar(externalAccount.value?.api_url ?? "", externalAccount.value?.uuid ?? "");
});

export function setAuthMode(mode: AuthMode) {
  authMode.value = mode;
  localStorage.setItem(AUTH_MODE_KEY, mode);
  // 模式切换清掉失败标记（premium 失败后切离线/第三方不该被挡住）
  avatarState.avatarError = false;
}

export async function loadMsAccount() {
  try {
    msAccount.value = await invoke<MsAccountInfo | null>("get_ms_account");
  } catch {
    msAccount.value = null;
  }
}

export async function loadExternalAccounts(): Promise<boolean> {
  try {
    externalAccounts.value = (await invoke<ExternalAccountInfo[]>("get_external_accounts")) ?? [];
    externalAccount.value = externalAccounts.value[externalAccounts.value.length - 1] ?? null;
    return true;
  } catch {
    externalAccounts.value = [];
    externalAccount.value = null;
    return false;
  }
}

/** 账号状态就绪 Promise（模块级单次）：判定认证模式前必须等异步 IPC 就绪；幂等，启动页与衣柜共用。 */
let authReadyPromise: Promise<void> | null = null;

/** 等账号状态就绪（正版 + 第三方列表读完）；读取失败不留缓存，下次调用可重试。 */
export function ensureAuthReady(): Promise<void> {
  authReadyPromise ??= Promise.all([loadMsAccounts(), loadExternalAccounts()]).then(([ms, ext]) => {
    if (!ms || !ext) authReadyPromise = null;
  });
  return authReadyPromise;
}

export async function loginExternal(apiUrl: string, account: string, password: string) {
  const acc = await invoke<ExternalAccountInfo>("external_login", {
    apiUrl,
    username: account,
    password,
  });
  await loadExternalAccounts();
  return acc;
}

export async function removeExternalAccount(apiUrl: string, uuid: string) {
  await invoke("remove_external_account", { apiUrl, uuid });
  await loadExternalAccounts();
}

export async function switchExternalAccount(apiUrl: string, uuid: string) {
  await invoke("switch_external_account", { apiUrl, uuid });
  await loadExternalAccounts();
}

/** 刷新第三方账号（refresh：新 token + 角色列表同步）；凭据失效时后端会注销该账号，故失败后也同步列表。 */
export async function refreshExternalAccount(apiUrl: string, uuid: string) {
  try {
    await invoke<ExternalAccountInfo>("refresh_external_account", { apiUrl, uuid });
  } catch (e) {
    await loadExternalAccounts().catch(() => {
    });
    throw e;
  }
  await loadExternalAccounts();
}

export async function switchExternalRole(apiUrl: string, uuid: string, profileId: string, profileName: string) {
  const acc = await invoke<ExternalAccountInfo>("switch_external_role", {
    apiUrl,
    uuid,
    profileId,
    profileName,
  });
  await loadExternalAccounts();
  return acc;
}

export async function loadMsAccounts(): Promise<boolean> {
  try {
    msAccounts.value = (await invoke<MsAccountInfo[]>("get_ms_accounts")) ?? [];
    msAccount.value = msAccounts.value[msAccounts.value.length - 1] ?? null;
    return true;
  } catch {
    msAccounts.value = [];
    msAccount.value = null;
    return false;
  }
}

/** 切换当前正版账号：后端按 uuid refresh_token 静默刷新后置为当前。 */
export async function switchMsAccount(uuid: string) {
  const acct = await invoke<MsAccountInfo>("switch_ms_account", { uuid });
  msAccount.value = acct;
  await loadMsAccounts(); // 刷新列表顺序（被切换账号移到末尾）
}

/** 移除指定正版账号（当前被移除时回退到上一账号）。 */
export async function removeMsAccount(uuid: string) {
  await invoke("remove_ms_account", { uuid });
  await loadMsAccounts();
}

export async function loadOfflineAccounts() {
  try {
    offlineAccounts.value = (await invoke<string[]>("get_offline_accounts")) ?? [];
  } catch {
    offlineAccounts.value = [];
  }
}

/** 保存离线用户名到历史（输入即存；已存在则置为末尾）。 */
export async function saveOfflineAccount(name: string) {
  try {
    await invoke("save_offline_account", { username: name });
    await loadOfflineAccounts();
  } catch {
    // 保存失败不打断输入，下次输入重试
  }
}

export async function removeOfflineAccount(name: string) {
  try {
    await invoke("remove_offline_account", { username: name });
    await loadOfflineAccounts();
  } catch {
  }
}

/** 读取最后一次离线用户名（与正版隔离）；用户名已填写时不覆盖。 */
export async function loadLastUsername() {
  try {
    const last = await invoke<string>("get_last_offline_username");
    if (last && !username.value) username.value = last;
  } catch {
  }
}

// ── 微软登录流程 ──

export const msLoginState = ref<"idle" | "waiting">("idle");

export async function startMsLogin() {
  if (msLoginState.value === "waiting") return; // 防重复：登录进行中忽略再次点击
  msLoginState.value = "waiting";
  try {
    await invoke("start_ms_login");
    // 结果由 ms-login-complete 事件推送
  } catch (e) {
    msLoginState.value = "idle";
    showError(String(e), true);
  }
}

/** 取消进行中的微软登录：abort 后端回调等待（释放端口），界面回到可重新登录态。 */
export async function cancelMsLogin() {
  msLoginState.value = "idle";
  try {
    await invoke("cancel_ms_login");
  } catch {
    // 端口清理失败不影响前端状态（后端超时 5 分钟后自动释放）
  }
}

/** ms-login-complete 事件 payload（镜像 Rust events::MsLoginCompletePayload）。 */
export interface MsLoginCompletePayload {
  ok: boolean;
  account: MsAccountInfo | null;
  error: string | null;
}

let loginListenerReady = false;

export async function ensureLoginListener() {
  if (loginListenerReady) return;
  loginListenerReady = true;
  await listen<MsLoginCompletePayload>(
    MS_LOGIN_COMPLETE,
    async (ev) => {
      msLoginState.value = "idle";
      if (ev.payload.ok && ev.payload.account) {
        msAccount.value = ev.payload.account;
        await loadMsAccounts(); // 新账号已 upsert 进列表（置为末尾）
      } else {
        showError(ev.payload.error || t("launch.error.loginFailed"), true);
      }
    },
  );
}
