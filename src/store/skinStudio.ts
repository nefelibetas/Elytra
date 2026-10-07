// 皮肤衣柜 store（模块级 reactive 单例，项目约定无 Pinia）：
//   左：3D 全身预览帧（复用 avatar.renderSkinFull3D，模块级缓存已渲染帧）
//   右：官方皮肤/披风 + 本地衣柜（list_local_skins / save_local_skin_file / remove_local_skin）
// 拖动旋转：pointer 事件累加 yaw/pitch（俯仰限 ±40°），每帧按需重渲（实测 15–37ms）。
import { reactive } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";
import { authMode, externalAccount, msAccount } from "./auth";
import { renderPremiumAvatar, renderSkinFull3D } from "./avatar";
import { showError } from "./alert";
import { showToast } from "../utils/toast";

/** 本地衣柜条目（后端 LocalSkinView）。 */
export interface LocalSkin {
  id: string;
  name: string;
  /** 皮肤模型（后端归一化为大写；与官方皮肤 API 的 variant 字符串一致） */
  variant: "CLASSIC" | "SLIM";
  /** 加入时间（Unix 秒） */
  added_at: number;
  /** 衣柜内绝对路径（可直接交给 upload_skin_file 应用） */
  path: string;
  /** 皮肤 PNG base64（预览/缩略图用；文件缺失时为空串） */
  skin: string;
}

/** 官方披风条目（后端 CapeInfo）。 */
export interface StudioCape {
  id: string;
  alias: string;
  url: string;
  active: boolean;
  /** 披风贴图 base64（下载失败为空串；WebView 无 http 插件，只能由后端带出） */
  texture: string;
}

/** 每体素像素数：预览画布固定为 20×34 体素（宽 120 / 高 204），随容器等比缩放。 */
const PPV = 6;
/** 俯仰角上限（±40°，越界会让模型上下翻转、观察不适）。 */
const PITCH_LIMIT = 40;
/** 拖动灵敏度（度/像素）。 */
const YAW_SENS = 0.6;
const PITCH_SENS = 0.5;
/** 已渲染帧缓存上限（超出按插入顺序淘汰最旧的）。 */
const FRAME_CACHE_MAX = 96;
/** 上传官方账号后等待 Mojang 处理新纹理生效的时间。 */
const APPLY_SETTLE_MS = 3000;

/** 已渲染帧缓存：key = 选择令牌 + 圆整视角 → dataURL（拖动中间帧不写缓存）。 */
const frameCache = new Map<string, string>();
/** 选择令牌：皮肤 / 披风任一变化即自增，旧帧缓存自然失效（免把 base64 塞进 key）。 */
let skinToken = 0;
let capeToken = 0;
/** 渲染序号：只应用最后一次请求的结果，拖动时不堆积过期帧。 */
let renderSeq = 0;
/** 待执行的重渲 rAF 句柄（把同一帧内的多次 pointermove 合并成一次重渲）。 */
let rafId = 0;
/** 拖动会话：上一次指针位置（非响应式，仅内部用）。 */
let lastX = 0;
let lastY = 0;

/** 写入帧缓存，超过上限先淘汰最旧键。 */
function putFrame(key: string, url: string) {
  if (frameCache.size >= FRAME_CACHE_MAX) {
    const oldest = frameCache.keys().next().value;
    if (oldest !== undefined) frameCache.delete(oldest);
  }
  frameCache.set(key, url);
}

