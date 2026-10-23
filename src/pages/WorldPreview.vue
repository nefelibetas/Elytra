<script setup lang="ts">
// 世界预览：真实 region 地图 + 维度/Y 切面 + 浮动面板。
import { computed, onMounted, reactive, ref, watch } from "vue";
import { useRoute } from "vue-router";
import { useI18n } from "vue-i18n";
import {
  ChevronDown,
  ChevronUp,
  Copy,
  FolderOpen,
  Grid3X3,
  Layers,
  Minus,
  Plus,
  RotateCcw,
  SlidersHorizontal,
} from "@lucide/vue";
import { getMapWorldInfo, type MapWorldInfo } from "../api/map";
import WorldMapCanvas from "../component/map/WorldMapCanvas.vue";
import { ensureGameFolder } from "../store/config/gameDirs";
import { openFolder } from "../utils/fs";
import { showToast } from "../utils/toast";
import { errMsg } from "../utils/error";
import { blockLabel } from "../utils/blockNames";
import { currentLocale } from "../i18n";
import LoadingSpinner from "../component/common/LoadingSpinner.vue";
import EmptyState from "../component/common/EmptyState.vue";
import ErrorState from "../component/common/ErrorState.vue";

interface HoverInfo {
  x: number;
  z: number;
  y: number | null;
  block: string | null;
  biome: string | null;
  pending?: boolean;
}

const route = useRoute();
const { t } = useI18n();

const versionName = computed(() => String(route.query.version ?? ""));
const worldName = computed(() => String(route.query.world ?? ""));
const canvasRef = ref<InstanceType<typeof WorldMapCanvas> | null>(null);

const info = ref<MapWorldInfo | null>(null);
const loading = ref(true);
const loadError = ref("");
const mapState = ref<"loading" | "ready" | "empty" | "error">("loading");
const mapError = ref("");

const dimension = ref("");
const mode = ref<"topdown" | "slice">("topdown");
const sliceY = ref(64);
const showGrid = ref(false);
const chunkBorders = ref(false);
const modColors = ref(true);

const hover = ref<HoverInfo | null>(null);
const pinned = ref<HoverInfo | null>(null);

const panelKey = "elytra.worldPreview.panels";
const panels = reactive({ dim: true, coord: true, config: false });

const style = computed(() => (mode.value === "slice" ? `slice@${sliceY.value}` : "topdown"));
const active = computed(() => pinned.value ?? hover.value);
const coordText = computed(() => {
  const c = active.value;
  if (!c) return "X —   Y —   Z —";
  return `X ${c.x}   Y ${c.y ?? "—"}   Z ${c.z}`;
});
const blockText = computed(() => {
  const c = active.value;
  if (!c) return t("worldPreview.noBlock");
  const raw = c.block;
  if (!raw) return c.pending ? t("worldPreview.loadingBlock") : t("worldPreview.noBlock");
  const id = raw.startsWith("minecraft:") ? raw.slice("minecraft:".length) : raw;
  if (id === "air" || id === "cave_air" || id === "void_air") return t("worldPreview.air");
  if (currentLocale.value === "zh-CN") return blockLabel(raw);
  return id.replace(/_/g, " ");
});

