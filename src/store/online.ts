// 联机 store：EasyTier 虚拟局域网（host 开房间 / guest 加入）。
// 调后端命令（commands/online.rs），监听 online-state 事件同步状态。
// 顺序自由：host 可先开联机（waiting）再在游戏里"对局域网开放"，
// 后端检测到端口后事件推送 hosting + 完整房间码；guest 从房间码解析端口。
// Scaffolding 协议：host 开房间（房间码 U/XXXX-XXXX-XXXX-XXXX），guest 加入；
// 跨网联机需在设置里配置协调服务器。
import { ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { ONLINE_STATE } from "./events";
import { errMsg } from "../utils/error";

/** 联机状态（镜像 Rust OnlineStatus）。 */
export type OnlineStatus = "idle" | "waiting" | "hosting" | "joining";

/** 玩家类型（镜像 Rust ProfileKind）。 */
export type ProfileKind = "HOST" | "GUEST" | "LOCAL";

/** 玩家档案（镜像 Rust Profile）。 */
export interface PlayerProfile {
  name: string;
  machine_id: string;
  vendor: string;
  kind: ProfileKind;
}

/** 状态快照（镜像 Rust OnlineSnapshot）。 */
export interface OnlineSnapshot {
  status: OnlineStatus;
  room_code: string | null;
  local_port: number | null;
  /** 玩家列表（HOST + GUEST；协议 c:player_profiles_list）。 */
  players: PlayerProfile[];
}

/** 联机配置（镜像 Rust OnlineConfig）。 */
export interface OnlineConfig {
  public_nodes: string[];
  /** 自定义联机广播名（guest 端局域网标签页显示名，支持 § 格式码；null = 自动）。 */
  advertise_motd: string | null;
  /** 联机昵称（协议 c:player_ping 的 name；null = 系统用户名）。 */
  nickname: string | null;
}

export const onlineStatus = ref<OnlineStatus>("idle");
/** 当前房间码（host：开启联机即生成，可分享给朋友加入）。 */
export const onlineRoomCode = ref("");
/** guest 加入后的本地转发端口（游戏内局域网标签页点 "Elytra 联机" 进入，此端口为参考）。 */
export const onlineLocalPort = ref<number | null>(null);
/** 玩家列表（HOST + GUEST；来自 online-state 快照）。 */
export const onlinePlayers = ref<PlayerProfile[]>([]);
/** 联机昵称（设置里配置；空 = 系统用户名）。 */
export const onlineNickname = ref("");
/** host 区操作错误（内联展示在"作为主机"卡片）。 */
export const onlineErrorHost = ref("");
/** guest 区操作错误（内联展示在"加入联机"卡片）。 */
export const onlineErrorGuest = ref("");
/** 协调服务器列表（每行一个 easytier peer URI，如 tcp://1.2.3.4:11010）。 */
export const onlinePublicNodes = ref("");
/** 自定义联机广播名（空 = 自动：用 host 世界名）。 */
export const onlineAdvertiseMotd = ref("");

function applySnapshot(snap: OnlineSnapshot) {
  onlineStatus.value = snap.status;
  onlineRoomCode.value = snap.room_code ?? "";
  onlineLocalPort.value = snap.local_port ?? null;
  onlinePlayers.value = snap.players ?? [];
}

let listening = false;

/** 幂等注册 online-state 事件监听（对齐 download.ts ensureListen 模式）。 */
export function ensureOnlineListen() {
  if (listening) return;
  listening = true;
  listen<OnlineSnapshot>(ONLINE_STATE, (ev) => {
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
    onlineAdvertiseMotd.value = cfg.advertise_motd ?? "";
    onlineNickname.value = cfg.nickname ?? "";
  } catch {
    // 查询失败保持默认
  }
}

/** 保存协调服务器配置（空 = 仅同网直连）。 */
export async function saveOnlineConfig() {
  // 提交完整配置：两个保存入口（协调服务器 / 广播名）互不覆盖对方字段
  await invoke("save_online_config", { config: buildOnlineConfig() });
}

/** 保存自定义联机广播名（空 = 恢复自动）。 */
export async function saveAdvertiseMotd() {
  await invoke("save_online_config", { config: buildOnlineConfig() });
}

/** 保存联机昵称（空 = 系统用户名）。 */
export async function saveNickname() {
  await invoke("save_online_config", { config: buildOnlineConfig() });
}

/** 组装完整 OnlineConfig（public_nodes + advertise_motd + nickname）。 */
function buildOnlineConfig(): OnlineConfig {
  const nodes = onlinePublicNodes.value
    .split("\n")
    .map((s) => s.trim())
    .filter((s) => s.length > 0);
  const motd = onlineAdvertiseMotd.value.trim();
  const nick = onlineNickname.value.trim();
  return {
    public_nodes: nodes,
    advertise_motd: motd.length > 0 ? motd : null,
    nickname: nick.length > 0 ? nick : null,
  };
}

/** 开启联机（host）：立即建网进入 waiting；检测到游戏端口后自动转 hosting。 */
export async function startHost() {
  onlineErrorHost.value = "";
  try {
    applySnapshot(await invoke<OnlineSnapshot>("online_start_host"));
  } catch (e) {
    onlineErrorHost.value = errMsg(e);
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
    onlineErrorGuest.value = errMsg(e);
    throw e;
  }
}

/** 停止联机。 */
export async function stopOnline() {
  onlineErrorHost.value = "";
  onlineErrorGuest.value = "";
  try {
    await invoke("online_stop");
    applySnapshot({ status: "idle", room_code: null, local_port: null, players: [] });
  } catch (e) {
    // 停止失败对 host 区展示（会话归属不确定时归 host）
    onlineErrorHost.value = errMsg(e);
  }
}
