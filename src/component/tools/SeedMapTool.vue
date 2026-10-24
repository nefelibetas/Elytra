<script setup lang="ts">
// 种子预览：输入种子/版本 → cubiomes 群系瓦片 + 结构候选；版本支持正式版参数族。
import { computed, onMounted, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import { Map as MapIcon, ChevronDown } from "@lucide/vue";
import {
  findSeedMapSlimeChunks,
  findSeedMapStructures,
  getSeedMapVersions,
  resolveSeedMapVersion,
  type ResolvedVersion,
  type SeedMapSlimeChunk,
  type SeedMapStructure,
  type SeedMapVersion,
} from "../../api/seedmap";
import SeedMapCanvas from "./SeedMapCanvas.vue";
import DropSelect from "../common/DropSelect.vue";
import EmptyState from "../common/EmptyState.vue";
import { showToast } from "../../utils/toast";
import { errMsg } from "../../utils/error";
import { biomeLabel } from "../../utils/blockNames";

const { t } = useI18n();

interface SeedMapHover {
  x: number;
  z: number;
  biome: string;
}

interface FoundStructure extends SeedMapStructure {
  distance: number;
}

interface FoundSlime extends SeedMapSlimeChunk {
  distance: number;
}

const versions = ref<SeedMapVersion[]>([]);
const seedInput = ref("");
const versionInput = ref("26.3");
const appliedSeed = ref("");
const appliedVersion = ref("");
const resolved = ref<ResolvedVersion | null>(null);
const hover = ref<SeedMapHover | null>(null);
const loading = ref(false);
const canvasRef = ref<InstanceType<typeof SeedMapCanvas> | null>(null);
const viewCenter = ref({ x: 0, z: 0 });

const findMode = ref("structure");
const structureKind = ref("village");
const findRadius = ref("5000");
const findResults = ref<FoundStructure[]>([]);
const slimeResults = ref<FoundSlime[]>([]);
const findBusy = ref(false);
const showSlime = ref(false);
const resultsCollapsed = ref(false);

const FIND_STRUCTURES = [
  "village",
  "outpost",
  "monument",
  "mansion",
  "desert_pyramid",
  "jungle_temple",
  "swamp_hut",
  "igloo",
  "ocean_ruin",
  "shipwreck",
  "ruined_portal",
  "ancient_city",
  "trail_ruins",
  "trial_chambers",
  "abandoned_camp",
];

const findModeOptions = computed(() => [
  { value: "structure", label: t("tools.seedmap.structureMode") },
  { value: "slime", label: t("tools.seedmap.slimeMode") },
]);

const structureOptions = computed(() =>
  FIND_STRUCTURES.map((kind) => ({
    value: kind,
    label: t(`tools.seedmap.structures.${kind}`),
  })),
);

const radiusOptions = [
  { value: "1000", label: "1k" },
  { value: "5000", label: "5k" },
  { value: "20000", label: "20k" },
];

const versionOptions = computed(() =>
  (versions.value.length ? versions.value : [{ id: "26.3", label: "26.3" }]).map((v) => ({
    value: v.id,
    label: v.label,
  })),
);

const LEGEND: Array<{ kind: string; key: string }> = [
  { kind: "village", key: "village" },
  { kind: "outpost", key: "outpost" },
  { kind: "monument", key: "monument" },
  { kind: "mansion", key: "mansion" },
  { kind: "ancient_city", key: "ancient_city" },
  { kind: "trial_chambers", key: "trial_chambers" },
  { kind: "ruined_portal", key: "ruined_portal" },
  { kind: "treasure", key: "treasure" },
];

const resolvedText = computed(() => {
  const r = resolved.value;
  if (!r || r.exact) return "";
  return t("tools.seedmap.approx", { version: r.version, family: r.family });
});

const resultCount = computed(() => findResults.value.length || slimeResults.value.length);

const coordText = computed(() => {
  const h = hover.value;
  return h ? `X: ${h.x}, Z: ${h.z}` : "X: —, Z: —";
});

const biomeText = computed(() => {
  const h = hover.value;
  if (!h) return t("tools.seedmap.hoverHint");
  return h.biome ? biomeLabel(h.biome) : t("tools.seedmap.unknownBiome");
});

onMounted(async () => {
  try {
    versions.value = await getSeedMapVersions();
    if (!versions.value.some((v) => v.id === versionInput.value)) {
      versionInput.value = versions.value[versions.value.length - 1]?.id ?? "26.3";
    }
  } catch (e) {
    showToast(errMsg(e));
  }
});

async function openMap() {
  const seed = seedInput.value.trim();
  if (!seed) {
    showToast(t("tools.seedmap.seedRequired"));
    return;
  }
  try {
    const r = await resolveSeedMapVersion(versionInput.value);
    resolved.value = r;
    appliedSeed.value = seed;
    appliedVersion.value = r.family;
    hover.value = null;
    findResults.value = [];
    slimeResults.value = [];
    showSlime.value = findMode.value === "slime";
    viewCenter.value = { x: 0, z: 0 };
  } catch (e) {
    showToast(errMsg(e));
  }
}

function distanceTo(x: number, z: number): number {
  const c = viewCenter.value;
  return Math.round(Math.hypot(x - c.x, z - c.z));
}

async function doFind() {
  if (!appliedSeed.value) return;
  findBusy.value = true;
  try {
    const center = viewCenter.value;
    findResults.value = [];
    slimeResults.value = [];
    resultsCollapsed.value = false;
    if (findMode.value === "structure") {
      const list = await findSeedMapStructures(
        appliedSeed.value,
        appliedVersion.value,
        structureKind.value,
        center.x,
        center.z,
        Number(findRadius.value),
      );
      findResults.value = list.map((s) => ({ ...s, distance: distanceTo(s.x, s.z) }));
      if (list.length === 0) {
        showToast(t("tools.seedmap.noResults"));
      } else {
        canvasRef.value?.focusOn(list[0].x, list[0].z);
      }
    } else {
      const list = await findSeedMapSlimeChunks(
        appliedSeed.value,
        appliedVersion.value,
        center.x,
        center.z,
        Number(findRadius.value),
      );
      slimeResults.value = list.map((c) => ({
        ...c,
        distance: distanceTo(c.x * 16 + 8, c.z * 16 + 8),
      }));
      if (list.length === 0) {
        showToast(t("tools.seedmap.noResults"));
      } else {
        showSlime.value = true;
        const first = list[0];
        canvasRef.value?.focusOn(first.x * 16 + 8, first.z * 16 + 8);
      }
    }
  } catch (e) {
    showToast(errMsg(e));
  } finally {
    findBusy.value = false;
  }
}

function focusStructure(s: SeedMapStructure) {
  canvasRef.value?.focusOn(s.x, s.z);
}

function focusSlime(c: SeedMapSlimeChunk) {
  canvasRef.value?.focusOn(c.x * 16 + 8, c.z * 16 + 8);
}

function clearFind() {
  findResults.value = [];
  slimeResults.value = [];
  showSlime.value = findMode.value === "slime";
}

watch(findMode, (mode) => {
  findResults.value = [];
  slimeResults.value = [];
  resultsCollapsed.value = false;
  showSlime.value = mode === "slime";
});

function onError(message: string) {
  showToast(message);
}
</script>

<template>
  <div class="smt">
    <div class="smt-bar">
      <div class="smt-field smt-seed-field">
        <span class="smt-label">{{ t("tools.seedmap.seedLabel") }}</span>
        <input
          v-model="seedInput"
          class="smt-input"
          :placeholder="t('tools.seedmap.seedPlaceholder')"
          spellcheck="false"
          @keyup.enter="openMap"
        />
      </div>
      <div class="smt-field smt-version-field">
        <span class="smt-label">{{ t("tools.seedmap.versionLabel") }}</span>
        <DropSelect v-model="versionInput" :options="versionOptions" width="100%" />
      </div>
      <button type="button" class="smt-open" :disabled="!seedInput.trim()" @click="openMap">
        {{ t("tools.seedmap.view") }}
      </button>
    </div>

    <div v-if="resolvedText" class="smt-meta">
      <span class="smt-resolved approx">{{ resolvedText }}</span>
    </div>

    <div class="smt-find">
      <div class="smt-field smt-find-mode">
        <span class="smt-label">{{ t("tools.seedmap.typeLabel") }}</span>
        <DropSelect v-model="findMode" :options="findModeOptions" width="112px" />
      </div>
      <div v-if="findMode === 'structure'" class="smt-field smt-find-kind">
        <span class="smt-label">{{ t("tools.seedmap.structureLabel") }}</span>
        <DropSelect v-model="structureKind" :options="structureOptions" width="150px" />
      </div>
      <div class="smt-field smt-find-radius">
        <span class="smt-label">{{ t("tools.seedmap.radiusLabel") }}</span>
        <DropSelect v-model="findRadius" :options="radiusOptions" width="84px" />
      </div>
      <button
        type="button"
        class="smt-find-btn"
        :disabled="!appliedSeed || findBusy"
        @click="doFind"
      >
        {{ findBusy ? t("tools.seedmap.finding") : t("tools.seedmap.findAction") }}
      </button>
      <button type="button" class="smt-reset" :disabled="!resultCount" @click="clearFind">
        {{ t("tools.seedmap.reset") }}
      </button>
    </div>

    <div class="smt-map">
      <SeedMapCanvas
        v-if="appliedSeed"
        ref="canvasRef"
        :key="`${appliedSeed}|${appliedVersion}`"
        :seed="appliedSeed"
        :version="appliedVersion"
        :show-structures="true"
        :show-slime="showSlime"
        :initial-scale="0.25"
        @hover="hover = $event"
        @loading="loading = $event"
        @camera="viewCenter = $event"
        @error="onError"
      />
      <EmptyState v-else>
        <MapIcon :size="44" :stroke-width="1.2" opacity="0.35" />
        <p>{{ t("tools.seedmap.empty") }}</p>
        <p class="smt-empty-sub">{{ t("tools.seedmap.emptySub") }}</p>
      </EmptyState>

      <div v-if="loading" class="smt-map-loading">{{ t("tools.seedmap.loading") }}</div>

      <div v-if="resultCount" class="smt-results-panel">
        <div class="smt-results-head">
          <button
            type="button"
            class="smt-results-toggle"
            :aria-expanded="!resultsCollapsed"
            @click="resultsCollapsed = !resultsCollapsed"
          >
            <ChevronDown
              :size="13"
              class="smt-results-chevron"
              :class="{ flipped: resultsCollapsed }"
            />
            <span>{{ t("tools.seedmap.results", { n: resultCount }) }}</span>
          </button>
          <button type="button" class="smt-results-close" @click="clearFind">×</button>
        </div>
        <Transition name="expand">
          <div
            v-if="!resultsCollapsed"
            class="smt-results-list"
            style="--expand-h: min(420px, 52vh)"
          >
            <template v-if="findResults.length">
              <button
                v-for="s in findResults"
                :key="`s-${s.kind}-${s.x}-${s.z}`"
                type="button"
                class="smt-result"
                @click="focusStructure(s)"
              >
                <span class="smt-result-kind">{{ t(`tools.seedmap.structures.${s.kind}`) }}</span>
                <span>({{ s.x }}, {{ s.z }})</span>
                <span class="smt-result-dist">{{ t("tools.seedmap.blocksAway", { n: s.distance }) }}</span>
              </button>
            </template>
            <template v-else>
              <button
                v-for="c in slimeResults"
                :key="`c-${c.x}-${c.z}`"
                type="button"
                class="smt-result"
                @click="focusSlime(c)"
              >
                <span class="smt-result-kind">{{ t("tools.seedmap.chunkLabel") }} ({{ c.x }}, {{ c.z }})</span>
                <span class="smt-result-dist">{{ t("tools.seedmap.blocksAway", { n: c.distance }) }}</span>
              </button>
            </template>
          </div>
        </Transition>
      </div>
    </div>

    <div class="smt-foot">
      <span class="smt-coords">{{ coordText }}</span>
      <span class="smt-biome">{{ biomeText }}</span>
      <span class="smt-legend">
        <span v-for="item in LEGEND" :key="item.kind" class="smt-legend-item">
          <i class="smt-dot" :class="`dot-${item.kind}`"></i>
          {{ t(`tools.seedmap.structures.${item.key}`) }}
        </span>
      </span>
    </div>
  </div>
</template>

<style scoped>
.smt {
  display: flex;
  flex-direction: column;
  gap: 8px;
  flex: 1;
  min-height: 0;
}

.smt-bar {
  display: flex;
  align-items: flex-end;
  gap: 8px;
  flex-wrap: wrap;
}

.smt-field {
  display: flex;
  flex-direction: column;
  gap: 4px;
  min-width: 0;
}

.smt-seed-field {
  flex: 1;
  min-width: 220px;
}

.smt-version-field {
  width: 150px;
  flex: none;
}

.smt-label {
  padding-left: 2px;
  font-size: 11px;
  line-height: 1;
  color: var(--text-tertiary);
}

.smt-input {
  box-sizing: border-box;
  width: 100%;
  height: 32px;
  padding: 0 10px;
  border: 1px solid var(--border-default);
  border-radius: var(--radius-sm);
  background: var(--surface-input);
  color: var(--text-primary);
  font-size: 13px;
  outline: none;
}

.smt-input:focus {
  border-color: var(--color-brand);
}

.smt-open {
  flex: none;
  height: 32px;
  padding: 0 16px;
  border: 1px solid var(--border-default);
  border-radius: var(--radius-sm);
  background: var(--color-brand-bg);
  color: var(--color-brand);
  font-size: 13px;
  cursor: pointer;
}

.smt-open:hover {
  border-color: var(--color-brand);
}

.smt-open:disabled {
  opacity: 0.45;
  cursor: default;
}

.smt-meta {
  display: flex;
  align-items: center;
  gap: 12px;
  min-height: 20px;
  font-size: 12px;
  color: var(--text-tertiary);
}

.smt-find {
  display: flex;
  align-items: flex-end;
  gap: 8px;
  flex-wrap: wrap;
}

.smt-find-btn {
  height: 32px;
  padding: 0 14px;
  border: 1px solid var(--border-default);
  border-radius: var(--radius-sm);
  background: var(--surface-elevated);
  color: var(--text-secondary);
  font-size: 12px;
  cursor: pointer;
}

.smt-find-btn:hover:not(:disabled) {
  border-color: var(--color-brand);
  color: var(--color-brand);
}

.smt-find-btn:disabled {
  opacity: 0.45;
  cursor: default;
}

.smt-reset {
  height: 32px;
  padding: 0 12px;
  border: 1px solid transparent;
  border-radius: var(--radius-sm);
  background: var(--color-warning-bg);
  color: var(--color-warning);
  font-size: 12px;
  font-weight: 600;
  cursor: pointer;
}

.smt-reset:hover:not(:disabled) {
  border-color: var(--color-warning);
}

.smt-reset:disabled {
  opacity: 0.4;
  cursor: default;
}

.smt-map-loading {
  position: absolute;
  right: 8px;
  bottom: 8px;
  z-index: 3;
  padding: 3px 8px;
  border: 1px solid var(--border-default);
  border-radius: var(--radius-sm);
  background: var(--surface-card);
  color: var(--color-brand);
  font-size: 11px;
  pointer-events: none;
}

.smt-results-panel {
  position: absolute;
  top: 8px;
  right: 8px;
  z-index: 3;
  display: flex;
  flex-direction: column;
  width: 280px;
  max-width: calc(100% - 16px);
  max-height: calc(100% - 16px);
  overflow: hidden;
  border: 1px solid var(--border-default);
  border-radius: var(--radius-md);
  background: var(--surface-card);
  box-shadow: var(--shadow-md, 0 8px 24px rgb(0 0 0 / 0.18));
}

.smt-results-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  padding: 4px 6px 4px 8px;
  border-bottom: 1px solid var(--border-default);
  color: var(--text-tertiary);
  font-size: 11px;
}

