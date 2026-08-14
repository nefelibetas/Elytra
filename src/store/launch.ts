// 启动流程 store：启动阶段/步骤、已安装版本加载、游戏退出事件监听。
// doLaunch 的页面跳转依赖参数化（navigateToDownload），store 不依赖 router。
import { reactive, computed } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { StepStatus } from "./download";
import { EVT_GAME_EXITED, EVT_GAME_WINDOW_VISIBLE } from "./events";
import { authMode, msAccount, username, type MsAccountInfo } from "./auth";
import { selectedVersion, setSelected, getSavedDir } from "./selectedVersion";
import { showError } from "./alert";

export const launchState = reactive({
  /** 启动流程："" 空闲 | "running" 启动中 | "done" 完成 */
  phase: "" as "" | "running" | "done",
  steps: [] as { name: string; status: StepStatus }[],
});

export const isLaunching = computed(() => launchState.phase === "running");

function step(name: string, status: StepStatus) {
  const s = launchState.steps.find(s => s.name === name);
  if (s) s.status = status;
}

function resetLaunch() {
  launchState.phase = "";
  launchState.steps = [];
}

/** 加载已安装版本：恢复上次选择（跨目录校验）+ 无版本时自动激活第一个有版本的目录。 */
export async function loadVersions() {
  try {
    const list: string[] = await invoke("get_installed_versions");
    // 上次选择的版本仅在所属目录仍是当前激活目录时才恢复（版本选择页同时写入两者）
    const savedDir = getSavedDir();
    if (savedDir) {
      try {
        const active = await invoke<{ path: string }>("get_active_game_dir");
        if (active.path !== savedDir) selectedVersion.value = "";
      } catch {
        // 目录信息获取失败时不额外处理
      }
    }
    // 当前目录无可用版本：检测所有目录，激活第一个有版本的目录并选中其第一个版本
    if (!list.includes(selectedVersion.value) || list.length === 0) {
      try {
        const r = await invoke<{ dir_path: string; version: string }>(
          "auto_activate_version",
        );
        const newList = await invoke<string[]>("get_installed_versions");
        selectedVersion.value = newList.includes(r.version)
          ? r.version
          : newList[0] ?? "";
        setSelected(selectedVersion.value, r.dir_path);
      } catch {
        // 所有目录均无版本：保持现状
        selectedVersion.value = list.includes(selectedVersion.value)
          ? selectedVersion.value
          : (list[0] ?? "");
      }
    }
  } catch {
    // ignore
  }
}

export async function doLaunch(navigateToDownload: () => void) {
  if (isLaunching.value) return;
  // 无可用游戏版本：按钮即"下载游戏"入口，跳转下载页
  if (!selectedVersion.value) {
    navigateToDownload();
    return;
  }

  const isOffline = authMode.value === "offline";

  // 离线模式必须输入用户名
  if (isOffline && !username.value.trim()) {
    showError("请输入用户名后再启动游戏");
    return;
  }

  // 正版模式必须已登录；启动前静默刷新 token
  // （界面切换在前，刷新是网络请求，避免点击后界面无反馈）
  if (!isOffline && !msAccount.value) {
    showError("请先登录 Microsoft 账号");
    return;
  }

  const name = isOffline ? username.value.trim() : (msAccount.value?.name ?? "");
  launchState.phase = "running";
  launchState.steps = [
    { name: "检查 Java 环境", status: "running" },
    { name: "检查游戏文件", status: "pending" },
    { name: "启动游戏", status: "pending" },
    { name: "等待游戏窗口", status: "pending" },
  ];

  try {
    // 正版：等待 token 刷新完成（失败时重置并返回，不重复弹窗）
    if (!isOffline) {
      try {
        msAccount.value = await invoke<MsAccountInfo>("refresh_ms_account");
      } catch (e) {
        msAccount.value = null;
        resetLaunch();
        showError(`正版登录已失效，请重新登录：${String(e)}`, true);
        return;
      }
    }

    // ① 环境 + 完整性校验
    await invoke("check_launch_env", { versionDirName: selectedVersion.value });
    step("检查 Java 环境", "done");
    step("检查游戏文件", "running");
    await new Promise(r => setTimeout(r, 100)); // 让用户看到完整性检查过程
    step("检查游戏文件", "done");

    // ② 启动游戏进程
    step("启动游戏", "running");
    const pid = await invoke("spawn_game", {
      versionDirName: selectedVersion.value,
      authMode: isOffline ? "offline" : "online",
      username: name,
      accessToken: isOffline ? "" : (msAccount.value?.access_token ?? ""),
      uuid: isOffline ? "" : (msAccount.value?.uuid ?? ""),
      xuid: isOffline ? "" : (msAccount.value?.xuid ?? ""),
    });
    step("启动游戏", "done");
    step("等待游戏窗口", "running");
    console.log("Game started, PID:", pid);

    // 记录最后一次离线用户名（与正版隔离）
    if (isOffline) {
      await invoke("save_last_offline_username", { username: name });
    }

    // ③ 等待游戏窗口出现（后端轮询可见窗口推送事件；90s 兜底防卡死）
    step("等待游戏窗口", "running");
    const windowShown = await waitGameWindow(selectedVersion.value);
    step("等待游戏窗口", "done");
    launchState.phase = "done";
    if (!windowShown) {
      showError(
        "未检测到游戏窗口（启动可能缓慢或进程已退出），请查看日志\n" +
        "版本：\n日志：.minecraft/logs/elytra-launch.log",
        true,
      );
    }

    // 完成后稍作停留再切回原样
    setTimeout(() => {
      resetLaunch();
    }, 1500);
  } catch (e) {
    resetLaunch();
    showError(String(e), true);
  }
}

let gameExitListenerReady = false;

/** 等待游戏主窗口：window-visible 事件到达（后端轮询命中）即成功；
 *  进程先退出则按窗口是否已出现过判定；90s 无事件兜底失败（防卡死）。 */
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
    listen<[number, string]>(EVT_GAME_WINDOW_VISIBLE, (ev) => {
      if (ev.payload[1] !== label) return;
      windowSeen = true;
      finish(true);
    }).then(u => unlisteners.push(u));
    listen<[number, string, number | null, string | null]>(EVT_GAME_EXITED, (ev) => {
      if (ev.payload[1] !== label) return;
      // 任意退出码：窗口已见则视为"已成功等待"，否则判定失败
      finish(windowSeen);
    }).then(u => unlisteners.push(u));
  });
}

/** 幂等注册 game-exited 监听（页面 onMounted 调用一次）：非零退出码弹错误提示。 */
export async function ensureGameExitListener() {
  if (gameExitListenerReady) return;
  gameExitListenerReady = true;
  await listen<[number, string, number | null, string | null]>(EVT_GAME_EXITED, (ev) => {
    console.log("Game exited:", ev.payload);
    const [, label, exitCode, crashReport] = ev.payload;
    // 退出码非 0：游戏异常退出（崩溃/启动失败）——不再静默，弹错误级提示
    // body 仅友好提示（AlertDialog 只显示首行），完整信息进复制区
    if (exitCode !== null && exitCode !== 0) {
      showError(
        `游戏异常退出（退出码 ${exitCode}），请查看崩溃报告或重新启动\n` +
        `版本：${label}\n退出码：${exitCode}\n日志：.minecraft/logs/elytra-launch.log` +
        (crashReport ? `\n崩溃报告：${crashReport}` : ""),
        true,
      );
    }
  });
}
