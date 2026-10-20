// 启动流程 store：启动阶段/步骤、版本加载、游戏退出监听；页面跳转参数化（navigateToDownload），不依赖 router。
import { reactive, computed } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { StepStatus } from "../task/download";
import type { InstalledVersionInfo } from "../config/gameDirs";
import { GAME_EXITED, GAME_WINDOW_VISIBLE } from "../app/events";
import { authMode, msAccount, username, externalAccount, refreshExternalAccount, type MsAccountInfo } from "../auth/auth";
import { selectedVersion, setSelected, getSavedDir } from "../app/selectedVersion";
import { showError } from "../app/alert";
import { t } from "../../i18n";
import { errMsg } from "../../utils/error";

export const launchState = reactive({
  /** 启动流程："" 空闲 | "running" 启动中 | "done" 完成 */
  phase: "" as "" | "running" | "done",
  /** key 为 i18n 消息 key：语言切换时步骤文案即时更新。 */
  steps: [] as { key: string; status: StepStatus }[],
});

export const isLaunching = computed(() => launchState.phase === "running");

/** 启动代际号：取消启动时自增，作废在飞流程的落点（doLaunch 每个 await 后校验）。 */
let launchSeq = 0;

/** 已请求取消的启动标签（版本目录名）：被终止进程的 GAME_EXITED（非零退出码）不再按崩溃提示。 */
const cancelledLabels = new Set<string>();

function step(key: string, status: StepStatus) {
  const s = launchState.steps.find(s => s.key === key);
  if (s) s.status = status;
}

function resetLaunch() {
  launchState.phase = "";
  launchState.steps = [];
}

/** 取消本次启动：后端置取消标志（未拉起则阻止启动，已拉起则终止进程），前端立即收回面板。 */
export async function cancelLaunch() {
  if (!isLaunching.value) return;
  launchSeq++;
  if (selectedVersion.value) cancelledLabels.add(selectedVersion.value);
  resetLaunch();
  try {
    await invoke("cancel_launch");
  } catch {
    // 取消失败（后端不可达）不阻断 UI：面板已收回，进程状态由后端事件兜底
  }
}

export async function loadVersions() {
  try {
    const list: InstalledVersionInfo[] = await invoke("get_installed_versions");
    // 仅在所属目录仍是当前激活目录时才恢复（版本选择页同时写入两者）
    const savedDir = getSavedDir();
    if (savedDir) {
      try {
        const active = await invoke<{ path: string }>("get_active_game_dir");
        if (active.path !== savedDir) selectedVersion.value = "";
      } catch {
      }
    }
    if (!list.some(v => v.name === selectedVersion.value) || list.length === 0) {
      try {
        const r = await invoke<{ dir_path: string; version: string }>(
          "auto_activate_version",
        );
        const newList: InstalledVersionInfo[] = await invoke("get_installed_versions");
        selectedVersion.value = newList.some(v => v.name === r.version)
          ? r.version
          : newList[0]?.name ?? "";
        setSelected(selectedVersion.value, r.dir_path);
      } catch {
        selectedVersion.value = list.some(v => v.name === selectedVersion.value)
          ? selectedVersion.value
          : (list[0]?.name ?? "");
      }
    }
  } catch {
  }
}

