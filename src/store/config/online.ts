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
  /** guest 与 host 的连接路径：direct / relay；null = 未加入或尚未探测。 */
  path: "direct" | "relay" | null;
  latency_ms: number | null;
  /** 当前会话是否允许中继。 */
  allow_relay: boolean;
}

/** 节点检查结果（镜像 Rust NodeCheckResult）。 */
export interface NodeCheckResult {
  node: string;
  status: "ok" | "unreachable" | "invalid" | "unchecked" | "not_node";
  latency_ms: number | null;
  detail: string | null;
}

export const onlineStatus = ref<OnlineStatus>("idle");
export const onlineRoomCode = ref("");
export const onlineLocalPort = ref<number | null>(null);
export const onlinePlayers = ref<PlayerProfile[]>([]);
export const onlinePath = ref<"direct" | "relay" | null>(null);
export const onlineLatency = ref<number | null>(null);
export const onlineNickname = ref("");
export const onlineErrorHost = ref("");
export const onlineErrorGuest = ref("");
/** 协调节点列表（单行输入 + 添加，内置节点也可删除）。 */
export const onlinePublicNodes = ref<string[]>([]);
export const onlineAdvertiseMotd = ref("");
/** 是否允许打洞失败时经公共节点中继（默认 false）。 */
export const onlineAllowRelay = ref(false);

function applySnapshot(snap: OnlineSnapshot) {
  onlineStatus.value = snap.status;
  onlineRoomCode.value = snap.room_code ?? "";
  onlineLocalPort.value = snap.local_port ?? null;
  onlinePlayers.value = snap.players ?? [];
  onlinePath.value = snap.path ?? null;
  onlineLatency.value = snap.latency_ms ?? null;
  onlineAllowRelay.value = snap.allow_relay ?? onlineAllowRelay.value;
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
    onlinePublicNodes.value = [...(cfg.public_nodes ?? [])];
    onlineAdvertiseMotd.value = cfg.advertise_motd ?? "";
    onlineNickname.value = cfg.nickname ?? "";
    onlineAllowRelay.value = cfg.allow_relay ?? false;
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
    .map((s) => s.trim())
    .filter((s) => s.length > 0);
  const motd = onlineAdvertiseMotd.value.trim();
  const nick = onlineNickname.value.trim();
  return {
    public_nodes: nodes,
    advertise_motd: motd.length > 0 ? motd : null,
    nickname: nick.length > 0 ? nick : null,
    allow_relay: onlineAllowRelay.value,
  };
}

/** 检查节点（格式 + TCP 可达性）。 */
export function checkOnlineNodes(nodes: string[]): Promise<NodeCheckResult[]> {
  return invoke<NodeCheckResult[]>("check_online_nodes", { nodes });
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
    applySnapshot({
      status: "idle",
      room_code: null,
      local_port: null,
      players: [],
      path: null,
      latency_ms: null,
      allow_relay: onlineAllowRelay.value,
    });
  } catch (e) {
    // 停止失败对 host 区展示（会话归属不确定时归 host）
    onlineErrorHost.value = errMsg(e);
  }
}
