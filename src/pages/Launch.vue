<script lang="ts">
// 模块级头像缓存：跨组件实例存活（路由切换卸载/重挂载不重复请求、不闪占位）
let lastPremiumUuid = "";
let lastOfflineName = "";
let lastOfflineKey = "";
let cachedPremiumAvatar = "";
let cachedOfflineAvatar = "";
</script>

<script setup lang="ts">
import { ref, computed, watch, onMounted, onUnmounted } from "vue";
import { useRouter } from "vue-router";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { openUrl } from "@tauri-apps/plugin-opener";
import { open, save } from "@tauri-apps/plugin-dialog";
import AlertDialog from "../component/AlertDialog.vue";

type AuthMode = "premium" | "offline";

type MsAccountInfo = {
  uuid: string;
  name: string;
  access_token: string;
  refresh_token: string;
  xuid: string;
  expires_at: number;
};

// 记住正版/离线选择（localStorage，跨启动与页面切换）
const AUTH_MODE_KEY = "elytra.authMode";
const authMode = ref<AuthMode>(localStorage.getItem(AUTH_MODE_KEY) === "offline" ? "offline" : "premium");
const msAccount = ref<MsAccountInfo | null>(null);
const msLoginState = ref<"idle" | "waiting">("idle");
const username = ref("");
const versions = ref<string[]>([]);
// 记住上次选择的版本（版本选择页写入）
const SELECTED_VERSION_KEY = "elytra.selectedVersion";
const selectedVersion = ref(localStorage.getItem(SELECTED_VERSION_KEY) ?? "");
const launching = ref(false);
const router = useRouter();
const alertMsg = ref("");
const showAlert = ref(false);
/** 弹窗是否为复杂错误（需要复制按钮） */
const alertCopyable = ref(false);

// ── 轻提示（toast：短暂显示，自动消失，无需手动关闭） ────────────
const toastMsg = ref("");
let toastTimer: number | undefined;
function showToast(msg: string) {
  toastMsg.value = msg;
  clearTimeout(toastTimer);
  toastTimer = window.setTimeout(() => (toastMsg.value = ""), 2500);
}

// ── 头像 ─────────────────────────────────────────────────────────
/** 正版：visage 服务端 3D 渲染头部（正面视角+光照）；离线：按用户名生成的 2D 像素脸。 */
const premiumAvatar = ref(cachedPremiumAvatar);
const offlineAvatar = ref(cachedOfflineAvatar);
const avatarError = ref(false);

const avatarUrl = computed(() =>
  authMode.value === "premium" ? premiumAvatar.value : offlineAvatar.value,
);

/** 正版：后端经 visage 3D 渲染服务获取头部图（浏览器无法设置自定义 UA，走后端）。
 * 优先按 Mojang texture ID 渲染——皮肤更换后 ID 即变，visage 按 ID 缓存不会串图；
 * 失败（如 profile 拉取异常）回退 uuid 路径。force 用于主动刷新（上传皮肤后等）。 */
async function renderPremiumAvatar(uuid: string, force = false) {
  if (!uuid) {
    premiumAvatar.value = "";
    cachedPremiumAvatar = "";
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
    premiumAvatar.value = `data:image/png;base64,${b64}`;
    cachedPremiumAvatar = premiumAvatar.value;
    avatarError.value = false;
  } catch (e) {
    console.error("正版头像加载失败:", e);
    premiumAvatar.value = "";
    cachedPremiumAvatar = "";
    lastPremiumUuid = ""; // 失败允许下次重试
    avatarError.value = true;
    alertMsg.value = `正版头像加载失败：${String(e)}`;
    alertCopyable.value = true;
    showAlert.value = true;
  }
}

