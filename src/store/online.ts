// 联机 store：EasyTier 虚拟局域网（host 开房间 / guest 加入）。
// 调后端命令（commands/online.rs），监听 online-state 事件同步状态。
// 顺序自由：host 可先开联机（waiting）再在游戏里"对局域网开放"，
// 后端检测到端口后事件推送 hosting + 完整房间码；guest 从房间码解析端口。
import { ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { EVT_ONLINE_STATE } from "./events";

/** 联机状态（镜像 Rust OnlineStatus）。 */
export type OnlineStatus = "idle" | "waiting" | "hosting" | "joining";

/** 状态快照（镜像 Rust OnlineSnapshot）。 */
export interface OnlineSnapshot {
  status: OnlineStatus;
  room_code: string | null;
  local_port: number | null;
}

export const onlineStatus = ref<OnlineStatus>("idle");
/** 当前房间码（host：waiting 时为身份段，hosting 时含端口可分享）。 */
export const onlineRoomCode = ref("");
/** guest 加入后的本地转发端口（游戏内填 127.0.0.1:此端口）。 */
export const onlineLocalPort = ref<number | null>(null);
/** 操作错误（内联展示）。 */
export const onlineError = ref("");

function applySnapshot(snap: OnlineSnapshot) {
  onlineStatus.value = snap.status;
  onlineRoomCode.value = snap.room_code ?? "";
  onlineLocalPort.value = snap.local_port ?? null;
}

let listening = false;

/** 幂等注册 online-state 事件监听（对齐 download.ts ensureListen 模式）。 */
export function ensureOnlineListen() {
  if (listening) return;
  listening = true;
  listen<OnlineSnapshot>(EVT_ONLINE_STATE, (ev) => {
    applySnapshot(ev.payload);
  });
}

/** 页面加载时拉取当前状态（避免仅靠事件）。 */
export async function loadOnlineStatus() {
  try {
    applySnapshot(await invoke<OnlineSnapshot>("online_status"));
  } catch {
    // 查询失败保持默认
  }
}

/** 开启联机（host）：立即建网进入 waiting；检测到游戏端口后自动转 hosting。 */
export async function startHost() {
  onlineError.value = "";
  try {
    applySnapshot(await invoke<OnlineSnapshot>("online_start_host"));
  } catch (e) {
    onlineError.value = String(e);
    throw e;
  }
}

/** 加入联机（guest）：解析房间码自动拿端口，返回本地转发端口。 */
export async function joinRoom(roomCode: string) {
  onlineError.value = "";
  try {
    const port = await invoke<number>("online_join", { roomCode });
    onlineLocalPort.value = port;
    onlineStatus.value = "joining";
    return port;
  } catch (e) {
    onlineError.value = String(e);
    throw e;
  }
}

/** 停止联机。 */
export async function stopOnline() {
  onlineError.value = "";
  try {
    await invoke("online_stop");
    applySnapshot({ status: "idle", room_code: null, local_port: null });
  } catch (e) {
    onlineError.value = String(e);
  }
}