.smt-results-toggle {
  display: flex;
  align-items: center;
  gap: 6px;
  flex: 1;
  min-width: 0;
  padding: 3px 0;
  border: none;
  background: transparent;
  color: inherit;
  font: inherit;
  text-align: left;
  cursor: pointer;
}

.smt-results-toggle:hover {
  color: var(--text-primary);
}

.smt-results-chevron {
  flex: none;
  transition: transform 0.18s ease;
}

.smt-results-chevron.flipped {
  transform: rotate(-90deg);
}

.smt-results-close {
  width: 20px;
  height: 20px;
  padding: 0;
  border: none;
  border-radius: var(--radius-sm);
  background: transparent;
  color: var(--text-tertiary);
  font-size: 15px;
  line-height: 1;
  cursor: pointer;
}

.smt-results-close:hover {
  background: var(--surface-hover);
  color: var(--text-primary);
}

.smt-results-list {
  display: flex;
  flex-direction: column;
  gap: 2px;
  max-height: min(420px, 52vh);
  padding: 4px;
  overflow-y: auto;
}

.smt-result {
  display: flex;
  align-items: center;
  gap: 6px;
  width: 100%;
  padding: 5px 6px;
  border: none;
  border-radius: var(--radius-sm);
  background: transparent;
  color: var(--text-secondary);
  font-size: 11px;
  text-align: left;
  cursor: pointer;
}