/** FNV-1a 哈希 → 32 位种子 */
function hashName(name: string): number {
  let h = 2166136261;
  for (let i = 0; i < name.length; i++) {
    h ^= name.charCodeAt(i);
    h = Math.imul(h, 16777619);
  }
  return h >>> 0;
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
async function renderOfflineAvatar(name: string) {
  const trimmed = name.trim();
  if (!trimmed) {
    offlineAvatar.value = "";
    cachedOfflineAvatar = "";
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
      offlineAvatar.value = await renderSkinHead(b64);
      cachedOfflineAvatar = offlineAvatar.value;
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
      (cfg.skin_type === "random" && hashName(trimmed) % 2 === 1)
        ? "alex"
        : "steve";
    const b64 = await invoke<string>("fetch_default_skin", { skin: skinKind });
    offlineAvatar.value = await renderSkinHead(b64);
    cachedOfflineAvatar = offlineAvatar.value;
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
    offlineAvatar.value = await renderSkinHead(b64);
    cachedOfflineAvatar = offlineAvatar.value;
  } catch (e) {
    console.error("自定义皮肤渲染失败:", e);
    generateOfflineAvatar(fallbackName);
  }
}

/** 按用户名生成 Minecraft 风格 8×8 像素脸（确定性，左右镜像）——离线用户名非真实账号时的回退。 */
function generateOfflineAvatar(name: string) {
  const trimmed = name.trim();
  if (!trimmed) {
    offlineAvatar.value = "";
    cachedOfflineAvatar = "";
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
  offlineAvatar.value = out.toDataURL("image/png");
  cachedOfflineAvatar = offlineAvatar.value;
}

/** 用户名输入防抖 1.337s 后刷新离线头像 */
let avatarTimer: number | undefined;
watch(username, () => {
  avatarError.value = false;
  if (avatarTimer) clearTimeout(avatarTimer);
  avatarTimer = window.setTimeout(() => renderOfflineAvatar(username.value), 1337);
});

// 启动流程："" 空闲 | "running" 启动中 | "done" 完成
const launchPhase = ref("");
const launchSteps = ref<{ name: string; status: "pending" | "running" | "done" }[]>([]);

function switchMode(mode: AuthMode) {
  authMode.value = mode;
  localStorage.setItem(AUTH_MODE_KEY, mode);
  // 切回离线时若用户名为空，填充最后一次离线用户名（与正版隔离）
  if (mode === "offline" && !username.value) {
    loadLastUsername();
  }
  // 离线设置可能已变更，切换时重新渲染
  if (mode === "offline") {
    renderOfflineAvatar(username.value);
  }
}

async function loadLastUsername() {
  try {
    const last = await invoke<string>("get_last_offline_username");
    if (last && !username.value) username.value = last;
  } catch {
    // ignore
  }
}

function step(name: string, status: "pending" | "running" | "done") {
  const idx = launchSteps.value.findIndex(s => s.name === name);
  if (idx >= 0) launchSteps.value[idx].status = status;
}

function resetLaunch() {
  launchPhase.value = "";
  launchSteps.value = [];
}

async function doLaunch() {
  if (launching.value) return;
  if (!selectedVersion.value) return;

  const isOffline = authMode.value === "offline";

  // 离线模式必须输入用户名
  if (isOffline && !username.value.trim()) {
    alertMsg.value = "请输入用户名后再启动游戏";
    alertCopyable.value = false;
    showAlert.value = true;
    return;
  }

  // 正版模式必须已登录；启动前静默刷新 token
  // （界面切换在前，刷新是网络请求，避免点击后界面无反馈）
  if (!isOffline && !msAccount.value) {
    alertMsg.value = "请先登录 Microsoft 账号";
    alertCopyable.value = false;
    showAlert.value = true;
    return;
  }

  const name = isOffline ? username.value.trim() : (msAccount.value?.name ?? "");
  launching.value = true;
  launchPhase.value = "running";
  launchSteps.value = [
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
        alertMsg.value = `正版登录已失效，请重新登录：${String(e)}`;
        alertCopyable.value = true;
        showAlert.value = true;
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

    // 按「启动器可见性」设置调整启动器窗口（最小化/隐藏/关闭）
    (window as any).__applyLaunchVisibility?.();

    // 记录最后一次离线用户名（与正版隔离）
    if (isOffline) {
      await invoke("save_last_offline_username", { username: name });
    }

    // ③ 等待游戏窗口出现（JVM 启动 + 游戏初始化约需数秒）
    await new Promise(r => setTimeout(r, 4000));
    step("等待游戏窗口", "done");
    launchPhase.value = "done";

    // 完成后稍作停留再切回原样
    setTimeout(() => {
      resetLaunch();
    }, 1500);
  } catch (e) {
    resetLaunch();
    alertMsg.value = String(e);
    alertCopyable.value = true;
    showAlert.value = true;
  } finally {
    launching.value = false;
  }
}

onMounted(async () => {
  try {
    const list: string[] = await invoke("get_installed_versions");
    versions.value = list;
    // 上次选择的版本仅在所属目录仍是当前激活目录时才恢复（版本选择页同时写入两者）
    const savedDir = localStorage.getItem("elytra.selectedVersionDir");
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
        versions.value = newList;
        selectedVersion.value = newList.includes(r.version)
          ? r.version
          : newList[0] ?? "";
        localStorage.setItem("elytra.selectedVersion", selectedVersion.value);
        localStorage.setItem("elytra.selectedVersionDir", r.dir_path);
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
});

async function loadMsAccount() {
  try {
    msAccount.value = await invoke<MsAccountInfo | null>("get_ms_account");
  } catch {
    msAccount.value = null;
  }
}

async function startMsLogin() {
  msLoginState.value = "waiting";
  try {
    await invoke("start_ms_login");
    // 结果由 ms-login-complete 事件推送
  } catch (e) {
    msLoginState.value = "idle";
    alertMsg.value = String(e);
    alertCopyable.value = true;
    showAlert.value = true;
  }
}

async function logoutMs() {
  try {
    await invoke("clear_ms_account");
  } catch {
    // ignore
  }
  msAccount.value = null;
}

// ── 编辑信息菜单（修改密码 / 修改玩家名 → 官网） ─────────────────
const EDIT_MENU_H = 96;
const editMenu = ref({ visible: false, x: 0, y: 0 });

function openEditMenu(e: MouseEvent) {
  editMenu.value = {
    visible: true,
    x: Math.min(e.clientX + 6, window.innerWidth - MENU_W - 6),
    y: Math.min(e.clientY + 6, window.innerHeight - EDIT_MENU_H - 6),
  };
}

function closeEditMenu() {
  editMenu.value.visible = false;
}

/** 修改密码：微软账号官方改密页。 */
function openPasswordPage() {
  closeEditMenu();
  openUrl("https://account.live.com/password/change");
}

/** 修改玩家名：Minecraft 官方角色管理页（含改名入口）。 */
function openNameChangePage() {
  closeEditMenu();
  openUrl("https://www.minecraft.net/msa");
}

// ── 皮肤/披风菜单 ────────────────────────────────────────────────
const MENU_W = 176;
const MENU_H = 224;
const skinMenu = ref({ visible: false, x: 0, y: 0 });

/** 点击衣服图标：菜单出现在鼠标右下方（靠近窗口边缘时内收）。
 * 同时后台预取披风列表，保证打开「修改披风」时数据已就绪。 */
function openSkinMenu(e: MouseEvent) {
  skinMenu.value = {
    visible: true,
    x: Math.min(e.clientX + 6, window.innerWidth - MENU_W - 6),
    y: Math.min(e.clientY + 6, window.innerHeight - MENU_H - 6),
  };
  ensureCapes();
}

function closeSkinMenu() {
  skinMenu.value.visible = false;
}

function onWindowClick() {
  if (skinMenu.value.visible) closeSkinMenu();
  if (editMenu.value.visible) closeEditMenu();
}

onMounted(() => window.addEventListener("click", onWindowClick));
onUnmounted(() => window.removeEventListener("click", onWindowClick));

function showErrorAlert(msg: string) {
  alertMsg.value = msg;
  alertCopyable.value = true;
  showAlert.value = true;
}

/** 1. 修改皮肤：本地选择 PNG → 上传官方账号 → 刷新头像。 */
async function uploadSkin() {
  closeSkinMenu();
  const acct = msAccount.value;
  if (!acct) return;
  const path = await open({
    multiple: false,
    filters: [{ name: "PNG 图片", extensions: ["png"] }],
  });
  if (!path) return;
  try {
    await invoke("upload_skin_file", {
      accessToken: acct.access_token,
      filePath: path,
    });
    showToast("皮肤上传成功，正在刷新头像…");
    // 等 Mojang 处理生效后按新 texture ID 渲染
    await new Promise(r => setTimeout(r, 3000));
    await renderPremiumAvatar(acct.uuid, true); // 皮肤已变，强制重新拉取
  } catch (e) {
    showErrorAlert(`皮肤上传失败：${String(e)}`);
  }
}

/** 2. 刷新皮肤：重新获取当前皮肤并更新头像。 */
async function refreshSkin() {
  closeSkinMenu();
  await renderPremiumAvatar(msAccount.value?.uuid ?? "", true); // 主动刷新，忽略缓存
  showToast("皮肤已刷新");
}

/** 3. 保存皮肤文件：文件选择器选目标位置，下载当前皮肤写入。 */
async function saveSkin() {
  closeSkinMenu();
  const acct = msAccount.value;
  if (!acct) return;
  const path = await save({
    defaultPath: `${acct.name}.png`,
    filters: [{ name: "PNG 图片", extensions: ["png"] }],
  });
  if (!path) return;
  try {
    await invoke("save_skin", { uuid: acct.uuid, path });
    showToast("皮肤已保存");
  } catch (e) {
    showErrorAlert(`保存皮肤失败：${String(e)}`);
  }
}

// ── 披风选择 dialog ─────────────────────────────────────────────
type CapeInfo = { id: string; alias: string; url: string; active: boolean };
const capes = ref<CapeInfo[]>([]);
const capeLoading = ref(false);
const capeDialogVisible = ref(false);
/** 当前选中披风 id，"" = 无披风 */
const selectedCapeId = ref("");

/** 拉取官方披风列表，返回是否成功。失败时保留旧缓存（下次可重试）。 */
async function loadCapes(): Promise<boolean> {
  const acct = msAccount.value;
  if (!acct) return false;
  capeLoading.value = true;
  try {
    capes.value = await invoke<CapeInfo[]>("fetch_capes", {
      accessToken: acct.access_token,
    });
    return true;
  } catch (e) {
    showErrorAlert(`获取披风列表失败：${String(e)}`);
    return false;
  } finally {
    capeLoading.value = false;
  }
}

let capesFetched = false;
/** 确保列表已加载过：菜单打开时后台预取，打开 dialog 时数据已就绪、无抖动。 */
async function ensureCapes() {
  if (capesFetched) return;
  if (await loadCapes()) capesFetched = true;
}

/** 5. 修改披风：打开选择 dialog（列表已预取，默认选中当前披风）。 */
async function openCapeDialog() {
  closeSkinMenu();
  await ensureCapes();
  capeDialogVisible.value = true;
  selectedCapeId.value = capes.value.find(c => c.active)?.id ?? "";
}

function closeCapeDialog() {
  capeDialogVisible.value = false;
}

/** 确定：无披风 → 移除；否则切换。成功后 toast 提示。 */
async function confirmCape() {
  const acct = msAccount.value;
  if (!acct) return;
  const cape = capes.value.find(c => c.id === selectedCapeId.value);
  try {
    if (cape) {
      await invoke("set_active_cape", {
        accessToken: acct.access_token,
        capeId: cape.id,
      });
      showToast(`已切换到披风「${cape.alias || cape.id.slice(0, 8)}」`);
    } else {
      await invoke("disable_cape", { accessToken: acct.access_token });
      showToast("已移除披风");
    }
    closeCapeDialog();
    await loadCapes(); // 刷新缓存
  } catch (e) {
    showErrorAlert(`切换披风失败：${String(e)}`);
  }
}

/** 6. 刷新披风列表：重新拉取官方列表。 */
async function refreshCape() {
  closeSkinMenu();
  await loadCapes();
  if (!capeLoading.value) {
    showToast(`披风列表已刷新（${capes.value.length} 个可用）`);
  }
}

onMounted(async () => {
  try {
    const list: string[] = await invoke("get_installed_versions");
    versions.value = list;
    // 优先恢复上次选择（版本选择页写入）；不存在则取列表最后一个
    if (!list.includes(selectedVersion.value)) {
      selectedVersion.value = list.length > 0 ? list[list.length - 1] : "";
    }
  } catch {
    // ignore
  }

  await loadLastUsername();
  await loadMsAccount();
  renderOfflineAvatar(username.value);
  // 账号变化（登录/退出）时渲染正版头像；初始已保存的账号也立即渲染
  watch(msAccount, () => {
    avatarError.value = false;
    renderPremiumAvatar(msAccount.value?.uuid ?? "");
  });
  renderPremiumAvatar(msAccount.value?.uuid ?? "");

  await listen<{ ok: boolean; account: MsAccountInfo | null; error: string | null }>(
    "ms-login-complete",
    (ev) => {
      msLoginState.value = "idle";
      if (ev.payload.ok && ev.payload.account) {
        msAccount.value = ev.payload.account;
      } else {
        alertMsg.value = ev.payload.error || "登录失败";
        alertCopyable.value = true;
        showAlert.value = true;
      }
    },
  );

  await listen<[number, string]>("game-exited", (ev) => {
    console.log("Game exited:", ev.payload);
  });
});
</script>

<template>
  <div class="launch">
    <div class="left">
      <Transition name="left-switch" mode="out-in">
      <!-- 启动进度面板（启动时替换左侧内容，游戏窗口出现后切回） -->
      <div v-if="launchPhase" key="progress" class="lp-panel">
        <div class="lp-head">
          <div class="lp-title">正在启动游戏</div>
          <div class="lp-version">{{ selectedVersion }}</div>
        </div>
        <div class="lp-steps">
          <div v-for="(st, i) in launchSteps" :key="i" class="lp-step" :class="st.status">
            <span class="lp-icon">
              <svg v-if="st.status === 'done'" viewBox="0 0 14 14" width="13" height="13" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M2 7l3.5 3.5L12 3" /></svg>
              <span v-else-if="st.status === 'running'" class="ls-spin"></span>
              <span v-else class="lp-dot"></span>
            </span>
            <span class="lp-name">{{ st.name }}</span>
          </div>
        </div>
        <div v-if="launchPhase === 'done'" class="lp-done">启动完成，游戏窗口即将出现</div>
      </div>

      <!-- 正常内容 -->
      <div v-else key="normal" class="left-normal">
      <!-- auth -->
      <div class="section s-auth">
        <div class="auth-toggle">
          <button
            class="auth-btn"
            :class="{ active: authMode === 'premium' }"
            @click="switchMode('premium')"
          >
            <svg viewBox="0 0 16 16" width="16" height="16" fill="currentColor">
              <path d="M8 1L3 5v3.5C3 12.3 5.3 15.5 8 16c2.7-.5 5-3.7 5-7.5V5L8 1z" />
            </svg>
            <span>正版</span>
          </button>
          <button
            class="auth-btn"
            :class="{ active: authMode === 'offline' }"
            @click="switchMode('offline')"
          >
            <svg viewBox="0 0 16 16" width="16" height="16" fill="currentColor">
              <path d="M8 2a4 4 0 100 8 4 4 0 000-8zm-6 9c0-1.3 2.7-2 6-2s6 .7 6 2v1H2v-1z" />
            </svg>
            <span>离线</span>
          </button>
        </div>
      </div>

      <!-- user -->
      <div class="section s-user">
        <div class="user-area" :class="{ 'menu-open': skinMenu.visible || editMenu.visible }">
          <div class="user-main">
            <div class="avatar">
              <img
                v-if="avatarUrl && !avatarError"
                :src="avatarUrl"
                alt="avatar"
                width="68"
                draggable="false"
                @error="avatarError = true"
              />
              <svg v-else viewBox="0 0 40 40" width="44" height="44" fill="currentColor">
                <circle cx="20" cy="20" r="20" />
                <circle cx="20" cy="15" r="6" fill="var(--surface-elevated)" opacity="0.8" />
                <ellipse cx="20" cy="30" rx="10" ry="7" fill="var(--surface-elevated)" opacity="0.8" />
              </svg>
            </div>
            <div v-if="authMode === 'premium'" class="uname" :class="{ 'uname-empty': !msAccount }">
              {{ msAccount?.name || '未登录' }}
            </div>
            <input
              v-else
              v-model="username"
              class="name-input"
              type="text"
              placeholder="输入用户名"
              maxlength="16"
            />
          </div>

          <div v-if="authMode === 'premium'" class="user-actions">
            <div v-if="msLoginState === 'waiting'" class="ms-login-waiting">
              <span class="ls-spin"></span>
              <span>请在浏览器中完成登录…</span>
            </div>
            <template v-else-if="msAccount">
              <!-- 衣服：皮肤/披风菜单 -->
              <button class="icon-btn" title="皮肤与披风" @click.stop="openSkinMenu($event)">
                <svg viewBox="0 0 16 16" width="14" height="14" fill="currentColor">
                  <path d="M5.5 2h5c.6 0 1.2.4 1.5 1l2.8 1.2c.5.2.7.8.5 1.3l-.4.9c-.2.4-.6.6-1 .6H14v5.5a1.5 1.5 0 01-1.5 1.5h-9A1.5 1.5 0 012 11.5V7h-.4c-.4 0-.8-.2-1-.6l-.4-.9c-.2-.5 0-1.1.5-1.3L4 3c.3-.6.9-1 1.5-1zm.5 1.5v2.5a2 2 0 004 0v-2.5h-4z" />
                </svg>
              </button>
              <!-- 编辑：账号信息菜单（修改密码 / 修改玩家名） -->
              <button class="icon-btn" title="编辑信息" @click.stop="openEditMenu($event)">
                <svg viewBox="0 0 16 16" width="14" height="14" fill="currentColor">
                  <path d="M11.3 1.3a1.8 1.8 0 012.6 2.6L5.6 12.2l-3.5 1 1-3.5 8.2-8.4zm.9 1.7a.5.5 0 00-.7 0L4 10.6l.4 1 1 .4 7.6-7.5a.5.5 0 000-.7l-.8-.8z" />
                </svg>
              </button>
              <!-- 登出 -->
              <button class="icon-btn" title="退出登录" @click="logoutMs">
                <svg viewBox="0 0 16 16" width="14" height="14" fill="currentColor">
                  <path d="M10.5 3h-5a1 1 0 00-1 1v8a1 1 0 001 1h5a1 1 0 001-1V4a1 1 0 00-1-1zM4 1h8a2 2 0 012 2v10a2 2 0 01-2 2H4a2 2 0 01-2-2V3a2 2 0 012-2zm2 5h4v1H6V6zm0 2h4v1H6V8zm0 2h4v1H6v-1z" />
                </svg>
              </button>
            </template>
            <button v-else class="act-btn" @click="startMsLogin">
              <svg viewBox="0 0 16 16" width="14" height="14" fill="currentColor">
                <path d="M8 1L3 5v3.5C3 12.3 5.3 15.5 8 16c2.7-.5 5-3.7 5-7.5V5L8 1z" />
              </svg>
              <span>登录微软账号</span>
            </button>
          </div>

          <!-- 皮肤/披风菜单（跟随鼠标，出现在右下方） -->
          <Teleport to="body">
            <div
              v-if="skinMenu.visible"
              class="skin-menu"
              :style="{ left: skinMenu.x + 'px', top: skinMenu.y + 'px' }"
              @click.stop
            >
              <div class="menu-item" @click="uploadSkin">修改皮肤</div>
              <div class="menu-item" @click="refreshSkin">刷新皮肤</div>
              <div class="menu-item" @click="saveSkin">保存皮肤文件</div>
              <div class="menu-sep"></div>
              <div class="menu-item" @click="openCapeDialog">修改披风</div>
              <div class="menu-item" @click="refreshCape">刷新披风列表</div>
            </div>
          </Teleport>

          <!-- 编辑信息菜单（修改密码 / 修改玩家名） -->
          <Teleport to="body">
            <div
              v-if="editMenu.visible"
              class="skin-menu"
              :style="{ left: editMenu.x + 'px', top: editMenu.y + 'px' }"
              @click.stop
            >
              <div class="menu-item" @click="openPasswordPage">修改密码</div>
              <div class="menu-item" @click="openNameChangePage">修改玩家名</div>
            </div>
          </Teleport>

          <!-- 轻提示 toast -->
          <Teleport to="body">
            <Transition name="toast-fade">
              <div v-if="toastMsg" class="toast">{{ toastMsg }}</div>
            </Transition>
          </Teleport>
        </div>
      </div>

      <!-- launch -->
      <div class="section s-launch">
        <div class="launch-area">
          <button
            class="launch-btn"
            :class="{ launching }"
            :disabled="!selectedVersion || launching"
            @click="doLaunch"
          >
            <template v-if="!selectedVersion">
              <span class="launch-label">下载游戏</span>
              <span class="launch-version">未找到可用的游戏版本</span>
            </template>
            <template v-else-if="launching">
              <span class="launch-label">启动中...</span>
              <span class="launch-version">{{ selectedVersion }}</span>
            </template>
            <template v-else>
              <span class="launch-label">启动游戏</span>
              <span class="launch-version">{{ selectedVersion }}</span>
            </template>
          </button>

          <div class="launch-actions">
            <button class="secondary-btn" @click="router.push('/version-select')">
              <svg viewBox="0 0 16 16" width="14" height="14" fill="currentColor">
                <path d="M2 2h5v5H2V2zm7 0h5v5H9V2zM2 9h5v5H2V9zm7 0h5v5H9V9z" />
              </svg>
              <span>版本选择</span>
            </button>
            <button v-if="selectedVersion" class="secondary-btn" @click="router.push({ path: '/version-setup', query: { name: selectedVersion } })">
              <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round">
                <line x1="21" y1="4" x2="14" y2="4"/>
                <line x1="10" y1="4" x2="3" y2="4"/>
                <line x1="21" y1="12" x2="12" y2="12"/>
                <line x1="8" y1="12" x2="3" y2="12"/>
                <line x1="21" y1="20" x2="16" y2="20"/>
                <line x1="12" y1="20" x2="3" y2="20"/>
                <line x1="14" y1="2" x2="14" y2="6"/>
                <line x1="8" y1="10" x2="8" y2="14"/>
                <line x1="16" y1="18" x2="16" y2="22"/>
              </svg>
              <span>版本设置</span>
            </button>
          </div>
        </div>
      </div>
      </div>
      </Transition>
    </div>

    <div class="right">
      <div v-if="!selectedVersion" class="right-hint">
        <svg viewBox="0 0 48 48" width="48" height="48" fill="none" stroke="currentColor" stroke-width="1.5" opacity="0.3">
          <rect x="8" y="10" width="32" height="28" rx="3"/>
          <line x1="16" y1="20" x2="32" y2="20"/>
          <line x1="16" y1="26" x2="28" y2="26"/>
          <line x1="16" y1="32" x2="24" y2="32"/>
        </svg>
        <p>选择一个游戏版本</p>
      </div>
      <div v-else class="right-placeholder"></div>
    </div>

    <AlertDialog
      v-model="showAlert"
      title="启动失败"
      :message="alertMsg"
      :copyable="alertCopyable"
    />

    <!-- 选择披风 dialog：checkbox 单选列表 + 无披风选项，无分割线 -->
    <AlertDialog v-model="capeDialogVisible" title="选择披风" :divider="false">
      <div class="cape-dialog-list">
        <div
          class="cape-opt"
          :class="{ checked: selectedCapeId === '' }"
          @click="selectedCapeId = ''"
        >
          <span class="cape-cb"></span>
          <span class="cape-opt-name">无披风</span>
        </div>
        <div
          v-for="c in capes"
          :key="c.id"
          class="cape-opt"
          :class="{ checked: selectedCapeId === c.id }"
          @click="selectedCapeId = c.id"
        >
          <span class="cape-cb"></span>
          <span class="cape-opt-name">{{ c.alias || c.id.slice(0, 8) }}</span>
        </div>
      </div>
      <template #actions>
        <button class="dialog-btn primary" @click="confirmCape">确定</button>
        <button class="dialog-btn" @click="closeCapeDialog">取消</button>
      </template>
    </AlertDialog>
  </div>
</template>

<style scoped>
.launch {
  display: flex;
  height: 100%;
}

/* ===== left ===== */
.left {
  flex: 0 0 35%;
  min-width: 280px;
  background-color: var(--surface-elevated);
  border-right: 1px solid var(--border-default);
  display: flex;
  flex-direction: column;
  padding: 20px 24px;
  gap: 16px;
}

.section {
  display: flex;
  align-items: center;
}

.s-auth { /* natural height */ }
.s-user { flex: 1; justify-content: center; }
.s-launch { align-items: flex-start; }

/* auth toggle — like titlebar nav */
.auth-toggle {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 4px;
  width: 100%;
}

.auth-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 6px;
  padding: 6px 20px;
  border-radius: var(--radius-sm);
  border: none;
  background: transparent;
  color: var(--text-tertiary);
  font-size: 13px;
  font-weight: 500;
  cursor: pointer;
  transition: all 0.15s;
}

.auth-btn:hover {
  color: var(--text-secondary);
  background-color: var(--surface-hover);
}

.auth-btn.active {
  color: var(--color-brand);
  background-color: var(--color-brand-bg);
}

/* user section */
.user-area {
  position: relative;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 12px;
}

/* 扩大 hover 热区：鼠标在头像/用户名四周也能保持操作行显示（不占布局、不挡点击） */
.user-area::before {
  content: "";
  position: absolute;
  inset: -20px -32px;
}

.user-main {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 10px;
}

.avatar {
  width: 72px;
  height: 72px;
  border-radius: 0;
  overflow: hidden;
  color: var(--color-brand);
  /* 占位图标时无背景，与周围一致；图片时由 img 铺满 */
  background-color: transparent;
}

.avatar svg {
  width: 100%;
  height: 100%;
}

.uname {
  font-size: 16px;
  font-weight: 600;
  color: var(--text-primary);
}

.name-input {
  width: 100%;
  border: none;
  border-bottom: 2px solid var(--border-default);
  background: transparent;
  padding: 4px 0;
  font-size: 15px;
  font-weight: 500;
  color: var(--text-primary);
  text-align: center;
  outline: none;
  transition: border-color 0.15s;
  /* 盖过 .user-area::before hover 热区，保证可点击聚焦 */
  position: relative;
  z-index: 1;
}

.name-input:focus { border-color: var(--color-brand); }
.name-input::placeholder {
  color: var(--text-tertiary);
  font-weight: 400;
}

/* premium hover actions：始终占位（不影响头像/用户名位置），仅切换透明度 */
.user-actions {
  position: relative;
  z-index: 1; /* 盖过 .user-area::before 热区，保证按钮可点击 */
  display: flex;
  gap: 8px;
  opacity: 0;
  pointer-events: none;
  transition: opacity 0.2s ease;
}

.user-area:hover .user-actions,
.user-area.menu-open .user-actions {
  opacity: 1;
  pointer-events: auto;
}

/* 图标操作按钮（衣服 / 编辑 / 登出） */
.icon-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 30px;
  height: 30px;
  border-radius: var(--radius-sm);
  border: 1px solid var(--border-default);
  background-color: transparent;
  color: var(--text-secondary);
  cursor: pointer;
  transition: all 0.15s;
}

.icon-btn:hover {
  color: var(--color-brand);
  border-color: var(--color-brand);
}

/* 皮肤/披风菜单 */
.skin-menu {
  position: fixed;
  z-index: 1000;
  min-width: 164px;
  padding: 6px;
  background: var(--surface-elevated);
  border: 1px solid var(--border-default);
  border-radius: var(--radius-md);
  box-shadow: 0 8px 24px rgba(0, 0, 0, 0.35);
}

.menu-item {
  padding: 8px 12px;
  font-size: 13px;
  color: var(--text-primary);
  border-radius: var(--radius-sm);
  cursor: pointer;
  white-space: nowrap;
  transition: background 0.12s;
}

.menu-item:hover {
  background: var(--color-brand);
  color: #fff;
}

.menu-sep {
  height: 1px;
  margin: 5px 8px;
  background: var(--border-default);
}

/* 选择披风 dialog：checkbox 单选列表（可滚动） */
.cape-dialog-list {
  max-height: 220px;
  overflow-y: auto;
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.cape-opt {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 7px 8px;
  border-radius: var(--radius-sm);
  cursor: pointer;
  transition: background 0.12s;
}

.cape-opt:hover {
  background: var(--color-brand-bg);
}

/* checkbox 外观：品牌渐变填充 + glow + 回弹勾选动画 */
.cape-cb {
  flex: none;
  width: 16px;
  height: 16px;
  border: 1.5px solid var(--border-strong);
  border-radius: 5px;
  display: flex;
  align-items: center;
  justify-content: center;
  background: #fff;
  transition: border-color 0.15s, background 0.15s, box-shadow 0.15s;
}

.cape-opt:hover .cape-cb {
  border-color: var(--color-brand);
}

.cape-cb::after {
  content: "";
  width: 8px;
  height: 4.5px;
  border-left: 2px solid #fff;
  border-bottom: 2px solid #fff;
  transform: rotate(-45deg) translate(0.5px, -0.5px) scale(0);
  transition: transform 0.18s cubic-bezier(0.34, 1.56, 0.64, 1);
}

.cape-opt.checked .cape-cb {
  background: var(--color-brand-gradient);
  border-color: transparent;
  box-shadow: 0 0 0 3px var(--color-brand-glow);
}

.cape-opt.checked .cape-cb::after {
  transform: rotate(-45deg) translate(0.5px, -0.5px) scale(1);
}

.cape-opt-name {
  font-size: 13px;
  color: var(--text-primary);
  user-select: none;
}

/* dialog 操作按钮 */
.dialog-btn {
  padding: 5px 14px;
  border: 1px solid var(--border-strong);
  border-radius: var(--radius-sm);
  background: transparent;
  color: var(--text-secondary);
  font-size: 12px;
  cursor: pointer;
  transition: all 0.15s;
}

.dialog-btn:hover {
  border-color: var(--color-brand);
  color: var(--color-brand);
}

.dialog-btn.primary {
  background: var(--color-brand);
  border-color: var(--color-brand);
  color: #fff;
}

.dialog-btn.primary:hover {
  opacity: 0.88;
}

/* 轻提示 toast */
.toast {
  position: fixed;
  left: 50%;
  bottom: 36px;
  transform: translateX(-50%);
  z-index: 2000;
  max-width: 480px;
  padding: 8px 18px;
  font-size: 13px;
  color: #fff;
  background: rgba(24, 24, 27, 0.92);
  border: 1px solid var(--border-default);
  border-radius: 999px;
  box-shadow: 0 6px 20px rgba(0, 0, 0, 0.35);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.toast-fade-enter-active,
.toast-fade-leave-active {
  transition: opacity 0.25s, transform 0.25s;
}

.toast-fade-enter-from,
.toast-fade-leave-to {
  opacity: 0;
  transform: translateX(-50%) translateY(8px);
}

.act-btn {
  display: flex;
  align-items: center;
  gap: 4px;
  padding: 4px 10px;
  border-radius: var(--radius-sm);
  border: 1px solid var(--border-default);
  background-color: transparent;
  color: var(--text-secondary);
  font-size: 12px;
  cursor: pointer;
  transition: all 0.15s;
  white-space: nowrap;
}

.act-btn:hover {
  border-color: var(--color-brand);
  color: var(--color-brand);
  background-color: var(--color-brand-bg);
}

/* launch section */
.launch-area {
  width: 100%;
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.launch-btn {
  width: 100%;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 2px;
  padding: 16px;
  border-radius: var(--radius-lg);
  border: none;
  background: var(--color-brand-gradient);
  color: var(--text-inverse);
  cursor: pointer;
  transition: all 0.2s;
  box-shadow: 0 4px 16px var(--color-brand-glow);
}

.launch-btn:hover:not(:disabled) {
  transform: translateY(-1px);
  box-shadow: 0 6px 20px var(--color-brand-glow);
}

.launch-btn:active:not(:disabled) { transform: translateY(0); }
.launch-btn:disabled {
  opacity: 0.4;
  cursor: not-allowed;
  box-shadow: none;
}

.launch-label {
  font-size: 16px;
  font-weight: 600;
  line-height: 1.4;
}

.launch-version {
  font-size: 12px;
  opacity: 0.8;
  line-height: 1.3;
}

.launch-actions {
  display: flex;
  gap: 8px;
}

.secondary-btn {
  flex: 1;
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 6px;
  padding: 8px 0;
  border-radius: var(--radius-md);
  border: 1px solid var(--border-strong);
  background-color: var(--surface-elevated);
  color: var(--text-secondary);
  font-size: 13px;
  font-weight: 500;
  cursor: pointer;
  transition: all 0.15s;
}

.secondary-btn:hover {
  border-color: var(--color-brand);
  color: var(--color-brand);
}

.launch-btn.launching {
  opacity: 0.7;
  pointer-events: none;
}

/* 左侧容器切换动画 */
.left-normal {
  flex: 1;
  display: flex;
  flex-direction: column;
  gap: 16px;
}

.left-switch-enter-active,
.left-switch-leave-active {
  transition: opacity 0.18s ease, transform 0.18s ease;
}

.left-switch-enter-from {
  opacity: 0;
  transform: translateY(10px);
}

.left-switch-leave-to {
  opacity: 0;
  transform: translateY(-10px);
}

/* 头像：visage 3D 渲染图 / 离线像素脸 */
.avatar img {
  width: 100%;
  height: 100%;
  object-fit: cover;
  display: block;
  image-rendering: pixelated;
}

/* 正版账号区 */
.uname-empty {
  color: var(--text-tertiary);
}

.ms-login-waiting {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 12px;
  color: var(--text-secondary);
}

/* 启动进度面板（左侧容器替换） */
.lp-panel {
  flex: 1;
  display: flex;
  flex-direction: column;
  justify-content: center;
  gap: 24px;
  padding: 20px 8px;
}

.lp-head {
  text-align: center;
}

.lp-title {
  font-size: 17px;
  font-weight: 600;
  color: var(--text-primary);
}

.lp-version {
  margin-top: 4px;
  font-size: 12px;
  color: var(--text-tertiary);
}

.lp-steps {
  display: flex;
  flex-direction: column;
  gap: 14px;
  padding: 0 20px;
}

.lp-step {
  display: flex;
  align-items: center;
  gap: 12px;
  font-size: 13px;
  color: var(--text-tertiary);
}

.lp-step.running .lp-name {
  color: var(--color-brand);
  font-weight: 500;
}

.lp-step.done {
  color: var(--text-secondary);
}

.lp-step.done .lp-icon {
  color: #059669;
}

.lp-icon {
  width: 20px;
  height: 20px;
  display: flex;
  align-items: center;
  justify-content: center;
  flex-shrink: 0;
}

.lp-dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  background: var(--border-strong);
}

.ls-spin {
  width: 14px;
  height: 14px;
  border: 2px solid var(--border-strong);
  border-top-color: var(--color-brand);
  border-radius: 50%;
  animation: ls-rotate 0.8s linear infinite;
}

@keyframes ls-rotate {
  to { transform: rotate(360deg); }
}

.lp-done {
  text-align: center;
  font-size: 13px;
  color: #059669;
  font-weight: 500;
}

.drop-enter-active { transition: all 0.15s ease-out; }
.drop-leave-active { transition: all 0.1s ease-in; }
.drop-enter-from { opacity: 0; transform: translateY(-4px); }
.drop-leave-to { opacity: 0; transform: translateY(-4px); }

/* ===== right ===== */
.right {
  flex: 1;
  background-color: var(--surface-page);
  display: flex;
  align-items: center;
  justify-content: center;
}

.right-hint {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 12px;
  color: var(--text-tertiary);
  user-select: none;
}

.right-hint p {
  font-size: 14px;
}

.right-placeholder {
  width: 100%;
  height: 100%;
}


</style>
