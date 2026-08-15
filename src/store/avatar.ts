// 头像渲染：正版（visage 服务端 3D 头部）+ 离线（哈希决定 Steve/Alex 默认皮肤 2D 头部 / 像素脸回退）。
// 模块级单例（替代原 composable）：状态在 avatarState，渲染函数直接导出，store/组件均可调用。
import { reactive } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { showError } from "./alert";

/** FNV-1a 哈希（与 Rust 侧 config/launcher/skin.rs hash_name 一致）。 */
function hashName(name: string): number {
  let h = 2166136261;
  for (let i = 0; i < name.length; i++) {
    h ^= name.charCodeAt(i);
    h = Math.imul(h, 16777619);
  }
  return h >>> 0;
}

/** 离线皮肤模型决策：哈希奇数 → Alex（slim）。 */
function randomIsAlex(name: string): boolean {
  return hashName(name) % 2 === 1;
}

export const avatarState = reactive({
  premiumAvatar: "",
  offlineAvatar: "",
  avatarError: false,
});

// 模块级去重缓存：账号/用户名未变不重复请求（路由切换重挂载不重复请求、不闪占位）
let lastPremiumUuid = "";
let lastOfflineName = "";
let avatarTimer: number | undefined;

/** 正版：后端经 visage 3D 渲染服务获取头部图（浏览器无法设置自定义 UA，走后端）。
 * 优先按 Mojang texture ID 渲染——皮肤更换后 ID 即变，visage 按 ID 缓存不会串图；
 * 失败（如 profile 拉取异常）回退 uuid 路径。force 用于主动刷新（上传皮肤后等）。 */
export async function renderPremiumAvatar(uuid: string, force = false) {
  if (!uuid) {
    avatarState.premiumAvatar = "";
    lastPremiumUuid = "";
    return;
  }
  // 账号未变 → 不重复请求（含进行中，避免并发竞态；失败时重置可重试）
  if (!force && lastPremiumUuid === uuid) {
    return;
  }
  lastPremiumUuid = uuid;
  try {
    let b64: string;
    try {
      const tid = await invoke<string>("fetch_texture_id", { uuid });
      b64 = await invoke<string>("fetch_avatar", { subject: tid });
    } catch {
      // 回退：直接按 uuid 渲染
      b64 = await invoke<string>("fetch_avatar", { subject: uuid });
    }
    avatarState.premiumAvatar = `data:image/png;base64,${b64}`;
    avatarState.avatarError = false;
  } catch (e) {
    console.error("正版头像加载失败:", e);
    avatarState.premiumAvatar = "";
    lastPremiumUuid = ""; // 失败允许下次重试
    avatarState.avatarError = true;
    showError(`正版头像加载失败：${String(e)}`, true);
  }
}