.smt-result:hover {
  background: var(--surface-hover);
  color: var(--color-brand);
}

.smt-result-kind {
  color: var(--text-primary);
}

.smt-result-dist {
  margin-left: auto;
  color: var(--text-tertiary);
}

.smt-resolved {
  color: var(--text-secondary);
}

.smt-resolved.approx {
  color: var(--color-warning);
}

.smt-map {
  position: relative;
  display: flex;
  flex: 1;
  min-height: 260px;
  min-width: 0;
  overflow: hidden;
  border: 1px solid var(--border-default);
  border-radius: var(--radius-md);
  background: var(--surface-page);
}

.smt-empty-sub {
  font-size: 11px;
  color: var(--text-tertiary);
}

.smt-foot {
  display: flex;
  align-items: center;
  gap: 12px;
  flex-wrap: wrap;
  min-height: 18px;
  font-size: 11px;
  color: var(--text-tertiary);
}

.smt-coords {
  min-width: 100px;
  font-variant-numeric: tabular-nums;
}

.smt-biome {
  color: var(--text-secondary);
}

.smt-legend {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  flex-wrap: wrap;
}

.smt-legend-item {
  display: inline-flex;
  align-items: center;
  gap: 4px;
}

.smt-dot {
  display: inline-block;
  width: 7px;
  height: 7px;
  border-radius: 50%;
  background: var(--text-tertiary);
}

.dot-village { background: #f2c14e; }
.dot-outpost { background: #db7b6b; }
.dot-monument { background: #59b6a5; }
.dot-mansion { background: #b28bd6; }
.dot-ancient_city { background: #6d7cff; }
.dot-trial_chambers { background: #e879f9; }
.dot-ruined_portal { background: #c084fc; }
.dot-treasure { background: #f59e0b; }

</style>
