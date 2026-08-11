// 头像渲染：正版（visage 服务端 3D 头部）+ 离线（五种皮肤类型 2D 头部 / 像素脸回退）。
// 模块级单例（替代原 composable）：状态在 avatarState，渲染函数直接导出，store/组件均可调用。
import { reactive } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { hashName, randomIsAlex } from "../utils/offlineSkin";
import { showError } from "./alert";

export const avatarState = reactive({
  premiumAvatar: "",
  offlineAvatar: "",
  avatarError: false,
});

// 模块级去重缓存：账号/用户名未变不重复请求（路由切换重挂载不重复请求、不闪占位）
let lastPremiumUuid = "";
let lastOfflineName = "";
let lastOfflineKey = "";
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

/** 离线头像：按「离线皮肤」设置渲染（对齐 PCL2 五种），失败回退像素脸。
 * 全部类型均为 2D 头像（原始纹理截取头部，无帽子层）：
 * - random：用户名哈希确定性选 Steve/Alex 默认皮肤
 * - steve / alex：固定对应默认皮肤
 * - player：指定正版玩家的 sessionserver 纹理
 * - custom：本地皮肤文件 */
export async function renderOfflineAvatar(name: string) {
  const trimmed = name.trim();
  if (!trimmed) {
    avatarState.offlineAvatar = "";
    return;
  }
  try {
    const cfg = await invoke<{
      skin_type: string;
      player_name: string;
      custom_path: string;
    }>("get_offline_skin");
    // 用户名与离线皮肤设置均未变 → 不重复请求（含进行中；设置变更时 key 变化自动重新渲染）
    const key = `${cfg.skin_type}|${cfg.player_name}|${cfg.custom_path}`;
    if (lastOfflineName === trimmed && lastOfflineKey === key) {
      return;
    }
    lastOfflineName = trimmed;
    lastOfflineKey = key;
    if (cfg.skin_type === "player") {
      const p = cfg.player_name.trim();
      if (!p) {
        generateOfflineAvatar(trimmed);
        return;
      }
      // 2D 头像：sessionserver 取正版玩家纹理 → 截取头部
      const b64 = await invoke<string>("fetch_player_skin_texture", { playerName: p });
      avatarState.offlineAvatar = await renderSkinHead(b64);
      return;
    }
    if (cfg.skin_type === "custom") {
      if (!cfg.custom_path) {
        generateOfflineAvatar(trimmed);
        return;
      }
      await renderCustomSkinFile(cfg.custom_path, trimmed);
      return;
    }
    // random：哈希确定性选默认皮肤；steve/alex：固定对应默认皮肤
    // 一律 2D 头像：游戏内置纹理（经典蓝衬衫 Steve / 绿衬衫 Alex）截取头部
    const skinKind =
      cfg.skin_type === "alex" ||
      (cfg.skin_type === "random" && randomIsAlex(trimmed))
        ? "alex"
        : "steve";
    const b64 = await invoke<string>("fetch_default_skin", { skin: skinKind });
    avatarState.offlineAvatar = await renderSkinHead(b64);
  } catch {
    generateOfflineAvatar(trimmed);
  }
}

/** 截取皮肤正面头部（8×8 格）放大成 2D 大头像（无帽子层）。 */
async function renderSkinHead(b64: string): Promise<string> {
  const img = await loadImage(`data:image/png;base64,${b64}`);
  const grid = Math.floor(img.width / 8); // 64 → 8，128 → 16
  const canvas = document.createElement("canvas");
  canvas.width = 68;
  canvas.height = 68;
  const ctx = canvas.getContext("2d")!;
  ctx.imageSmoothingEnabled = false;
  ctx.drawImage(img, grid, grid, grid, grid, 0, 0, 68, 68);
  return canvas.toDataURL("image/png");
}

/** 自定义本地皮肤：截取正面区域（64/128 布局均为 (grid, grid)）放大成 2D 大头像。 */
async function renderCustomSkinFile(path: string, fallbackName: string) {
  try {
    const b64 = await invoke<string>("read_skin_file", { path });
    avatarState.offlineAvatar = await renderSkinHead(b64);
  } catch (e) {
    console.error("自定义皮肤渲染失败:", e);
    generateOfflineAvatar(fallbackName);
  }
}

/** 按用户名生成 Minecraft 风格 8×8 像素脸（确定性，左右镜像）——离线用户名非真实账号时的回退。 */
function generateOfflineAvatar(name: string) {
  const trimmed = name.trim();
  if (!trimmed) {
    avatarState.offlineAvatar = "";
    return;
  }
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
  avatarState.offlineAvatar = out.toDataURL("image/png");
}

/** 用户名输入防抖 1.337s 后刷新离线头像。 */
export function scheduleOfflineRender(name: string) {
  avatarState.avatarError = false;
  if (avatarTimer) clearTimeout(avatarTimer);
  avatarTimer = window.setTimeout(() => renderOfflineAvatar(name), 1337);
}
