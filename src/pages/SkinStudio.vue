<script setup lang="ts">
// 衣柜：左 = 3D 全身预览（拖动旋转，默认 3/4 正面；左上/右上浮动图层），右 = 皮肤 / 披风 tab。
// 交互与状态全在 store/skinStudio（模块级单例），本页仅编排布局与缩略图缓存。
import { computed, nextTick, onMounted, onUnmounted, reactive, ref, watch } from "vue";
import { useRouter } from "vue-router";
import { Check, Download, Plus, RefreshCw, RotateCcw, Shirt, Wind, X } from "@lucide/vue";
import AlertDialog from "../component/common/AlertDialog.vue";
import EmptyState from "../component/common/EmptyState.vue";
import LoadingSpinner from "../component/common/LoadingSpinner.vue";
import { renderSkinThumb3D } from "../store/auth/avatar";
import { drawFullCanvas, loadSkinImage } from "../store/skin/skinRender";
import { ensureAuthReady } from "../store/auth/auth";
import { skinStudio, type LocalSkin } from "../store/skin/skinStudio";
import { showToast } from "../utils/toast";

const router = useRouter();

/** 常驻预览画布：自研等距渲染器直接画到它上面（无 WebGL 上下文、无库）。
 *  拖动/动作变化只改参数后重绘一帧 —— 单帧几毫秒，足够流畅。 */
const stageCanvas = ref<HTMLCanvasElement | null>(null);
/** 已解码的当前皮肤（重绘时复用，不重复解码）。 */
let stageImg: HTMLImageElement | null = null;
/** 已解码的当前披风贴图；无披风时为 null。 */
let stageCape: HTMLImageElement | null = null;
/** 动画帧句柄（0 = 未运行）。静止时停掉循环，不空转。 */
let animRaf = 0;
/** 首帧是否已就绪：三方数据全部到齐并解码完之前，watch 触发的重绘一律抑制——
 *  否则皮肤先到、披风还在路上时会先画出一张没披风的中间图。 */
let stageReady = false;
/** 动画起始时刻（相位基准）。 */
let animStart = 0;

/** 默认占比（模型约占画布高的 40%）；实际占比 = BASE_FILL × skinStudio.zoom。 */
const BASE_FILL = 0.4;

/** 重绘预览一帧（画布尺寸跟随容器 + 设备像素比）。 */
function drawStage(): void {
  const el = stageCanvas.value;
  const box = el?.parentElement;
  if (!el || !box || !stageImg) return;
  const dpr = window.devicePixelRatio || 1;
  const w = Math.max(1, Math.round(box.clientWidth * dpr));
  const h = Math.max(1, Math.round(box.clientHeight * dpr));
  if (el.width !== w || el.height !== h) {
    el.width = w;
    el.height = h;
  }
  drawFullCanvas(el, stageImg, {
    yaw: skinStudio.yaw,
    pitch: skinStudio.pitch,
    slim: skinStudio.currentSlim,
    legacy: stageImg.naturalHeight < 64,
    animation: skinStudio.animation,
    phase: (performance.now() - animStart) / 1000,
    fill: BASE_FILL * skinStudio.zoom,
    capeTex: stageCape ?? undefined,
  });
}

/** 滚轮缩放（桌面 WebView）。 */
function onWheel(e: WheelEvent): void {
  skinStudio.zoomBy(e.deltaY < 0 ? 1.1 : 1 / 1.1);
  if (!animRaf) drawStage();
}

/** 双指捏合缩放（触屏）。单指仍走拖动旋转。 */
let pinchDist = 0;
function touchDist(t: TouchList): number {
  const a = t[0];
  const b = t[1];
  if (!a || !b) return 0;
  return Math.hypot(a.clientX - b.clientX, a.clientY - b.clientY);
}
function onTouchStart(e: TouchEvent): void {
  if (e.touches.length === 2) {
    pinchDist = touchDist(e.touches);
    skinStudio.dragging = false; // 双指时不旋转
  }
}
function onTouchMove(e: TouchEvent): void {
  if (e.touches.length !== 2) return;
  e.preventDefault();
  const d = touchDist(e.touches);
  if (pinchDist > 0 && d > 0) {
    skinStudio.zoomBy(d / pinchDist);
    if (!animRaf) drawStage();
  }
  pinchDist = d;
}
function onTouchEnd(): void {
  pinchDist = 0;
}

/** 动画帧循环（仅「待机/行走」时运行）。 */
function animTick(): void {
  drawStage();
  animRaf = requestAnimationFrame(animTick);
}

/** 动作切换：起停动画循环。静止时停帧并重绘一张定格图。 */
function syncAnimation(): void {
  const wantLoop = skinStudio.animation !== "none";
  if (wantLoop && !animRaf) {
    animStart = performance.now();
    animRaf = requestAnimationFrame(animTick);
  } else if (!wantLoop && animRaf) {
    cancelAnimationFrame(animRaf);
    animRaf = 0;
    drawStage();
  }
}