function dimensionLabel(id: string): string {
  if (id === "") return t("worldPreview.dimensionOverworld");
  if (id === "DIM-1") return t("worldPreview.dimensionNether");
  if (id === "DIM1") return t("worldPreview.dimensionEnd");
  return id.replace(/^dimensions\//, "");
}

function loadPanels() {
  try {
    const saved = JSON.parse(localStorage.getItem(panelKey) ?? "") as Partial<typeof panels>;
    panels.dim = saved.dim ?? true;
    panels.coord = saved.coord ?? true;
    panels.config = saved.config ?? false;
  } catch {
    // 使用默认展开状态
  }
}
function savePanels() {
  try {
    localStorage.setItem(panelKey, JSON.stringify(panels));
  } catch {
    // 存储不可用时仅本会话生效
  }
}
loadPanels();
watch(panels, savePanels, { deep: true });

async function loadInfo() {
  loading.value = true;
  loadError.value = "";
  try {
    if (!versionName.value || !worldName.value) {
      throw new Error(t("worldPreview.error"));
    }
    const data = await getMapWorldInfo(versionName.value, worldName.value);
    info.value = data;
    dimension.value = data.dimensions[0] ?? "";
    sliceY.value = Math.min(data.max_y, Math.max(data.min_y, 64));
    mapState.value = data.dimensions.length ? "loading" : "empty";
  } catch (e) {
    loadError.value = errMsg(e);
  } finally {
    loading.value = false;
  }
}

function onHover(value: HoverInfo | null) {
  hover.value = value;
}
function onPin(value: HoverInfo | null) {
  pinned.value = value;
  if (value) hover.value = value;
}
function onState(value: "loading" | "ready" | "empty" | "error", detail?: string) {
  mapState.value = value;
  mapError.value = detail ?? "";
}

function retry() {
  mapState.value = "loading";
  mapError.value = "";
  void canvasRef.value?.refresh();
}

watch(sliceY, (value) => {
  if (!info.value) return;
  const next = Number.isFinite(value)
    ? Math.min(info.value.max_y, Math.max(info.value.min_y, Math.round(value)))
    : 64;
  if (next !== value) sliceY.value = next;
});

async function copyCoords() {
  const c = active.value;
  if (!c) return;
  try {
    await navigator.clipboard.writeText(`${c.x} ${c.y ?? ""} ${c.z}`.trim());
    showToast(t("common.action.copied"));
  } catch {
    showToast(t("common.action.copyFailed"));
  }
}

async function copySeed() {
  if (!info.value?.seed) return;
  try {
    await navigator.clipboard.writeText(info.value.seed);
    showToast(t("worldPreview.seedCopied"));
  } catch {
    showToast(t("common.action.copyFailed"));
  }
}

async function openSaves() {
  try {
    const path = await ensureGameFolder("saves", versionName.value);
    await openFolder(path);
  } catch (e) {
    showToast(errMsg(e));
  }
}

onMounted(() => {
  // 全页画布不能被上一个页面的滚动位置推到标题栏下方。
  document.querySelector<HTMLElement>(".main-content")?.scrollTo({ top: 0 });
  void loadInfo();
});
</script>

<template>
  <div class="world-preview">
    <LoadingSpinner v-if="loading" class="wp-center" :text="t('worldPreview.loading')" />
    <ErrorState v-else-if="loadError" class="wp-center" :text="loadError" />
    <template v-else-if="info">
      <WorldMapCanvas
        v-if="info.dimensions.length > 0"
        ref="canvasRef"
        :version-name="versionName"
        :world-name="worldName"
        :dimension="dimension"
        :render-style="style"
        :show-grid="showGrid"
        :chunk-borders="chunkBorders"
        :mod-colors="modColors"
        @hover="onHover"
        @pin="onPin"
        @state="onState"
      />
      <EmptyState v-if="mapState === 'empty'" class="wp-center">
        <Layers :size="40" :stroke-width="1.5" opacity="0.5" />
        <p>{{ t("worldPreview.empty") }}</p>
      </EmptyState>
      <ErrorState v-else-if="mapState === 'error'" class="wp-center">
        <p>{{ mapError ? errMsg(mapError) : t("worldPreview.error") }}</p>
        <button class="wp-btn" @click="retry">{{ t("worldPreview.retry") }}</button>
      </ErrorState>
      <LoadingSpinner v-else-if="mapState === 'loading'" class="wp-center" :text="t('worldPreview.loading')" />

      <!-- 顶部中间：维度 -->
      <section class="wp-panel wp-top" :class="{ collapsed: !panels.dim }">
        <button
          class="wp-panel-head"
          :aria-expanded="panels.dim"
          @click="panels.dim = !panels.dim"
        >
          <Layers :size="14" />
          <span>{{ panels.dim ? t("worldPreview.dimensions") : dimensionLabel(dimension) }}</span>
          <ChevronDown v-if="!panels.dim" :size="13" />
          <ChevronUp v-else :size="13" />
        </button>
        <Transition name="expand">
          <div v-if="panels.dim" class="wp-dims">
            <button
              v-for="id in info.dimensions"
              :key="id || 'overworld'"
              class="wp-dim"
              :class="{ active: dimension === id }"
              @click="dimension = id"
            >
              {{ dimensionLabel(id) }}
            </button>
          </div>
        </Transition>
      </section>

      <!-- 右上角：缩放/重置 -->
      <div class="wp-controls">
        <button class="wp-icon-btn" :title="t('worldPreview.zoomIn')" :aria-label="t('worldPreview.zoomIn')" @click="canvasRef?.zoomIn()">
          <Plus :size="15" />
        </button>
        <button class="wp-icon-btn" :title="t('worldPreview.zoomOut')" :aria-label="t('worldPreview.zoomOut')" @click="canvasRef?.zoomOut()">
          <Minus :size="15" />
        </button>
        <button class="wp-icon-btn" :title="t('worldPreview.resetView')" :aria-label="t('worldPreview.resetView')" @click="canvasRef?.resetView()">
          <RotateCcw :size="14" />
        </button>
      </div>

      <!-- 右侧中间：配置 -->
      <section class="wp-panel wp-config" :class="{ collapsed: !panels.config }">
        <button class="wp-panel-head" :aria-expanded="panels.config" @click="panels.config = !panels.config">
          <SlidersHorizontal :size="14" />
          <span>{{ t("worldPreview.config") }}</span>
          <ChevronDown v-if="!panels.config" :size="13" />
          <ChevronUp v-else :size="13" />
        </button>
        <Transition name="expand">
          <div v-if="panels.config" class="wp-config-body">
            <div class="wp-group">
              <div class="wp-group-title">{{ t("worldPreview.mode") }}</div>
              <div class="wp-segment">
                <button :class="{ active: mode === 'topdown' }" @click="mode = 'topdown'">{{ t("worldPreview.topdown") }}</button>
                <button :class="{ active: mode === 'slice' }" @click="mode = 'slice'">{{ t("worldPreview.slice") }}</button>
              </div>
              <label v-if="mode === 'slice'" class="wp-range">
                <span>{{ t("worldPreview.yLayer") }}</span>
                <input v-model.number="sliceY" type="range" :min="info.min_y" :max="info.max_y" step="1" />
                <input v-model.number="sliceY" class="wp-number" type="number" :min="info.min_y" :max="info.max_y" step="1" />
              </label>
            </div>

            <div class="wp-group">
              <div class="wp-group-title">{{ t("worldPreview.display") }}</div>
              <label class="wp-check">
                <input v-model="showGrid" type="checkbox" />
                <Grid3X3 :size="13" />
                <span>{{ t("worldPreview.grid") }}</span>
              </label>
              <label class="wp-check">
                <input v-model="chunkBorders" type="checkbox" />
                <Layers :size="13" />
                <span>{{ t("worldPreview.chunkBorders") }}</span>
              </label>
              <label class="wp-check">
                <input v-model="modColors" type="checkbox" />
                <span>{{ t("worldPreview.modColors") }}</span>
              </label>
            </div>

            <div class="wp-group">
              <div class="wp-group-title">{{ t("worldPreview.data") }}</div>
              <button class="wp-link" :disabled="!info.seed" @click="copySeed">
                <Copy :size="13" />
                <span>{{ t("worldPreview.copySeed") }}</span>
              </button>
              <button class="wp-link" @click="openSaves">
                <FolderOpen :size="13" />
                <span>{{ t("worldPreview.openFolder") }}</span>
              </button>
            </div>
          </div>
        </Transition>
      </section>

      <!-- 底部中间：坐标 + 方块名 -->
      <section class="wp-panel wp-coord" :class="{ collapsed: !panels.coord }">
        <div class="wp-coord-main">
          <span class="wp-coord-text">{{ coordText }}</span>
          <button class="wp-icon-btn wp-copy" :title="t('common.action.copy')" :aria-label="t('common.action.copy')" @click="copyCoords">
            <Copy :size="13" />
          </button>
          <button class="wp-icon-btn" :aria-expanded="panels.coord" :aria-label="panels.coord ? t('worldPreview.collapse') : t('worldPreview.expand')" @click="panels.coord = !panels.coord">
            <ChevronDown v-if="!panels.coord" :size="13" />
            <ChevronUp v-else :size="13" />
          </button>
        </div>
        <Transition name="expand">
          <div v-if="panels.coord" class="wp-block" :title="active?.block ?? ''">
            {{ blockText }}
          </div>
        </Transition>
      </section>
    </template>
  </div>
</template>

<style scoped>
.world-preview {
  position: relative;
  height: 100%;
  min-height: 0;
  overflow: hidden;
  background: var(--surface-page);
}

.wp-center {
  position: absolute;
  inset: 0;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  z-index: 2;
  pointer-events: none;
}

.wp-center :deep(button) { pointer-events: auto; }

.wp-panel {
  position: absolute;
  z-index: 4;
  color: var(--text-primary);
  border: 1px solid var(--border-default);
  border-radius: var(--radius-md);
  background: var(--surface-card);
  box-shadow: var(--shadow-md, 0 8px 24px rgb(0 0 0 / 0.18));
  backdrop-filter: blur(10px);
  background: color-mix(in srgb, var(--surface-card) 86%, transparent);
}

@supports not (background: color-mix(in srgb, red 50%, transparent)) {
  .wp-panel { background: var(--surface-card); }
}

.wp-panel-head {
  width: 100%;
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 8px 10px;
  border: none;
  background: transparent;
  color: var(--text-secondary);
  font-size: 12px;
  font-weight: 600;
  cursor: pointer;
  white-space: nowrap;
}

.wp-panel-head:hover { color: var(--text-primary); }

.wp-top {
  top: 12px;
  left: 50%;
  transform: translateX(-50%);
  max-width: min(520px, calc(100% - 220px));
}

.wp-dims {
  display: flex;
  gap: 4px;
  padding: 0 8px 8px;
  overflow-x: auto;
}

.wp-dim {
  flex: none;
  padding: 5px 9px;
  border: 1px solid var(--border-default);
  border-radius: var(--radius-sm);
  background: transparent;
  color: var(--text-secondary);
  font-size: 11.5px;
  cursor: pointer;
  white-space: nowrap;
}

.wp-dim:hover { color: var(--text-primary); border-color: var(--border-strong); }
.wp-dim.active { color: var(--color-brand); border-color: var(--color-brand); background: color-mix(in srgb, var(--color-brand) 10%, transparent); }

.wp-controls {
  position: absolute;
  top: 12px;
  right: 14px;
  z-index: 5;
  display: flex;
  flex-direction: column;
  gap: 4px;
  padding: 4px;
  border: 1px solid var(--border-default);
  border-radius: var(--radius-md);
  background: var(--surface-card);
  background: color-mix(in srgb, var(--surface-card) 86%, transparent);
  backdrop-filter: blur(10px);
}

@supports not (background: color-mix(in srgb, red 50%, transparent)) {
  .wp-controls { background: var(--surface-card); }
}

.wp-icon-btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 26px;
  height: 26px;
  border: none;
  border-radius: var(--radius-sm);
  background: transparent;
  color: var(--text-secondary);
  cursor: pointer;
}

