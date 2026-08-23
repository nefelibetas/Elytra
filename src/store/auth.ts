// 认证状态单例（Launch / VersionSetup 共用）。
// 收敛原先两处手写的 authMode localStorage 键、get_ms_account、get_last_offline_username 契约。
import { ref, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { EVT_MS_LOGIN_COMPLETE } from "./events";
import { showError } from "./alert";
import { avatarState, renderPremiumAvatar } from "./avatar";

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
/** 全部已保存正版账号（登录时间升序，末尾 = 当前），供账号列表菜单使用 */
export const msAccounts = ref<MsAccountInfo[]>([]);
/** 离线账号历史用户名（最近使用升序，末尾 = 当前） */
export const offlineAccounts = ref<string[]>([]);
export const username = ref("");

// 账号变化（登录/退出）时渲染正版头像（模块级注册一次，避免路由往返累积监听）
watch(msAccount, () => {
  avatarState.avatarError = false;
  renderPremiumAvatar(msAccount.value?.uuid ?? "");
});

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

/** 加载全部正版账号（当前 = 列表末尾，即最近登录/切换的）。 */
export async function loadMsAccounts() {
  try {
    msAccounts.value = (await invoke<MsAccountInfo[]>("get_ms_accounts")) ?? [];
    msAccount.value = msAccounts.value[msAccounts.value.length - 1] ?? null;
  } catch {
    msAccounts.value = [];
    msAccount.value = null;
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

/** 读取离线账号历史。 */
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

/** 移除指定离线历史用户名。 */
export async function removeOfflineAccount(name: string) {
  try {
    await invoke("remove_offline_account", { username: name });
    await loadOfflineAccounts();
  } catch {
    // ignore
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

/** 幂等注册 ms-login-complete 监听（页面 onMounted 调用一次）。 */
export async function ensureLoginListener() {
  if (loginListenerReady) return;
  loginListenerReady = true;
  await listen<MsLoginCompletePayload>(
    EVT_MS_LOGIN_COMPLETE,
    async (ev) => {
      msLoginState.value = "idle";
      if (ev.payload.ok && ev.payload.account) {
        msAccount.value = ev.payload.account;
        await loadMsAccounts(); // 新账号已 upsert 进列表（置为末尾）
      } else {
        showError(ev.payload.error || "登录失败", true);
      }
    },
  );
}