/** 解码当前皮肤（不重绘）。拆分出来是为了首帧能「皮肤 + 披风都解码完再画一次」，
 *  避免出现只有皮肤、披风还在路上的中间帧。 */
async function decodeSkin(): Promise<void> {
  const skin = skinStudio.currentSkin;
  if (!skin) {
    stageImg = null;
    return;
  }
  try {
    stageImg = await loadSkinImage(skin.startsWith("data:") ? skin : `data:image/png;base64,${skin}`);
  } catch {
    stageImg = null;
  }
}

/** 解码当前披风贴图（不重绘；无披风则清空）。 */
async function decodeCape(): Promise<void> {
  const cape = skinStudio.currentCapeTexture;
  if (!cape) {
    stageCape = null;
    return;
  }
  try {
    stageCape = await loadSkinImage(cape.startsWith("data:") ? cape : `data:image/png;base64,${cape}`);
  } catch {
    stageCape = null;
  }
}

/** 皮肤变化 → 解码并重绘（首帧未就绪时只解码不画）。 */
async function refreshSkin(): Promise<void> {
  await decodeSkin();
  if (stageReady && !animRaf) drawStage();
}

/** 披风变化 → 解码并重绘（同上）。 */
async function refreshCape(): Promise<void> {
  await decodeCape();
  if (stageReady && !animRaf) drawStage();
}

/** 衣柜只服务正版账号：第三方账号的外观一律在站点网页管理（点头像直接跳过去），
 *  离线没有任何皮肤来源。两个入口都已在 AccountSection 拦住，这里兜底直接改 URL / 历史前进后退。 */
onMounted(async () => {
  // 进入即切刷新态：store 是模块级单例，否则上一次会话的缓存列表会先闪一帧
  skinStudio.phase = "loading";
  // 账号列表是异步 IPC：未就绪就判定来源，会把正版账号当成离线而误拦（store 的 loadOfficial 同样先 await）。
  await ensureAuthReady();
  if (!skinStudio.premium) {
    showToast("皮肤衣柜仅对正版账号开放");
    void router.replace("/");
    return;
  }
  await nextTick(); // 等 canvas 挂载
  stageImg = null;
  stageCape = null;
  // 皮肤与披风**都**就绪后再渲染首帧：先等三方数据（load 内部已是并发拉取），
  // 再并发解码两张图，最后只画一次 —— 不会先冒出一张没披风的中间图。
  await skinStudio.load();
  await Promise.all([decodeSkin(), decodeCape()]);
  stageReady = true; // 数据齐 + 两张图解码完 → 放行重绘（含首帧）
  if (!animRaf) drawStage();
  syncAnimation();
});

onUnmounted(() => {
  cancelAnimationFrame(animRaf);
  animRaf = 0;
  stageReady = false;
  stageImg = null;
  stageCape = null;
  skinStudio.dispose();
});

/** 皮肤 / 模型变化 → 重新解码重绘 */
watch(
  () => [skinStudio.currentSkin, skinStudio.currentSlim] as const,
  () => void refreshSkin(),
);

/** 披风选择变化 → 重新解码重绘 */
watch(
  () => skinStudio.currentCapeTexture,
  () => void refreshCape(),
);

/** 拖动 / 重置视角 / 缩放 → 重绘一帧 */
watch(
  () => [skinStudio.yaw, skinStudio.pitch, skinStudio.zoom] as const,
  () => {
    if (!animRaf) drawStage();
  },
);

/** 动作切换 → 起停动画循环 */
watch(
  () => skinStudio.animation,
  () => syncAnimation(),
);

/** 预览动作选项。名称按场景语义取，不直接暴露库的动画类名。 */
const ANIM_OPTIONS = [
  { value: "none", label: "静止" },
  { value: "idle", label: "待机" },
  { value: "walk", label: "行走" },
] as const;

/** 皮肤模型的中文名——与左侧预览的类型标签、模型弹窗用同一套说法（CLASSIC / SLIM 是
 *  后端与官方 API 的取值，不该直接暴露给用户）。
 *  缺失/未知值按「经典」兜底：一个角标不该让整张卡片渲染中断。 */
function variantLabel(variant?: string | null): string {
  return variant?.toUpperCase() === "SLIM" ? "纤细" : "经典";
}

/** 平面皮肤图（3D 缩略图未就绪 / 渲染失败时的兜底）：64×64 纹理里截头部正面 (8,8,8,8) 区域放大
 *  （size 800% + position 14.286% = 8/56 → 精确对准该 UV 区）。 */
function skinThumb(b64: string) {
  return {
    backgroundImage: `url(data:image/png;base64,${b64})`,
    backgroundSize: "800% 800%",
    backgroundPosition: "14.286% 14.286%",
  };
}

/** 披风缩略图：64×32 纹理里截披风正面 (0,0,10,16) 区域。 */
function capeThumb(b64: string) {
  return {
    backgroundImage: `url(data:image/png;base64,${b64})`,
    backgroundSize: "640% 200%",
    backgroundPosition: "0 0",
  };
}