export const skinStudio = reactive({
  // ── 3D 预览 ──
  /** 当前帧（dataURL）；空串 = 暂无可预览皮肤 */
  frame: "",
  rendering: false,
  dragging: false,
  yaw: 0,
  pitch: 0,

  // ── 账号当前皮肤 ──
  officialSkin: "",
  /** "classic" | "slim"（后端 fetch_*_skin_profile 原样返回） */
  officialModel: "classic",
  officialLoading: false,
  officialError: "",

  // ── 本地衣柜 ──
  skins: [] as LocalSkin[],
  skinsLoading: false,
  skinsError: "",

  // ── 官方披风 ──
  capes: [] as StudioCape[],
  capeLoading: false,
  capesError: "",
  capeBusy: false,

  // ── 选择状态 ──
  /** 'official' = 账号当前皮肤；否则为本地条目 id */
  selectedKey: "official",
  /** 预览披风 id（= 当前启用披风）；"" = 无披风 */
  selectedCapeId: "",

  // ── 上传皮肤（选文件 → 确认模型） ──
  applyBusy: false,
  skinModelDialog: false,
  pendingSkinPath: "",

  // ── 派生 ──
  /** 可执行账号级操作（应用皮肤 / 管理披风）——须为已登录的正版账号。 */
  get premium(): boolean {
    return authMode.value === "premium" && !!msAccount.value;
  },

  /** 当前选中的本地条目（选中官方皮肤时为 null）。 */
  get selectedLocal(): LocalSkin | null {
    if (this.selectedKey === "official") return null;
    return this.skins.find((s) => s.id === this.selectedKey) ?? null;
  },

  /** 预览用皮肤 base64（选中本地项优先）。 */
  get currentSkin(): string {
    return this.selectedLocal?.skin || this.officialSkin;
  },

  /** 预览是否按纤细模型渲染。 */
  get currentSlim(): boolean {
    const local = this.selectedLocal;
    if (local) return local.variant === "SLIM";
    return this.officialModel.toLowerCase() === "slim";
  },

  /** 预览披风贴图 base64（无披风 / 贴图缺失时为空串）。 */
  get currentCapeTexture(): string {
    if (!this.selectedCapeId) return "";
    return this.capes.find((c) => c.id === this.selectedCapeId)?.texture ?? "";
  },

  /** 「应用为当前皮肤」是否可用。 */
  get canApply(): boolean {
    return this.premium && !!this.selectedLocal && !this.applyBusy;
  },

  // ── 加载 ──

  /** 页面挂载：并发拉取官方皮肤 / 本地衣柜 / 官方披风，然后渲染首帧。 */
  async load() {
    this.yaw = 0;
    this.pitch = 0;
    await Promise.allSettled([this.loadOfficial(), this.loadSkins(), this.loadCapes()]);
    skinToken++;
    this.scheduleRender();
  },

  /** 拉取账号当前皮肤（正版 / 第三方 / 离线各自来源）；失败回退内置 Steve 保证预览可用。 */
  async loadOfficial() {
    this.officialLoading = true;
    this.officialError = "";
    const acc = msAccount.value;
    const ext = externalAccount.value;
    try {
      if (this.premium && acc) {
        const p = await invoke<{ skin: string; model: string }>("fetch_account_skin_profile", {
          uuid: acc.uuid,
        });
        this.officialSkin = p.skin;
        this.officialModel = p.model || "classic";
      } else if (authMode.value === "thirdparty" && ext) {
        const p = await invoke<{ skin: string; model: string }>("fetch_external_skin_profile", {
          apiUrl: ext.api_url,
          uuid: ext.uuid,
        });
        this.officialSkin = p.skin;
        this.officialModel = p.model || "classic";
      } else {
        this.officialSkin = await invoke<string>("fetch_default_skin", { skin: "steve" });
        this.officialModel = "classic";
      }
    } catch (e) {
      this.officialError = `当前皮肤加载失败\n${String(e)}`;
      try {
        this.officialSkin = await invoke<string>("fetch_default_skin", { skin: "steve" });
        this.officialModel = "classic";
      } catch {
        this.officialSkin = "";
      }
    } finally {
      this.officialLoading = false;
      skinToken++;
    }
  },

  /** 拉取本地衣柜列表（后端按加入时间倒序）。 */
  async loadSkins() {
    this.skinsLoading = true;
    this.skinsError = "";
    try {
      this.skins = await invoke<LocalSkin[]>("list_local_skins");
      // 选中项已不存在（被删/换账号）→ 回落官方皮肤
      if (this.selectedKey !== "official" && !this.skins.some((s) => s.id === this.selectedKey)) {
        this.selectedKey = "official";
        skinToken++;
      }
    } catch (e) {
      this.skinsError = `读取本地衣柜失败\n${String(e)}`;
    } finally {
      this.skinsLoading = false;
    }
  },

  /** 拉取官方披风并把预览同步到当前启用披风（非正版账号跳过）。 */
  async loadCapes() {
    this.capesError = "";
    if (!this.premium) {
      this.capes = [];
      this.selectedCapeId = "";
      return;
    }
    this.capeLoading = true;
    try {
      this.capes = await invoke<StudioCape[]>("fetch_capes");
      this.selectedCapeId = this.capes.find((c) => c.active)?.id ?? "";
      capeToken++;
    } catch (e) {
      this.capesError = `披风列表加载失败\n${String(e)}`;
    } finally {
      this.capeLoading = false;
    }
  },

  /** 刷新：重新拉取三方数据并重渲当前帧。 */
  async refresh() {
    await Promise.allSettled([this.loadOfficial(), this.loadSkins(), this.loadCapes()]);
    this.scheduleRender();
    showToast("已刷新");
  },

  /** 注销：取消待执行的重渲（模块级单例状态留给下次进入复用）。 */
  dispose() {
    if (rafId) {
      cancelAnimationFrame(rafId);
      rafId = 0;
    }
    this.dragging = false;
  },

  // ── 渲染 ──

  /** 合并同一帧内的多次拖动事件后重渲。 */
  scheduleRender() {
    if (rafId) return;
    rafId = requestAnimationFrame(() => {
      rafId = 0;
      void this.render();
    });
  },

  /** 渲染当前选择 + 视角的一帧（命中缓存直接取用）。 */
  async render() {
    const skin = this.currentSkin;
    if (!skin) {
      this.frame = "";
      return;
    }
    const key = `${skinToken}|${capeToken}|${Math.round(this.yaw)}|${Math.round(this.pitch)}`;
    const cached = frameCache.get(key);
    if (cached) {
      this.frame = cached;
      return;
    }
    const seq = ++renderSeq;
    this.rendering = true;
    try {
      const url = await renderSkinFull3D(
        skin,
        this.currentSlim,
        this.yaw,
        this.pitch,
        PPV,
        this.currentCapeTexture || undefined,
      );
      if (seq !== renderSeq) return; // 已有更新的渲染请求，丢弃本帧
      this.frame = url;
      if (!this.dragging) putFrame(key, url); // 拖动中的中间帧不占缓存
    } catch (e) {
      if (seq === renderSeq) console.error("3D 预览渲染失败:", e);
    } finally {
      if (seq === renderSeq) this.rendering = false;
    }
  },

  // ── 拖动旋转 ──

  beginDrag(e: PointerEvent) {
    if (e.button !== 0) return; // 仅左键/单指（右键留给上下文菜单）
    e.preventDefault();
    const el = e.currentTarget as HTMLElement | null;
    try {
      el?.setPointerCapture(e.pointerId);
    } catch {
      // 指针已失效：忽略，后续 move 由元素自身接收
    }
    this.dragging = true;
    lastX = e.clientX;
    lastY = e.clientY;
  },

  moveDrag(e: PointerEvent) {
    if (!this.dragging) return;
    this.yaw += (e.clientX - lastX) * YAW_SENS;
    this.pitch = Math.max(-PITCH_LIMIT, Math.min(PITCH_LIMIT, this.pitch + (e.clientY - lastY) * PITCH_SENS));
    lastX = e.clientX;
    lastY = e.clientY;
    this.scheduleRender();
  },

  endDrag(e: PointerEvent) {
    if (!this.dragging) return;
    this.dragging = false;
    const el = e.currentTarget as HTMLElement | null;
    try {
      if (el?.hasPointerCapture(e.pointerId)) el.releasePointerCapture(e.pointerId);
    } catch {
      // 已释放：忽略
    }
    this.scheduleRender(); // 落定帧此时才写缓存
  },

  /** 重置视角（正面）。 */
  resetView() {
    this.yaw = 0;
    this.pitch = 0;
    this.scheduleRender();
  },

  // ── 衣柜操作 ──

  /** 选中皮肤（'official' 或本地条目 id）→ 立即预览。 */
  selectSkin(key: string) {
    if (this.selectedKey === key) return;
    this.selectedKey = key;
    skinToken++;
    this.scheduleRender();
  },

  /** 上传本地文件到衣柜：先选 PNG，再确认模型。 */
  async pickSkinFile() {
    const path = await open({
      multiple: false,
      filters: [{ name: "PNG 图片", extensions: ["png"] }],
    });
    if (!path || typeof path !== "string") return;
    this.pendingSkinPath = path;
    this.skinModelDialog = true;
  },

  /** 确认模型 → 交由后端读字节存入衣柜（校验 PNG 魔数 + 尺寸）。 */
  async confirmSaveSkin(variant: "CLASSIC" | "SLIM") {
    this.skinModelDialog = false;
    const path = this.pendingSkinPath;
    this.pendingSkinPath = "";
    if (!path) return;
    try {
      // 条目名默认取文件名（去扩展名）
      const name = (path.split(/[\\/]/).pop() ?? path).replace(/\.[^.]+$/, "") || "未命名皮肤";
      const item = await invoke<LocalSkin>("save_local_skin_file", {
        name,
        filePath: path,
        variant,
      });
      this.skins = [item, ...this.skins.filter((s) => s.id !== item.id)];
      this.selectSkin(item.id); // 新入柜即选中预览
      showToast(`已加入衣柜：${item.name}`);
    } catch (e) {
      showError(`保存到衣柜失败：${String(e)}`, true);
    }
  },

  /** 从衣柜删除（后端返回删除后的完整列表）。 */
  async removeSkin(item: LocalSkin) {
    try {
      this.skins = await invoke<LocalSkin[]>("remove_local_skin", { id: item.id });
      if (this.selectedKey === item.id) {
        this.selectedKey = "official";
        skinToken++;
        this.scheduleRender();
      }
      showToast(`已删除：${item.name}`);
    } catch (e) {
      showError(`删除衣柜皮肤失败：${String(e)}`, true);
    }
  },

  /** 把选中的本地皮肤应用为账号当前皮肤（上传官方）。 */
  async applySelected() {
    const local = this.selectedLocal;
    const acc = msAccount.value;
    if (!this.premium || !acc || !local || this.applyBusy) return;
    this.applyBusy = true;
    try {
      await invoke("upload_skin_file", { filePath: local.path, variant: local.variant });
      showToast("皮肤已上传，等待生效…");
      await new Promise((r) => setTimeout(r, APPLY_SETTLE_MS)); // 等 Mojang 处理新纹理
      await this.loadOfficial();
      this.selectedKey = "official";
      skinToken++;
      this.scheduleRender();
      void renderPremiumAvatar(acc.uuid, true); // 启动页头像同步
      showToast("皮肤已更新");
    } catch (e) {
      showError(`应用皮肤失败：${String(e)}`, true);
    } finally {
      this.applyBusy = false;
    }
  },

  /** 导出账号当前皮肤到用户选定路径。 */
  async saveSkinFile() {
    const acc = msAccount.value;
    if (!this.premium || !acc) return;
    const path = await save({
      defaultPath: `${acc.name}.png`,
      filters: [{ name: "PNG 图片", extensions: ["png"] }],
    });
    if (!path) return;
    try {
      await invoke("save_skin", { uuid: acc.uuid, path });
      showToast("皮肤已保存");
    } catch (e) {
      showError(`保存皮肤失败：${String(e)}`, true);
    }
  },

  // ── 披风操作 ──

  /** 点选披风：立即预览并切换（id = "" 表示移除披风）；失败回滚预览。 */
  async useCape(id: string) {
    if (!this.premium || this.capeBusy || id === this.selectedCapeId) return;
    const prev = this.selectedCapeId;
    this.selectedCapeId = id;
    capeToken++;
    this.scheduleRender();
    this.capeBusy = true;
    try {
      if (id) await invoke("set_active_cape", { capeId: id });
      else await invoke("disable_cape");
      if (id) {
        const alias = this.capes.find((c) => c.id === id)?.alias || id.slice(0, 8);
        showToast(`已切换到披风「${alias}」`);
      } else {
        showToast("已移除披风");
      }
      await this.loadCapes();
      this.scheduleRender();
    } catch (e) {
      this.selectedCapeId = prev;
      capeToken++;
      this.scheduleRender();
      showError(`切换披风失败：${String(e)}`, true);
    } finally {
      this.capeBusy = false;
    }
  },
});
