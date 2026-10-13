// 联机 store：EasyTier 虚拟局域网（host 开房 / guest 加入），调 commands/online.rs 并监听 online-state；host 可先开联机（waiting），游戏内开放局域网后后端检测端口再推 hosting + 房间码；跨网需配协调服务器。
import { ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { ONLINE_STATE } from "../app/events";
import type { OnlineConfig } from "../../types/config/OnlineConfig";
import { errMsg } from "../../utils/error";

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

export const onlineStatus = ref<OnlineStatus>("idle");
export const onlineRoomCode = ref("");
export const onlineLocalPort = ref<number | null>(null);
export const onlinePlayers = ref<PlayerProfile[]>([]);
export const onlineNickname = ref("");
export const onlineErrorHost = ref("");
export const onlineErrorGuest = ref("");
export const onlinePublicNodes = ref("");
export const onlineAdvertiseMotd = ref("");

function applySnapshot(snap: OnlineSnapshot) {
  onlineStatus.value = snap.status;
  onlineRoomCode.value = snap.room_code ?? "";
  onlineLocalPort.value = snap.local_port ?? null;
  onlinePlayers.value = snap.players ?? [];
}

let listening = false;

export function ensureOnlineListen() {
  if (listening) return;
  listening = true;
  listen<OnlineSnapshot>(ONLINE_STATE, (ev) => {
    applySnapshot(ev.payload);
  });
}

export async function loadOnlineStatus() {
  try {
    applySnapshot(await invoke<OnlineSnapshot>("online_status"));
  } catch {
  }
}

export async function loadOnlineConfig() {
  try {
    const cfg = await invoke<OnlineConfig>("get_online_config");
    onlinePublicNodes.value = (cfg.public_nodes ?? []).join("\n");
    onlineAdvertiseMotd.value = cfg.advertise_motd ?? "";
    onlineNickname.value = cfg.nickname ?? "";
  } catch {
  }
}

export async function saveOnlineConfig() {
  // 提交完整配置：两个保存入口（协调服务器 / 广播名）互不覆盖对方字段
  await invoke("save_online_config", { config: buildOnlineConfig() });
}

export async function saveAdvertiseMotd() {
  await invoke("save_online_config", { config: buildOnlineConfig() });
}

export async function saveNickname() {
  await invoke("save_online_config", { config: buildOnlineConfig() });
}

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

export async function startHost() {
  onlineErrorHost.value = "";
  try {
    applySnapshot(await invoke<OnlineSnapshot>("online_start_host"));
  } catch (e) {
    onlineErrorHost.value = errMsg(e);
    throw e;
  }
}

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