/** 卡片 3D 缩略图缓存键 = 皮肤贴图 + 模型：两者相同即同一张图，无需失效逻辑。
 *  **不含披风**：缩略图不渲染披风（见 thumbReqs），披风贴图又是异步到达的——
 *  一旦计入键，首批缩略图会在披风列表到达后全部作废重渲（整屏双倍开销）。 */
function thumbKey(b64: string, slim: boolean) {
  return `${slim ? 1 : 0}|${b64}`;
}

/** 卡片 3D 缩略图（键见 thumbKey）→ dataURL；未就绪 / 渲染失败留空串（回落平面图）。 */
const thumbs = reactive<Record<string, string>>({});

/** 去重后的本地条目（缩略图渲染与卡片列表共用同一条去重规则）：
 *  实时卡的贴图先占位（实时卡恒显示，本地一律让它），本地之间同一份贴图只留最靠前的一条
 *  （后端按加入时间倒序 → 留最新）；无贴图（文件缺失）的条目不参与去重，各自保留。 */
const localEntries = computed<LocalSkin[]>(() => {
  const seen = new Set<string>();
  const official = skinStudio.officialSkin;
  if (official) seen.add(official);
  const out: LocalSkin[] = [];
  for (const s of skinStudio.skins) {
    if (s.skin && seen.has(s.skin)) continue; // 与实时卡 / 更靠前的本地条目重复
    if (s.skin) seen.add(s.skin);
    out.push(s);
  }
  return out;
});

/** 待渲染的皮肤（官方当前 + 本地条目，须有贴图）——卡片增减 / 换账号时补齐。
 *  **不渲染披风**：60×102 的卡面上披风几乎不可辨（正面视角还被身体挡住），
 *  信息由左侧大预览（可拖动旋转）承担；且披风贴图异步到达，若参与渲染会让整屏缩略图重渲一次。 */
const thumbReqs = computed(() => {
  const reqs: { key: string; b64: string; slim: boolean }[] = [];
  const official = skinStudio.officialSkin;
  const officialSlim = skinStudio.officialModel.toLowerCase() === "slim";
  if (official) {
    reqs.push({ key: thumbKey(official, officialSlim), b64: official, slim: officialSlim });
  }
  for (const s of localEntries.value) {
    if (!s.skin) continue;
    const slim = s.variant === "SLIM";
    reqs.push({ key: thumbKey(s.skin, slim), b64: s.skin, slim });
  }
  return reqs;
});

// 渲染器内部同样按「贴图 + 模型」缓存，这里再记一层结果：tab 往返 / 重复皮肤都不重渲。
watch(
  thumbReqs,
  (reqs) => {
    for (const r of reqs) {
      if (thumbs[r.key] !== undefined) continue; // 已就绪（含失败标记）
      thumbs[r.key] = ""; // 占位：同一张贴图只提交一次渲染
      void renderSkinThumb3D(r.b64, r.slim)
        .then((url) => {
          thumbs[r.key] = url;
        })
        .catch((e) => console.error("3D 缩略图渲染失败:", e)); // 留空串 → 平面图兜底
    }
  },
  { immediate: true },
);

/** 皮肤卡片（官方当前置首 + 去重后的本地条目；信息全部走角标，卡片内不再有名称行）。 */
interface SkinCard {
  key: string;
  /** 皮肤贴图 base64；空串 = 无图（占位方块） */
  skin: string;
  /** 3D 缩略图 dataURL；空串 = 未就绪 / 失败 → 回落平面皮肤图 */
  thumb: string;
  /** 皮肤模型：卡片左上角标（CLASSIC / SLIM） */
  variant: string;
  /** 本地条目（可删除）；官方当前卡片为 null */
  local: LocalSkin | null;
}

const skinCards = computed<SkinCard[]>(() => {
  const cards: SkinCard[] = [];
  const official = skinStudio.officialSkin;
  const officialSlim = skinStudio.officialModel.toLowerCase() === "slim";
  // 无实时皮肤（未登录且内置默认皮肤也没取到）时不占一张空卡：否则空态与空卡并存，自相矛盾
  if (official) {
    cards.push({
      key: "official",
      skin: official,
      thumb: thumbs[thumbKey(official, officialSlim)] || "",
      variant: officialSlim ? "SLIM" : "CLASSIC",
      local: null,
    });
  }
  for (const s of localEntries.value) {
    const slim = s.variant === "SLIM";
    cards.push({
      key: s.id,
      skin: s.skin,
      thumb: s.skin ? thumbs[thumbKey(s.skin, slim)] || "" : "",
      variant: s.variant,
      local: s,
    });
  }
  return cards;
});

/** 可见的本地条目数（去重后）；面板计数与网格内容保持一致。 */
const visibleSkinCount = computed(() => localEntries.value.length);

/** 实际高亮的卡片：选中的本地条目被去重隐藏时（与当前皮肤同一张图）改高亮实时卡——
 *  两者本就是这个账号的同一份皮肤，避免出现「选了却看不到选中态」。 */