export async function doLaunch(navigateToDownload: () => void) {
  if (isLaunching.value) return;
  if (!selectedVersion.value) {
    navigateToDownload();
    return;
  }

  const isOffline = authMode.value === "offline";
  const isExternal = authMode.value === "external";

  if (isOffline && !username.value.trim()) {
    showError(t("launch.error.usernameRequired"));
    return;
  }

  // 正版须已登录；启动前静默刷新 token（界面切换在前，避免点击后无反馈）
  if (!isOffline && !isExternal && !msAccount.value) {
    showError(t("launch.error.premiumRequired"));
    return;
  }

  if (isExternal && !externalAccount.value) {
    showError(t("launch.error.externalRequired"));
    return;
  }

  const name = isOffline
    ? username.value.trim()
    : isExternal
      ? (externalAccount.value?.name ?? "")
      : (msAccount.value?.name ?? "");
  launchState.phase = "running";
  launchState.steps = [
    { key: "launch.steps.java", status: "running" },
    { key: "launch.steps.files", status: "pending" },
    { key: "launch.steps.start", status: "pending" },
    { key: "launch.steps.window", status: "pending" },
  ];

  // 本次启动代际号：取消启动会自增，使在飞 await 的落点全部作废（后端同步收到取消请求）
  const seq = ++launchSeq;

  try {
    cancelledLabels.clear(); // 上次取消的抑制标记只对那一次启动有效
    await invoke("begin_launch");
    if (seq !== launchSeq) return;

    // 启动前确保凭据可用：正版后端按需续签（未过期零网络开销）；第三方每次静默续签（Yggdrasil 无过期时间）；离线跳过
    if (isExternal) {
      const ext = externalAccount.value;
      if (ext) {
        try {
          await refreshExternalAccount(ext.api_url, ext.uuid);
        } catch (e) {
          if (seq !== launchSeq) return;
          externalAccount.value = null;
          resetLaunch();
          showError(t("launch.error.externalExpired", { error: errMsg(e) }), true);
          return;
        }
      }
    } else if (!isOffline) {
      try {
        msAccount.value = await invoke<MsAccountInfo>("ensure_ms_account");
      } catch (e) {
        if (seq !== launchSeq) return;
        msAccount.value = null;
        resetLaunch();
        showError(t("launch.error.premiumExpired", { error: errMsg(e) }), true);
        return;
      }
    }
    if (seq !== launchSeq) return;

    // ① 环境 + 完整性校验
    await invoke("check_launch_env", { versionDirName: selectedVersion.value });
    if (seq !== launchSeq) return;
    step("launch.steps.java", "done");
    step("launch.steps.files", "running");
    await new Promise(r => setTimeout(r, 100)); // 让用户看到完整性检查过程
    if (seq !== launchSeq) return;
    step("launch.steps.files", "done");

    // ② 启动游戏进程
    step("launch.steps.start", "running");
    const pid = await invoke("spawn_game", {
      versionDirName: selectedVersion.value,
      authMode: isOffline ? "offline" : isExternal ? "external" : "online",
      username: name,
      accessToken: isOffline
        ? ""
        : isExternal
          ? (externalAccount.value?.access_token ?? "")
          : (msAccount.value?.access_token ?? ""),
      uuid: isOffline
        ? ""
        : isExternal
          ? (externalAccount.value?.uuid ?? "")
          : (msAccount.value?.uuid ?? ""),
      xuid: isOffline || isExternal ? "" : (msAccount.value?.xuid ?? ""),
    });
    if (seq !== launchSeq) return; // 已取消：后端已按取消终止该进程
    step("launch.steps.start", "done");
    step("launch.steps.window", "running");
    console.log("Game started, PID:", pid);

    // 记录最后离线用户名（与正版隔离）
    if (isOffline) {
      await invoke("save_last_offline_username", { username: name });
    }

    // ③ 等待游戏窗口（后端轮询可见窗口推送事件；90s 兜底防卡死）
    step("launch.steps.window", "running");
    const windowShown = await waitGameWindow(selectedVersion.value);
    if (seq !== launchSeq) return;
    step("launch.steps.window", "done");
    launchState.phase = "done";
    if (!windowShown) {
      showError(t("launch.error.windowNotFound", { version: selectedVersion.value }), true);
    }

    setTimeout(() => {
      resetLaunch();
    }, 1500);
  } catch (e) {
    // 已取消：本次流程的落点全部作废（取消路径不放错误弹窗，后端「启动已取消」亦不提示）
    if (seq !== launchSeq) return;
    resetLaunch();
    showError(errMsg(e), true);
  }
}

type GameExitedPayload = [number, string, number | null, string | null];

let gameExitListenerReady = false;

function waitGameWindow(label: string): Promise<boolean> {
  return new Promise((resolve) => {
    let settled = false;
    let windowSeen = false;
    const unlisteners: Array<() => void> = [];
    const finish = (ok: boolean) => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      for (const u of unlisteners) u();
      resolve(ok);
    };
    const timer = setTimeout(() => finish(false), 90_000);
    listen<[number, string]>(GAME_WINDOW_VISIBLE, (ev) => {
      if (ev.payload[1] !== label) return;
      windowSeen = true;
      finish(true);
    }).then(u => unlisteners.push(u));
    listen<GameExitedPayload>(GAME_EXITED, (ev) => {
      if (ev.payload[1] !== label) return;
      finish(windowSeen);
    }).then(u => unlisteners.push(u));
  });
}

/** 最近一次游戏日志（launcher/process.rs 落盘）。 */
export async function loadGameLog(gameDir: string, versionName: string): Promise<string> {
  return await invoke<string>("get_game_log", { gameDir, versionName });
}

export async function ensureGameExitListener() {
  if (gameExitListenerReady) return;
  gameExitListenerReady = true;
  await listen<GameExitedPayload>(GAME_EXITED, (ev) => {
    console.log("Game exited:", ev.payload);
    const [, label, exitCode, crashReport] = ev.payload;
    // 取消启动会终止已拉起的进程（非零退出码）：不算崩溃
    if (exitCode !== null && exitCode !== 0 && !cancelledLabels.has(label)) {
      const key = crashReport ? "launch.error.crashedWithReport" : "launch.error.crashed";
      showError(t(key, { code: exitCode, version: label, report: crashReport ?? "" }), true);
    }
  });
}
