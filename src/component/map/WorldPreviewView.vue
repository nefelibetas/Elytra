<script setup lang="ts">
// 世界预览视图：真实 region 地图 + 维度/可调最大 Y 俯视 + 浮动面板。
// 全页与工具箱详情共用，路径由调用方解析后传入。
import { computed, nextTick, onMounted, reactive, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import {
  ChevronDown,
  ChevronRight,
  ChevronUp,
  Copy,
  Ellipsis,
  FolderOpen,
  Grid3X3,
  Layers,
  MapPin,
  Minus,
  Plus,
  RotateCcw,
  Search,
  SlidersHorizontal,
} from "@lucide/vue";
import {
  findMapBiome,
  findMapFolderBiome,
  findMapSlimeChunks,
  getMapFolderInfo,
  getMapWorldInfo,
  listMapBiomes,
  listMapFolderBiomes,
  type MapBiomeSummary,
  type MapWorldInfo,
} from "../../api/map";
import WorldMapCanvas from "./WorldMapCanvas.vue";
import DropSelect from "../common/DropSelect.vue";
import { ensureGameFolder } from "../../store/config/gameDirs";
import { openFolder } from "../../utils/fs";
import { showToast } from "../../utils/toast";
import { errMsg } from "../../utils/error";
import { biomeLabel } from "../../utils/blockNames";
import LoadingSpinner from "../common/LoadingSpinner.vue";
import EmptyState from "../common/EmptyState.vue";
import ErrorState from "../common/ErrorState.vue";

interface HoverInfo {
  x: number;
  z: number;
  biome: string | null;
  pending?: boolean;
}

const props = defineProps<{
  versionName: string;
  worldName: string;
  /** 非空时按任意世界根目录读取（工具箱详情），忽略 version/world */
  folder?: string;
}>();

const { t } = useI18n();

const versionName = computed(() => props.versionName);
const worldName = computed(() => props.worldName);
const folder = computed(() => props.folder ?? "");
const folderMode = computed(() => folder.value.length > 0);
const canvasRef = ref<InstanceType<typeof WorldMapCanvas> | null>(null);

const info = ref<MapWorldInfo | null>(null);
const loading = ref(true);
const loadError = ref("");
const mapState = ref<"loading" | "ready" | "empty" | "error">("loading");
const mapError = ref("");

const dimension = ref("");
const maxY = ref(319);
const showGrid = ref(false);
const chunkBorders = ref(false);
const modColors = ref(true);

const overworld = computed(() => dimension.value === "");
const findMode = ref("biome");
const biome = ref("");
const biomeList = ref<MapBiomeSummary[]>([]);
const biomeLoading = ref(false);
const findBusy = ref(false);
const findStatus = ref("");
const showSlime = ref(false);
let searchGen = 0;

const findModeOptions = computed(() => [
  { value: "biome", label: t("worldPreview.findBiome") },
  ...(overworld.value ? [{ value: "slime", label: t("worldPreview.findSlime") }] : []),
]);
const biomeOptions = computed(() =>
  biomeList.value.map((b) => ({ value: b.biome, label: biomeLabel(b.biome) })),
);

const hover = ref<HoverInfo | null>(null);

// v2：坐标面板改为默认收起，旧键不再复用
const panelKey = "elytra.worldPreview.panels.v2";
const panels = reactive({ dim: true, coord: false, config: false });

const active = computed(() => hover.value);
const coordText = computed(() => {
  const c = active.value;
  if (!c) return "X: —, Z: —";
  return `X: ${c.x}, Z: ${c.z}`;
});
const compactCoords = computed(() => {
  const c = active.value;
  return c ? `(${c.x}, ${c.z})` : t("worldPreview.coordinates");
});
/** 顶部维度行最多 3 个按钮，其余收入「更多」 */
const visibleDimensions = computed(() => info.value?.dimensions.slice(0, 3) ?? []);
const extraDimensions = computed(() => info.value?.dimensions.slice(3) ?? []);
const dimsMoreOpen = ref(false);

function selectDimension(id: string) {
  if (id !== dimension.value) searchGen++;
  dimension.value = id;
  dimsMoreOpen.value = false;
}

function collapseDimensions() {
  panels.dim = false;
  dimsMoreOpen.value = false;
}
const biomeText = computed(() => {
  const c = active.value;
  if (!c) return t("worldPreview.noBiome");
  if (c.pending) return t("worldPreview.loadingBiome");
  return c.biome ? biomeLabel(c.biome) : t("worldPreview.unknownBiome");
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
  const gen = ++searchGen;
  loading.value = true;
  loadError.value = "";
  try {
    if (!folderMode.value && (!versionName.value || !worldName.value)) {
      throw new Error(t("worldPreview.error"));
    }
    const data = folderMode.value
      ? await getMapFolderInfo(folder.value)
      : await getMapWorldInfo(versionName.value, worldName.value);
    if (gen !== searchGen) return;
    info.value = data;
    dimension.value = data.dimensions[0] ?? "";
    // 俯视默认从世界最高 Y 向下扫描。
    maxY.value = data.max_y;
    biomeList.value = [];
    biome.value = "";
    findStatus.value = "";
    biomeLoading.value = false;
    if (dimension.value !== "") {
      showSlime.value = false;
      if (findMode.value === "slime") findMode.value = "biome";
    } else {
      showSlime.value = findMode.value === "slime";
    }
    mapState.value = data.dimensions.length ? "loading" : "empty";
  } catch (e) {
    if (gen === searchGen) loadError.value = errMsg(e);
  } finally {
    if (gen === searchGen) loading.value = false;
  }
  if (panels.config && findMode.value === "biome") void ensureBiomes();
}

function onHover(value: HoverInfo | null) {
  hover.value = value;
}

function onState(value: "loading" | "ready" | "empty" | "error", detail?: string) {
  mapState.value = value;
  mapError.value = detail ?? "";
}

async function ensureBiomes() {
  if (!info.value || biomeLoading.value || biomeList.value.length || findMode.value !== "biome") {
    return;
  }
  const gen = searchGen;
  biomeLoading.value = true;
  try {
    const list = folderMode.value
      ? await listMapFolderBiomes(folder.value, dimension.value, info.value.max_y)
      : await listMapBiomes(
          versionName.value,
          worldName.value,
          dimension.value,
          info.value.max_y,
        );
    if (gen !== searchGen) return;
    biomeList.value = list;
    biome.value = list[0]?.biome ?? "";
    if (list.length === 0) findStatus.value = t("worldPreview.noBiomes");
  } catch (e) {
    if (gen === searchGen) showToast(errMsg(e));
  } finally {
    if (gen === searchGen) biomeLoading.value = false;
  }
}

async function doFind() {
  const data = info.value;
  if (!data || findBusy.value) return;
  const gen = searchGen;
  findBusy.value = true;
  findStatus.value = "";
  const center = canvasRef.value?.center() ?? { x: 0, z: 0 };
  try {
    if (findMode.value === "slime") {
      if (!data.seed) {
        showToast(t("worldPreview.seedMissing"));
        return;
      }
      showSlime.value = overworld.value;
      const list = await findMapSlimeChunks(
        data.seed,
        Math.round(center.x),
        Math.round(center.z),
        5000,
      );
      if (gen !== searchGen) return;
      if (list.length === 0) {
        findStatus.value = t("worldPreview.noFind");
        return;
      }
      const x = list[0].x * 16 + 8;
      const z = list[0].z * 16 + 8;
      canvasRef.value?.focusOn(x, z);
      findStatus.value = t("worldPreview.foundAt", { x, z });
      return;
    }
    if (!biome.value) {
      await ensureBiomes();
      if (!biome.value) return;
    }
    const hit = folderMode.value
      ? await findMapFolderBiome(
          folder.value,
          dimension.value,
          data.max_y,
          biome.value,
          Math.round(center.x),
          Math.round(center.z),
        )
      : await findMapBiome(
          versionName.value,
          worldName.value,
          dimension.value,
          data.max_y,
          biome.value,
          Math.round(center.x),
          Math.round(center.z),
        );
    if (gen !== searchGen) return;
    if (!hit) {
      findStatus.value = t("worldPreview.noFind");
      return;
    }
    canvasRef.value?.focusOn(hit.x, hit.z);
    findStatus.value = t("worldPreview.foundAt", { x: hit.x, z: hit.z });
  } catch (e) {
    if (gen === searchGen) showToast(errMsg(e));
  } finally {
    if (gen === searchGen) findBusy.value = false;
  }
}

watch(dimension, (id) => {
  biomeList.value = [];
  biome.value = "";
  biomeLoading.value = false;
  findBusy.value = false;
  findStatus.value = "";
  if (id !== "") {
    showSlime.value = false;
    if (findMode.value === "slime") findMode.value = "biome";
  }
  if (panels.config && findMode.value === "biome") void ensureBiomes();
});

watch(findMode, (mode) => {
  findStatus.value = "";
  showSlime.value = mode === "slime" && overworld.value;
  if (mode === "biome") void ensureBiomes();
});

watch(
  () => panels.config,
  (open) => {
    if (open) void ensureBiomes();
  },
);

function retry() {
  mapState.value = "loading";
  mapError.value = "";
  void canvasRef.value?.refresh();
}

watch(maxY, (value) => {
  if (!info.value) return;
  const next = Number.isFinite(value)
    ? Math.min(info.value.max_y, Math.max(info.value.min_y, Math.round(value)))
    : info.value.max_y;
  if (next !== value) maxY.value = next;
});

async function copyCoords() {
  const c = active.value;
  if (!c) return;
  try {
    await navigator.clipboard.writeText(`X: ${c.x}, Z: ${c.z}`);
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
    const path = folderMode.value
      ? folder.value
      : await ensureGameFolder("saves", versionName.value);
    await openFolder(path);
  } catch (e) {
    showToast(errMsg(e));
  }
}

async function reload() {
  await loadInfo();
  await nextTick();
  await canvasRef.value?.refresh();
}

onMounted(() => {
  // 全页画布不能被上一个页面的滚动位置推到标题栏下方；工具箱内嵌不接管滚动。
  if (!folderMode.value) {
    document.querySelector<HTMLElement>(".main-content")?.scrollTo({ top: 0 });
  }
});

watch(
  () => [props.versionName, props.worldName, props.folder],
  () => void loadInfo(),
  { immediate: true },
);

defineExpose({ reload });
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
        :root-path="folder"
        :dimension="dimension"
        :max-y="maxY"
        :show-grid="showGrid"
        :chunk-borders="chunkBorders"
        :mod-colors="modColors"
        :seed="info.seed ?? ''"
        :show-slime="showSlime"
        @hover="onHover"
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

      <!-- 顶部中间：维度（展开时不占标题行，直接按钮） -->
      <section class="wp-panel wp-top" :class="{ collapsed: !panels.dim }">
        <Transition name="panel-swap" mode="out-in">
          <button
            v-if="!panels.dim"
            key="collapsed"
            class="wp-panel-head"
            :aria-expanded="false"
            :aria-label="t('worldPreview.expand')"
            @click="panels.dim = true"
          >
            <Layers :size="14" />
            <span>{{ t("worldPreview.dimensions") }}</span>
            <ChevronDown :size="13" class="wp-head-chevron" />
          </button>
          <div v-else key="expanded" class="wp-dims-row">
            <button
              v-for="id in visibleDimensions"
              :key="id || 'overworld'"
              class="wp-dim"
              :class="{ active: dimension === id }"
              @click="selectDimension(id)"
            >
              {{ dimensionLabel(id) }}
            </button>
            <button
              v-if="extraDimensions.length"
              class="wp-dim wp-dim-more"
              :class="{ active: extraDimensions.includes(dimension) }"
              :aria-expanded="dimsMoreOpen"
              :aria-label="t('worldPreview.moreDimensions')"
              :title="t('worldPreview.moreDimensions')"
              @click="dimsMoreOpen = !dimsMoreOpen"
            >
              <Ellipsis :size="14" />
            </button>
            <button
              class="wp-icon-btn wp-panel-toggle"
              :aria-label="t('worldPreview.collapse')"
              @click="collapseDimensions"
            >
              <ChevronUp :size="13" />
            </button>
          </div>
        </Transition>
        <Transition name="expand">
          <div v-if="panels.dim && dimsMoreOpen && extraDimensions.length" class="wp-dims">
            <button
              v-for="id in extraDimensions"
              :key="id"
              class="wp-dim"
              :class="{ active: dimension === id }"
              @click="selectDimension(id)"
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

      <!-- 右上角：控件下方配置（展开时仅收起箭头） -->
      <section class="wp-panel wp-config" :class="{ collapsed: !panels.config }">
        <button class="wp-panel-head" :aria-expanded="panels.config" @click="panels.config = !panels.config">
          <SlidersHorizontal :size="14" />
          <span>{{ t("worldPreview.config") }}</span>
          <ChevronRight
            :size="13"
            class="wp-head-chevron wp-chevron"
            :class="{ flipped: !panels.config }"
          />
        </button>
        <Transition name="expand">
          <div v-if="panels.config" class="wp-config-body">
            <div class="wp-group">
              <div class="wp-group-title">{{ t("worldPreview.maxY") }}</div>
              <label class="wp-range">
                <span>Y</span>
                <input v-model.number="maxY" type="range" :min="info.min_y" :max="info.max_y" step="1" />
                <input v-model.number="maxY" class="wp-number" type="number" :min="info.min_y" :max="info.max_y" step="1" />
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
              <div class="wp-group-title">{{ t("worldPreview.find") }}</div>
              <DropSelect v-model="findMode" :options="findModeOptions" width="100%" />
              <DropSelect
                v-if="findMode === 'biome'"
                v-model="biome"
                :options="biomeOptions"
                :disabled="biomeLoading || !biomeOptions.length"
                :placeholder="
                  biomeLoading ? t('worldPreview.scanning') : t('worldPreview.noBiomes')
                "
                width="100%"
              />
              <button
                class="wp-link"
                :disabled="
                  findBusy ||
                  biomeLoading ||
                  (findMode === 'biome' && !biome) ||
                  (findMode === 'slime' && !info.seed)
                "
                @click="doFind"
              >
                <Search :size="13" />
                <span>{{ findBusy ? t("worldPreview.finding") : t("worldPreview.findAction") }}</span>
              </button>
              <span v-if="findStatus" class="wp-find-status">{{ findStatus }}</span>
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

      <!-- 底部中间：坐标 + 群系 -->
      <section class="wp-panel wp-coord" :class="{ collapsed: !panels.coord }">
        <div class="wp-coord-main">
          <MapPin v-if="!panels.coord" :size="14" class="wp-coord-lead" />
          <span class="wp-coord-text">{{ panels.coord ? coordText : compactCoords }}</span>
          <div class="wp-coord-actions">
            <button
              v-if="panels.coord"
              class="wp-icon-btn wp-copy"
              :title="t('common.action.copy')"
              :aria-label="t('common.action.copy')"
              @click="copyCoords"
            >
              <Copy :size="13" />
            </button>
            <button
              class="wp-coord-toggle"
              :aria-expanded="panels.coord"
              :aria-label="panels.coord ? t('worldPreview.collapse') : t('worldPreview.expand')"
              @click="panels.coord = !panels.coord"
            >
              <ChevronDown :size="13" class="wp-chevron" :class="{ flipped: !panels.coord }" />
            </button>
          </div>
        </div>
        <Transition name="expand">
          <div v-if="panels.coord" class="wp-biome" :title="active?.biome ?? ''">
            {{ biomeText }}
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

.wp-head-chevron { margin-left: auto; flex: none; }

.wp-chevron { flex: none; transition: transform 0.18s ease; }
.wp-chevron.flipped { transform: rotate(180deg); }

.wp-top {
  top: 12px;
  left: 50%;
  transform: translateX(-50%);
  max-width: min(520px, calc(100% - 220px));
}

.wp-dims-row {
  display: flex;
  align-items: center;
  gap: 4px;
  padding: 6px 6px 6px 8px;
}

.wp-dims {
  display: flex;
  flex-wrap: wrap;
  gap: 4px;
  padding: 0 8px 8px;
}

.wp-panel-toggle { width: 22px; height: 22px; margin-left: 2px; }

.wp-dim-more {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  padding: 5px 6px;
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
  flex-direction: row;
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
  top: 56px;
  right: 14px;
  width: 232px;
  max-height: calc(100% - 120px);
  display: flex;
  flex-direction: column;
  overflow: visible;
  background: var(--surface-card);
  backdrop-filter: none;
  transition: width 0.18s ease;
}

.wp-config.collapsed { width: 112px; }

.wp-config .wp-panel-head { justify-content: flex-start; }

.wp-config-body {
  display: flex;
  flex-direction: column;
  gap: 12px;
  flex: 1;
  min-height: 0;
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

.wp-find-status {
  font-size: 11px;
  color: var(--text-tertiary);
  word-break: break-all;
}

.wp-coord {
  bottom: 14px;
  left: 50%;
  transform: translateX(-50%);
  min-width: 280px;
  max-width: calc(100% - 300px);
  padding: 4px 8px;
}

.wp-coord.collapsed {
  min-width: 0;
  /* 与顶部/配置收起态一致的侧向内缩，避免图标贴边 */
  padding: 0 6px;
}

.wp-coord-main {
  position: relative;
  display: flex;
  align-items: center;
  justify-content: center;
  min-height: 30px;
  padding: 0 30px;
}

/* 收起态高度对齐其他面板（35px），图标各内缩 9px */
.wp-coord.collapsed .wp-coord-main {
  min-height: 33px;
  padding: 0 26px;
}

.wp-coord:not(.collapsed) .wp-coord-main { padding: 0 52px; }

.wp-coord-lead {
  position: absolute;
  left: 4px;
  top: 50%;
  transform: translateY(-50%);
  color: var(--text-secondary);
}

.wp-coord-actions {
  position: absolute;
  right: 4px;
  top: 50%;
  transform: translateY(-50%);
  display: flex;
  align-items: center;
  gap: 2px;
}

/* 字形边距与左侧图标一致（11px）；padding/margin 只放大点击区不改变字形位置 */
.wp-coord-toggle {
  box-sizing: content-box;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 13px;
  height: 13px;
  padding: 4px;
  margin: -4px;
  border: none;
  border-radius: var(--radius-sm);
  background: transparent;
  color: var(--text-secondary);
  cursor: pointer;
}

.wp-coord-toggle:hover {
  color: var(--text-primary);
  background: var(--surface-hover);
}

.wp-coord-text {
  font-variant-numeric: tabular-nums;
  font-size: 12.5px;
  color: var(--text-primary);
  white-space: nowrap;
  text-align: center;
}

.wp-copy { width: 22px; height: 22px; }

.wp-biome {
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

.panel-swap-enter-active,
.panel-swap-leave-active {
  transition: opacity 0.16s ease, transform 0.16s ease;
}

.panel-swap-enter-from,
.panel-swap-leave-to {
  opacity: 0;
  transform: translateY(-4px);
}

@media (max-width: 820px) {
  .wp-config { width: 208px; }
  .wp-coord { max-width: calc(100% - 250px); }
}
</style>
