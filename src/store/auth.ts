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
  msLoginState.value = "waiting";
  try {
    await invoke("start_ms_login");
    // 结果由 ms-login-complete 事件推送
  } catch (e) {
    msLoginState.value = "idle";
    showError(String(e), true);
  }
}

export async function logoutMs() {
  try {
    await invoke("clear_ms_account");
  } catch {
    // ignore
  }
  msAccount.value = null;
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

let loginListenerReady = false;

/** 幂等注册 ms-login-complete 监听（页面 onMounted 调用一次）。 */
export async function ensureLoginListener() {
  if (loginListenerReady) return;
  loginListenerReady = true;
  await listen<{ ok: boolean; account: MsAccountInfo | null; error: string | null }>(
    EVT_MS_LOGIN_COMPLETE,
    (ev) => {
      msLoginState.value = "idle";
      if (ev.payload.ok && ev.payload.account) {
        msAccount.value = ev.payload.account;
      } else {
        showError(ev.payload.error || "登录失败", true);
      }
    },
  );
}