const effectiveSelectedKey = computed(() => {
  const key = skinStudio.selectedKey;
  if (key === "official") return key;
  return localEntries.value.some((s) => s.id === key) ? key : "official";
});

/** 删除卡片（仅本地条目有删除按钮）。 */
function removeCard(c: SkinCard) {
  if (c.local) void skinStudio.removeSkin(c.local);
}

/** 上传皮肤时可选的模型（经典 / 纤细）——两个选项，无默认值。 */
const MODEL_OPTIONS = [
  { variant: "CLASSIC", label: "经典模型", sub: "Steve · 4px 宽手臂" },
  { variant: "SLIM", label: "纤细模型", sub: "Alex · 3px 窄手臂" },
] as const;

/** 上传弹窗里当前选中的模型；空串 = 还没选（确认按钮不可用，逼出一次明确的点击）。 */
const pickedModel = ref<"" | "CLASSIC" | "SLIM">("");

// 每次重新打开上传弹窗都清空选择：上一次的选择不作为下一次的默认值。
watch(
  () => skinStudio.skinModelDialog,
  (open) => {
    if (open) pickedModel.value = "";
  },
);

/** 按用户明确选中的模型把待上传文件收进衣柜。 */
function savePickedSkin() {
  if (pickedModel.value) void skinStudio.confirmSaveSkin(pickedModel.value);
}
</script>