.wp-icon-btn:hover { color: var(--text-primary); background: var(--surface-hover); }

.wp-config {
  top: 50%;
  right: 14px;
  width: 232px;
  max-height: calc(100% - 150px);
  transform: translateY(-50%);
  overflow: hidden;
}

.wp-config .wp-panel-head { justify-content: flex-start; }

.wp-config-body {
  display: flex;
  flex-direction: column;
  gap: 12px;
  max-height: calc(100vh - 230px);
  overflow-y: auto;
  padding: 0 10px 10px;
}

.wp-group {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.wp-group + .wp-group {
  padding-top: 10px;
  border-top: 1px solid var(--border-subtle, var(--border-default));
}

.wp-group-title {
  font-size: 11px;
  font-weight: 600;
  color: var(--text-tertiary);
  letter-spacing: 0.02em;
}

.wp-segment {
  display: flex;
  gap: 2px;
  padding: 2px;
  border: 1px solid var(--border-default);
  border-radius: var(--radius-sm);
}

.wp-segment button {
  flex: 1;
  padding: 4px 6px;
  border: none;
  border-radius: calc(var(--radius-sm) - 1px);
  background: transparent;
  color: var(--text-secondary);
  font-size: 11.5px;
  cursor: pointer;
}

.wp-segment button.active {
  background: var(--surface-elevated);
  color: var(--text-primary);
}

.wp-range {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 11px;
  color: var(--text-secondary);
}

.wp-range input[type="range"] { flex: 1; min-width: 0; }
.wp-number {
  width: 48px;
  padding: 3px 4px;
  border: 1px solid var(--border-default);
  border-radius: var(--radius-sm);
  background: var(--surface-page);
  color: var(--text-primary);
  font-size: 11px;
}

.wp-check {
  display: flex;
  align-items: center;
  gap: 6px;
  color: var(--text-secondary);
  font-size: 11.5px;
  cursor: pointer;
}

.wp-check input { margin: 0; accent-color: var(--color-brand); }

.wp-link {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 4px 0;
  border: none;
  background: transparent;
  color: var(--text-secondary);
  font-size: 11.5px;
  cursor: pointer;
  text-align: left;
}

.wp-link:hover:not(:disabled) { color: var(--color-brand); }
.wp-link:disabled { opacity: 0.45; cursor: default; }

.wp-coord {
  bottom: 14px;
  left: 50%;
  transform: translateX(-50%);
  min-width: 280px;
  max-width: calc(100% - 300px);
  padding: 6px 8px;
}

.wp-coord-main {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 6px;
}

.wp-coord-text {
  font-variant-numeric: tabular-nums;
  font-size: 12.5px;
  color: var(--text-primary);
  white-space: nowrap;
}

.wp-copy { width: 22px; height: 22px; }

.wp-block {
  padding: 2px 4px 0;
  text-align: center;
  font-size: 11px;
  color: var(--text-tertiary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.wp-btn {
  pointer-events: auto;
  margin-top: 8px;
  padding: 5px 12px;
  border: 1px solid var(--border-strong);
  border-radius: var(--radius-sm);
  background: var(--surface-card);
  color: var(--text-secondary);
  font-size: 12px;
  cursor: pointer;
}

@media (max-width: 820px) {
  .wp-config { width: 208px; }
  .wp-coord { max-width: calc(100% - 250px); }
}
</style>