/** 确定性 PRNG（mulberry32） */
function mulberry32(seed: number) {
  return function () {
    seed |= 0;
    seed = (seed + 0x6d2b79f5) | 0;
    let t = Math.imul(seed ^ (seed >>> 15), 1 | seed);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

function loadImage(src: string): Promise<HTMLImageElement> {
  return new Promise((resolve, reject) => {
    const img = new Image();
    img.onload = () => resolve(img);
    img.onerror = () => reject(new Error("image load failed"));
    img.src = src;
  });
}

/** 离线头像：按用户名哈希确定性渲染 Steve/Alex 默认皮肤（与游戏内一致），失败回退像素脸。
 * 一律 2D 头像（游戏内置纹理截取头部，无帽子层）。 */
export async function renderOfflineAvatar(name: string) {
  const trimmed = name.trim();
  if (!trimmed) {
    avatarState.offlineAvatar = "";
    return;
  }
  if (lastOfflineName === trimmed) {
    return; // 用户名未变 → 不重复请求（含进行中）
  }
  lastOfflineName = trimmed;
  const head = await renderOfflineHead(trimmed);
  if (head) {
    avatarState.offlineAvatar = head;
  } else {
    lastOfflineName = ""; // 失败允许下次重试（如版本 jar 就绪后）
    avatarState.offlineAvatar = generatePixelFace(trimmed);
  }
}

// ── 离线头像缓存（用户名 → Steve/Alex 2D 头部 dataURL；切换回已有用户名秒出，不重复 IO）──

const OFFLINE_HEAD_CACHE = new Map<string, string>();

/** 渲染离线用户名的 Steve/Alex 默认皮肤 2D 头部（哈希决定模型，与主头像/游戏内一致）。
 * 缓存按用户名复用（仅缓存成功结果，失败不缓存以便之后重试）；失败返回 ""（调用方回退）。 */
export async function renderOfflineHead(name: string): Promise<string> {
  const trimmed = name.trim();
  if (!trimmed) return "";
  const cached = OFFLINE_HEAD_CACHE.get(trimmed);
  if (cached !== undefined) return cached;
  try {
    const skinKind = randomIsAlex(trimmed) ? "alex" : "steve";
    const b64 = await invoke<string>("fetch_default_skin", { skin: skinKind });
    const head = await renderSkinHead(b64);
    OFFLINE_HEAD_CACHE.set(trimmed, head);
    return head;
  } catch {
    return "";
  }
}

/** 截取皮肤正面头部（8×8 格）放大成 2D 大头像（无帽子层）。 */
export async function renderSkinHead(b64: string): Promise<string> {  const img = await loadImage(`data:image/png;base64,${b64}`);
  const grid = Math.floor(img.width / 8); // 64 → 8，128 → 16
  const canvas = document.createElement("canvas");
  canvas.width = 68;
  canvas.height = 68;
  const ctx = canvas.getContext("2d")!;
  ctx.imageSmoothingEnabled = false;
  ctx.drawImage(img, grid, grid, grid, grid, 0, 0, 68, 68);
  return canvas.toDataURL("image/png");
}

/** 按用户名生成 Minecraft 风格 8×8 像素脸（确定性，左右镜像）——离线用户名非真实账号时的回退。
 * 独立纯函数：离线主头像兜底与历史列表行头像共用（同用户名同脸）。返回 dataURL。 */
export function generatePixelFace(name: string): string {
  const trimmed = name.trim();
  const seed = hashName(trimmed);
  const rng = mulberry32(seed);
  const skinTones = ["#c68642", "#b5763a", "#a0662f", "#d1a579", "#e0ac69", "#8d5524", "#e8b57e"];
  const hairColors = ["#2a2222", "#3b2f2f", "#4a3728", "#1f1f1f", "#5c4033", "#6b4a2a"];

  const base = skinTones[seed % skinTones.length];
  const hair = hairColors[(seed >> 3) % hairColors.length];
  const eye = "#2f2f2f";

  // 8×8 像素脸
  const canvas = document.createElement("canvas");
  canvas.width = 8;
  canvas.height = 8;
  const ctx = canvas.getContext("2d")!;

  // 皮肤底色
  ctx.fillStyle = base;
  ctx.fillRect(0, 0, 8, 8);
  // 头发：顶部两行，左半随机 + 镜像
  ctx.fillStyle = hair;
  for (let x = 0; x < 4; x++) {
    for (let y = 0; y < 2; y++) {
      if (rng() > 0.35) {
        ctx.fillRect(x, y, 1, 1);
        ctx.fillRect(7 - x, y, 1, 1);
      }
    }
  }
  // 眼睛
  ctx.fillStyle = eye;
  ctx.fillRect(2, 3, 1, 1);
  ctx.fillRect(5, 3, 1, 1);
  // 嘴
  ctx.fillRect(3, 6, 1, 1);
  ctx.fillRect(4, 6, 1, 1);

  // 放大到 68×68
  const out = document.createElement("canvas");
  out.width = 68;
  out.height = 68;
  const octx = out.getContext("2d")!;
  octx.imageSmoothingEnabled = false;
  octx.drawImage(canvas, 0, 0, 68, 68);
  return out.toDataURL("image/png");
}

/** 用户名输入防抖 1.337s 后刷新离线头像。 */
export function scheduleOfflineRender(name: string) {
  avatarState.avatarError = false;
  if (avatarTimer) clearTimeout(avatarTimer);
  avatarTimer = window.setTimeout(() => renderOfflineAvatar(name), 1337);
}

// ── 账号列表 2D 头像（sessionserver 皮肤截取头部，按 uuid 会话内缓存）──

const ACCOUNT_HEAD_CACHE = new Map<string, string>();

/** 拉取正版账号皮肤并截取 2D 头部，返回 dataURL；失败（无网络/未设置皮肤）返回 ""（调用方回退图标）。 */
export async function renderAccountHead(uuid: string): Promise<string> {
  const cached = ACCOUNT_HEAD_CACHE.get(uuid);
  if (cached !== undefined) return cached;
  try {
    const b64 = await invoke<string>("fetch_account_skin", { uuid });
    const head = await renderSkinHead(b64);
    ACCOUNT_HEAD_CACHE.set(uuid, head);
    return head;
  } catch {
    ACCOUNT_HEAD_CACHE.set(uuid, "");
    return "";
  }
}