<template>
  <div class="studio">
    <!-- 左：3D 全身预览（按住拖动旋转，滚轮 / 双指捏合缩放；初始化 3/4 正面微俯） -->
    <div class="stage-col">
      <div
        class="stage"
        :class="{ grabbing: skinStudio.dragging }"
        @pointerdown="skinStudio.beginDrag($event)"
        @pointermove="skinStudio.moveDrag($event)"
        @pointerup="skinStudio.endDrag($event)"
        @pointercancel="skinStudio.endDrag($event)"
        @wheel.prevent="onWheel"
        @touchstart="onTouchStart"
        @touchmove="onTouchMove"
        @touchend="onTouchEnd"
      >
        <!-- 等距渲染画布：皮肤 / 模型 / 视角 / 缩放 / 动作变化后重绘一帧 -->
        <canvas v-show="!!skinStudio.currentSkin" ref="stageCanvas" class="frame"></canvas>
        <LoadingSpinner
          v-if="!skinStudio.currentSkin && skinStudio.phase === 'loading'"
          text="正在加载皮肤与披风…"
        />
        <EmptyState v-else-if="!skinStudio.currentSkin">
          <p>暂无可预览的皮肤</p>
          <p>登录账号或从本地衣柜选一件</p>
        </EmptyState>

        <!-- 左上浮动图层：当前预览的皮肤类型（经典 / 纤细） -->
        <div class="layer type-layer">
          <span class="type-tag" :class="{ slim: skinStudio.currentSlim }">
            {{ skinStudio.currentSlim ? "纤细" : "经典" }}
          </span>
        </div>

        <!-- 右上浮动图层：操作按钮（仅图标 + tooltip；按钮上按下不触发旋转 → @pointerdown.stop） -->
        <div class="layer action-layer">
          <button
            class="tool fab icon-only"
            v-tooltip="'重置视角与缩放'"
            @pointerdown.stop
            @click="skinStudio.resetView"
          >
            <RotateCcw :size="15" />
          </button>
          <button
            class="tool fab icon-only"
            v-tooltip="'重新拉取皮肤与披风'"
            @pointerdown.stop
            @click="skinStudio.refresh"
          >
            <RefreshCw :size="15" />
          </button>
          <button
            class="tool fab icon-only"
            :disabled="!skinStudio.premium"
            v-tooltip="'导出账号当前皮肤 PNG'"
            @pointerdown.stop
            @click="skinStudio.saveSkinFile"
          >
            <Download :size="15" />
          </button>
          <button
            v-if="skinStudio.selectedLocal"
            class="tool fab primary"
            :disabled="!skinStudio.canApply"
            v-tooltip="'上传为账号当前皮肤'"
            @pointerdown.stop
            @click="skinStudio.applySelected"
          >
            <Check :size="15" />
            <span>{{ skinStudio.applyBusy ? "应用中" : "应用" }}</span>
          </button>
        </div>

        <!-- 底部居中：动作切换（静止 / 待机 / 行走） -->
        <div class="layer anim-layer">
          <div class="anim-group">
            <button
              v-for="a in ANIM_OPTIONS"
              :key="a.value"
              class="anim-btn"
              :class="{ on: skinStudio.animation === a.value }"
              @pointerdown.stop
              @click="skinStudio.animation = a.value"
            >
              {{ a.label }}
            </button>
          </div>
        </div>
      </div>
    </div>

    <!-- 右：衣柜（皮肤 / 披风 tab 共用一个面板） -->
    <div class="wardrobe">
      <section class="panel">
        <header class="panel-hd">
          <div class="tabs">
            <button
              class="tab"
              :class="{ active: skinStudio.tab === 'skins' }"
              @click="skinStudio.tab = 'skins'"
            >
              <Shirt :size="14" /><span>皮肤</span>
            </button>
            <button
              class="tab"
              :class="{ active: skinStudio.tab === 'capes' }"
              @click="skinStudio.tab = 'capes'"
            >
              <Wind :size="14" /><span>披风</span>
            </button>
          </div>
          <div class="panel-hd-right">
            <span v-if="skinStudio.tab === 'skins' && visibleSkinCount" class="panel-count">
              {{ visibleSkinCount }} 件收藏
            </span>
            <span
              v-else-if="skinStudio.tab === 'capes' && skinStudio.premium && skinStudio.capes.length"
              class="panel-count"
            >
              {{ skinStudio.capes.filter(c => c.active).length ? "已装备" : "未装备" }}
            </span>
          </div>
        </header>

        <!-- 皮肤 tab：3D 侧视缩略图网格（3 列 × 2 行，超出滚动；末尾一格为上传卡片） -->
        <div v-if="skinStudio.tab === 'skins'" class="panel-body">
          <!-- 刷新态：整个右侧只显示刷新中，不与旧列表共存 -->
          <div v-if="skinStudio.phase === 'loading'" class="panel-loading">
            <LoadingSpinner text="正在获取皮肤与披风…" />
          </div>
          <template v-else>
          <div class="grid skins">
            <div
              v-for="c in skinCards"
              :key="c.key"
              class="card"
              :class="{ active: effectiveSelectedKey === c.key }"
              @click="skinStudio.selectSkin(c.key)"
            >
              <div class="thumb-3d">
                <img v-if="c.thumb" class="thumb-3d-img" :src="c.thumb" alt="" draggable="false" />
                <div v-else-if="c.skin" class="thumb" :style="skinThumb(c.skin)"></div>
                <div v-else class="thumb empty"></div>
              </div>
              <!-- 角标只剩左上角的皮肤类型（来源角标已删除：衣柜现在只服务正版账号与本地文件，
                   来源不再有区分度） -->
              <span class="card-tag variant" :class="{ slim: c.variant === 'SLIM' }">
                {{ variantLabel(c.variant) }}
              </span>
              <button
                v-if="c.local"
                class="card-del"
                v-tooltip="'从衣柜删除'"
                @click.stop="removeCard(c)"
              >
                <X :size="12" />
              </button>
            </div>

            <!-- 上传本地皮肤：与普通卡片区分（虚线框 + 加号），放在网格末尾 -->
            <div
              class="card upload"
              v-tooltip="'选择本地 64×64 皮肤 PNG，上传为账号当前皮肤'"
              @click="skinStudio.pickSkinFile()"
            >
              <span class="upload-plus"><Plus :size="20" /></span>
              <div class="card-name"><span class="nm">上传本地皮肤</span></div>
            </div>
          </div>

          <!-- 空态只看「有没有实时卡」：账号当前皮肤即使没进本地衣柜也已有卡片，算不得空 -->
          <EmptyState v-if="!visibleSkinCount && !skinStudio.officialSkin">
            <p>衣柜还是空的</p>
            <p>点下方「上传本地皮肤」把皮肤文件收进来</p>
          </EmptyState>
          <p v-if="skinStudio.skinsError" class="err">{{ skinStudio.skinsError }}</p>
          </template>
        </div>

        <!-- 披风 tab -->
        <div v-else class="panel-body">
          <div v-if="skinStudio.premium && skinStudio.phase === 'loading'" class="panel-loading">
            <LoadingSpinner text="正在获取披风…" />
          </div>
          <template v-else-if="skinStudio.premium">
            <div class="grid">
              <div
                class="card"
                :class="{ active: skinStudio.selectedCapeId === '' }"
                v-tooltip="'不装备披风'"
                @click="skinStudio.useCape('')"
              >
                <div class="thumb cape none"><X :size="18" /></div>
                <div class="card-name">无披风</div>
              </div>
              <div
                v-for="c in skinStudio.capes"
                :key="c.id"
                class="card"
                :class="{ active: skinStudio.selectedCapeId === c.id }"
                @click="skinStudio.useCape(c.id)"
              >
                <div v-if="c.texture" class="thumb cape" :style="capeThumb(c.texture)"></div>
                <div v-else class="thumb cape empty"></div>
                <div class="card-name" :title="c.alias || c.id">
                  {{ c.alias || c.id.slice(0, 8) }}
                </div>
              </div>
            </div>

            <EmptyState v-if="!skinStudio.capes.length">
              <p>该账号暂无可用披风</p>
            </EmptyState>
            <p v-if="skinStudio.capesError" class="err">{{ skinStudio.capesError }}</p>
          </template>
          <EmptyState v-else>
            <p>披风需要正版账号</p>
            <p>登录微软账号后可在此切换 / 移除披风</p>
          </EmptyState>
        </div>
      </section>

      <!-- 状态提示（无内容时不占位） -->
      <footer v-if="!skinStudio.premium || skinStudio.officialError" class="wardrobe-foot">
        <p v-if="!skinStudio.premium" class="hint">
          本地衣柜可用于预览；应用皮肤 / 管理披风需登录正版账号
        </p>
        <p v-if="skinStudio.officialError" class="err">{{ skinStudio.officialError }}</p>
      </footer>
    </div>

    <!-- 上传皮肤前选择模型：经典（Steve 宽）/ 纤细（Alex 窄）。无默认值，必须点一次再确认 -->
    <AlertDialog v-model="skinStudio.skinModelDialog" title="选择皮肤模型">
      <div class="model-desc">
        皮肤将直接上传为账号当前皮肤；模型不可中途更换（需重新上传），按所选模型应用。
      </div>
      <div class="model-pick">
        <button
          v-for="m in MODEL_OPTIONS"
          :key="m.variant"
          class="model-opt"
          :class="{ on: pickedModel === m.variant }"
          @click="pickedModel = m.variant"
        >
          <span class="model-mark" />
          <span class="model-name">{{ m.label }}</span>
          <span class="model-sub">{{ m.sub }}</span>
        </button>
      </div>
      <template #actions>
        <button class="dialog-btn" @click="skinStudio.skinModelDialog = false">取消</button>
        <button class="dialog-btn primary" :disabled="!pickedModel" @click="savePickedSkin">
          确认
        </button>
      </template>
    </AlertDialog>
  </div>
