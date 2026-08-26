// 联机 store：EasyTier 虚拟局域网（host 开房间 / guest 加入）。
// 调后端命令（commands/online.rs），监听 online-state 事件同步状态。
// 顺序自由：host 可先开联机（waiting）再在游戏里"对局域网开放"，
// 后端检测到端口后事件推送 hosting + 完整房间码；guest 从房间码解析端口。
// 同网直连：host 局域网 IP 编入房间码（@IP 段），guest 解析后直连 host；
// 跨网联机需在设置里配置协调服务器（easytier 官方公共节点已下线）。
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

/** 联机配置（镜像 Rust OnlineConfig）。 */
export interface OnlineConfig {
  public_nodes: string[];
}

export const onlineStatus = ref<OnlineStatus>("idle");
/** 当前房间码（host：waiting 时为入口端口身份段，hosting 时含端口可分享）。 */
export const onlineRoomCode = ref("");
/** guest 加入后的本地转发端口（游戏内局域网标签页点 "Elytra 联机" 进入，此端口为参考）。 */
export const onlineLocalPort = ref<number | null>(null);
/** host 区操作错误（内联展示在"作为主机"卡片）。 */
export const onlineErrorHost = ref("");
/** guest 区操作错误（内联展示在"加入联机"卡片）。 */
export const onlineErrorGuest = ref("");
/** 协调服务器列表（每行一个 easytier peer URI，如 tcp://1.2.3.4:11010）。 */
export const onlinePublicNodes = ref("");

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

/** 加载协调服务器配置。 */
export async function loadOnlineConfig() {
  try {
    const cfg = await invoke<OnlineConfig>("get_online_config");
    onlinePublicNodes.value = (cfg.public_nodes ?? []).join("\n");
  } catch {
    // 查询失败保持默认
  }
}

/** 保存协调服务器配置（空 = 仅同网直连）。 */
export async function saveOnlineConfig() {
  const nodes = onlinePublicNodes.value
    .split("\n")
    .map((s) => s.trim())
    .filter((s) => s.length > 0);
  await invoke("save_online_config", { config: { public_nodes: nodes } });
}

/** 开启联机（host）：立即建网进入 waiting；检测到游戏端口后自动转 hosting。 */
export async function startHost() {
  onlineErrorHost.value = "";
  try {
    applySnapshot(await invoke<OnlineSnapshot>("online_start_host"));
  } catch (e) {
    onlineErrorHost.value = String(e);
    throw e;
  }
}

/** 加入联机（guest）：解析房间码自动拿端口，返回本地转发端口。 */
export async function joinRoom(roomCode: string) {
  onlineErrorGuest.value = "";
  try {
    const port = await invoke<number>("online_join", { roomCode });
    onlineLocalPort.value = port;
    onlineStatus.value = "joining";
    return port;
  } catch (e) {
    onlineErrorGuest.value = String(e);
    throw e;
  }
}

/** 停止联机。 */
export async function stopOnline() {
  onlineErrorHost.value = "";
  onlineErrorGuest.value = "";
  try {
    await invoke("online_stop");
    applySnapshot({ status: "idle", room_code: null, local_port: null });
  } catch (e) {
    // 停止失败对 host 区展示（会话归属不确定时归 host）
    onlineErrorHost.value = String(e);
  }
}