</template>

<style scoped>
/* 刷新态：占满面板主体并居中（刷新时右侧不残留旧列表） */
.panel-loading {
  display: flex;
  align-items: center;
  justify-content: center;
  min-height: 180px;
}
.studio {
  display: flex;
  height: 100%;
  min-height: 0;
}

/* ===== 左：3D 预览 ===== */
.stage-col {
  flex: 0 0 38%;
  min-width: 300px;
  display: flex;
  flex-direction: column;
  min-height: 0;
  background-color: var(--surface-elevated);
  border-right: 1px solid var(--border-default);
}

.stage {
  position: relative;
  flex: 1;
  min-height: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  /* 四周等距留白（底部提示已移除，无需额外空间） */
  padding: 16px;
  overflow: hidden;
  cursor: grab;
  /* 拖动旋转：禁掉浏览器手势/选中，pointer 事件才不会被滚屏吞掉 */
  touch-action: none;
  user-select: none;
  -webkit-user-select: none;
  background:
    radial-gradient(circle at 50% 28%, var(--color-brand-bg), transparent 68%),
    var(--surface-page);
}

.stage.grabbing {
  cursor: grabbing;
}

/* 常驻 WebGL 画布：尺寸由 ResizeObserver → viewer.resize 设定（含设备像素比），
   这里不做任何缩放 —— 对 canvas 用 max-width/max-height 会让像素被拉伸变形。 */
.frame {
  display: block;
  image-rendering: pixelated;
  pointer-events: none;
  -webkit-user-drag: none;
}

/* ===== 浮动图层（叠在 3D 图之上；空白处不吃指针 → 不挡拖动） ===== */
.layer {
  position: absolute;
  z-index: 2;
  display: flex;
  pointer-events: none;
}

.type-layer {
  top: 12px;
  left: 12px;
}

.type-tag {
  padding: 3px 10px;
  border: 1px solid var(--border-default);
  border-radius: 999px;
  background-color: var(--surface-card);
  box-shadow: var(--shadow-sm);
  font-size: 11.5px;
  font-weight: 600;
  letter-spacing: 0.4px;
  color: var(--text-secondary);
}

.type-tag.slim {
  border-color: var(--color-brand);
  color: var(--color-brand);
  background-color: var(--color-brand-bg);
}

.action-layer {
  top: 12px;
  right: 12px;
  flex-direction: column;
  align-items: flex-end;
  gap: 6px;
}

/* 底部居中动作切换：分段按钮，选中项用品牌色（接管指针，不触发拖动） */
.anim-layer {
  left: 0;
  right: 0;
  bottom: 14px;
  justify-content: center;
}

.anim-group {
  display: flex;
  align-items: center;
  gap: 2px;
  padding: 3px;
  border-radius: var(--radius-sm);
  background-color: var(--surface-card);
  box-shadow: var(--shadow-sm);
  pointer-events: auto;
}

.anim-btn {
  padding: 4px 10px;
  border: none;
  border-radius: var(--radius-sm);
  background-color: transparent;
  color: var(--text-secondary);
  font-size: 12px;
  cursor: pointer;
  transition: color 0.15s, background-color 0.15s;
}

.anim-btn:hover {
  color: var(--text-primary);
}

.anim-btn.on {
  color: var(--color-brand);
  background-color: var(--color-brand-bg);
  font-weight: 600;
}

.tool {
  display: flex;
  align-items: center;
  gap: 5px;
  padding: 5px 11px;
  border-radius: var(--radius-sm);
  border: 1px solid var(--border-default);
  background-color: transparent;
  color: var(--text-secondary);
  font-size: 12px;
  white-space: nowrap;
  cursor: pointer;
  transition: all 0.15s;
}

/* 浮动按钮：实底 + 投影，浮在 3D 图之上；接管指针（与模板 @pointerdown.stop 配套） */
.tool.fab {
  pointer-events: auto;
  background-color: var(--surface-card);
  box-shadow: var(--shadow-sm);
  height: 30px;
}

/* 仅图标按钮：正方形、图标居中（文案交给 tooltip），把浮层的占位压到最小 */
.tool.icon-only {
  width: 30px;
  padding: 0;
  justify-content: center;
  gap: 0;
  flex-shrink: 0;
}

.tool:hover:not(:disabled) {
  border-color: var(--color-brand);
  color: var(--color-brand);
  background-color: var(--color-brand-bg);
}

.tool:disabled {
  opacity: 0.45;
  cursor: not-allowed;
}

.tool.primary {
  border-color: transparent;
  background: var(--color-brand-gradient);
  color: var(--text-inverse);
  font-weight: 500;
}

.tool.primary:hover:not(:disabled) {
  color: var(--text-inverse);
  box-shadow: 0 0 0 3px var(--color-brand-glow);
}

.err {
  margin: 0;
  font-size: 11.5px;
  color: var(--color-danger);
  white-space: pre-line;
  word-break: break-all;
}

.hint {
  margin: 0;
  font-size: 11.5px;
  color: var(--text-tertiary);
}

/* ===== 右：衣柜 ===== */
.wardrobe {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
}

.panel {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
}

.panel-hd {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  padding: 9px 16px;
  border-bottom: 1px solid var(--border-default);
}

/* tab 切换：皮肤 / 披风（同 side-nav 的「品牌色 + 品牌底色」激活态） */
.tabs {
  display: flex;
  align-items: center;
  gap: 2px;
  padding: 2px;
  border-radius: var(--radius-sm);
  background-color: var(--surface-hover);
}

.tab {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 5px 12px;
  border: none;
  border-radius: var(--radius-sm);
  background-color: transparent;
  color: var(--text-secondary);
  font-size: 12.5px;
  font-weight: 500;
  cursor: pointer;
  transition: color 0.15s, background-color 0.15s;
}

.tab:hover {
  color: var(--text-primary);
}

.tab.active {
  color: var(--color-brand);
  background-color: var(--surface-card);
  font-weight: 600;
}

.panel-hd-right {
  display: flex;
  align-items: center;
  gap: 10px;
}

.panel-count {
  font-size: 11.5px;
  color: var(--text-tertiary);
}

.panel-body {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  padding: 14px 16px 18px;
}

.wardrobe-foot {
  flex: none;
  display: flex;
  flex-direction: column;
  gap: 6px;
  padding: 10px 16px 12px;
  border-top: 1px solid var(--border-default);
}

.grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(96px, 1fr));
  gap: 12px;
}

/* 皮肤网格：每行 3 个、最多可见 2 行（再多的在网格内滚动）
   行高固定 → 各卡尺寸一致；minmax(0,1fr) 防内容撑破列宽 */
.grid.skins {
  --row-h: 186px;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  grid-auto-rows: var(--row-h);
  max-height: calc(var(--row-h) * 2 + 12px); /* 2 行 + 行间距 */
  overflow-y: auto;
}

.card {
  position: relative;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 7px;
  padding: 10px 8px 8px;
  border: 1px solid var(--border-default);
  border-radius: var(--radius-md);
  background-color: var(--surface-card);
  cursor: pointer;
  transition: border-color 0.15s, background-color 0.15s, box-shadow 0.15s;
}

.card:hover {
  border-color: var(--color-brand);
  background-color: var(--color-brand-bg);
}

/* 选中态：用 inset 环而不是外扩 box-shadow——外扩环的圆角半径是「卡片圆角 + 外扩量」，
   与卡片不同步；且外扩部分会被网格滚动容器在边缘裁掉（左右阴影不对称、边角像被切方）。
   inset 环的圆角恒等于卡片圆角，也不会被任何父容器裁切。 */
.card.active {
  border-color: var(--color-brand);
  box-shadow: inset 0 0 0 2px var(--color-brand-glow);
}

/* 3D 侧视缩略图：撑满卡片内容区（高度由卡片行高决定 → 明显加高，60×102 帧等比缩放不糊）；
   min-height 仅作兜底（行高失效时也不会塌成 0） */
.thumb-3d {
  flex: 1;
  min-height: 120px;
  width: 100%;
  display: flex;
  align-items: center;
  justify-content: center;
  background: radial-gradient(circle at 50% 38%, var(--color-brand-bg), transparent 68%);
}

.thumb-3d-img {
  max-width: 100%;
  max-height: 100%;
  image-rendering: pixelated;
  pointer-events: none;
  -webkit-user-drag: none;
}

/* 平面皮肤图：仅在 3D 缩略图未就绪 / 失败时短暂出现 */
.thumb {
  width: 64px;
  height: 64px;
  border-radius: 4px;
  background-color: var(--surface-hover);
  background-repeat: no-repeat;
  image-rendering: pixelated;
}

/* 披风贴图正面为 10×16 体素 → 缩略图保持同比例（40×64） */
.thumb.cape {
  width: 40px;
  height: 64px;
}

.thumb.empty {
  background-image: none;
}

.thumb.cape.none {
  display: flex;
  align-items: center;
  justify-content: center;
  color: var(--text-tertiary);
}

.card-name {
  max-width: 100%;
  display: flex;
  align-items: center;
  gap: 4px;
  font-size: 12px;
  color: var(--text-secondary);
}

/* 名称超出省略（皮肤卡已无名称行——名称/来源一律走角标；仅上传卡与披风卡还有名称行） */
.card-name .nm {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.card.active .card-name {
  color: var(--color-brand);
  font-weight: 600;
}

.card-tag {
  position: absolute;
  top: 5px;
  left: 5px;
  padding: 1px 5px;
  border-radius: 4px;
  font-size: 9.5px;
  font-weight: 700;
  letter-spacing: 0.4px;
  color: var(--text-tertiary);
  background-color: var(--surface-hover);
}

/* 皮肤类型角标：经典中性、纤细品牌色（中文比原来的 CLASSIC / SLIM 短，字号略放大保证可读） */
.card-tag.variant {
  font-size: 10.5px;
}

.card-tag.variant.slim {
  color: var(--color-brand);
  background-color: var(--color-brand-bg);
}

/* 上传卡片（网格最后一格）：虚线框 + 圆形加号，与普通卡片区分。
   边框/文字比普通卡片用更深一档的 token——太浅的话在深色主题下几乎看不见。 */
.card.upload {
  justify-content: center;
  gap: 8px;
  border-style: dashed;
  border-color: var(--border-strong);
  background-color: transparent;
  color: var(--text-secondary);
}

.card.upload:hover {
  border-color: var(--color-brand);
  background-color: var(--color-brand-bg);
  color: var(--color-brand);
}

.card.upload:hover .card-name {
  color: inherit;
}

.upload-plus {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 44px;
  height: 44px;
  /* 1.5px + border-strong：虚线本身的视觉重量比实线轻，用粗一档补回来 */
  border: 1.5px dashed var(--border-strong);
  border-radius: 50%;
}

.card.upload:hover .upload-plus {
  border-color: var(--color-brand);
}

.card-del {
  position: absolute;
  top: 4px;
  right: 4px;
  display: flex;
  align-items: center;
  justify-content: center;
  width: 20px;
  height: 20px;
  border: none;
  border-radius: var(--radius-sm);
  background: var(--surface-card);
  color: var(--text-tertiary);
  cursor: pointer;
  opacity: 0;
  visibility: hidden; /* 隐藏态彻底移出命中测试（WebView2 下 pointer-events 过渡不可靠） */
  transition: opacity 0.12s, visibility 0.12s, color 0.12s, background-color 0.12s;
}

.card:hover .card-del {
  opacity: 1;
  visibility: visible;
}

.card-del:hover {
  color: var(--color-danger);
  background-color: var(--color-danger-bg);
}

.model-desc {
  font-size: 12.5px;
  color: var(--text-secondary);
  line-height: 1.6;
}

/* 模型选择：两张等权选项卡，默认一张也不选（必须明确点一次才能确认） */
.model-pick {
  display: flex;
  gap: 8px;
  margin-top: 12px;
}

.model-opt {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  gap: 3px;
  padding: 8px 10px;
  border: 1px solid var(--border-default);
  border-radius: var(--radius-md);
  background-color: var(--surface-card);
  text-align: left;
  cursor: pointer;
  transition: border-color 0.15s, background-color 0.15s;
}

.model-opt:hover {
  border-color: var(--color-brand);
  background-color: var(--color-brand-bg);
}

.model-opt.on {
  border-color: var(--color-brand);
  background-color: var(--color-brand-bg);
}

/* 单选圆点：未选为空圈，选中为品牌色实心（inset 环留出内圈空隙，三端 WebView 均支持） */
.model-mark {
  flex: none;
  width: 12px;
  height: 12px;
  box-sizing: border-box;
  border: 1px solid var(--border-strong);
  border-radius: 50%;
  margin-bottom: 2px;
}

.model-opt.on .model-mark {
  border-color: var(--color-brand);
  background-color: var(--color-brand);
  box-shadow: inset 0 0 0 3px var(--surface-card);
}

.model-name {
  font-size: 12.5px;
  font-weight: 600;
  color: var(--text-primary);
}

.model-opt.on .model-name {
  color: var(--color-brand);
}

.model-sub {
  font-size: 11px;
  color: var(--text-tertiary);
}

/* 还没选模型时「确认」不可用（灰掉，不给出任何默认模型） */
.dialog-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}
</style>
