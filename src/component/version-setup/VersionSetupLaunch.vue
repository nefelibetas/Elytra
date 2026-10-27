<script setup lang="ts">
// 版本设置-启动选项：实例级启动参数快照（内存/Java/JVM/窗口等），修改即保存。
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import { useRoute } from "vue-router";
import { useI18n } from "vue-i18n";
import { RefreshCw, Undo2 } from "@lucide/vue";
import DropSelect, { type SelOption } from "../common/DropSelect.vue";
import CheckToggle from "../common/CheckToggle.vue";
import LoadingSpinner from "../common/LoadingSpinner.vue";
import { memState } from "../../store/app/memory";
import { fmtGb, memLabelLeft } from "../../utils/format";
import { jdkList, loadConfig, refreshJdks } from "../../store/config/jdkOptions";
import { gcOptions } from "../../store/config/launchOptions";
import {
  getInstanceLaunchOptions,
  saveInstanceLaunchOptions,
} from "../../store/task/versionSetup";
import { showToast } from "../../utils/toast";
import { errMsg } from "../../utils/error";
import type { InstanceLaunchOptions } from "../../types/launcher/InstanceLaunchOptions";
import type { InstanceLaunchState } from "../../types/launcher/InstanceLaunchState";
import type { GcType } from "../../types/launcher/GcType";
import type { ProcessPriority } from "../../types/launcher/ProcessPriority";

const route = useRoute();
const { t } = useI18n();
// 组件卸载时 route.query 可能先清空；固定本次编辑的实例 id，避免延迟保存写到空目录。
const versionId = String(route.query.name ?? "");

const AUTO_VALUE = "__auto__";

const loading = ref(true);
const error = ref("");
const state = ref<InstanceLaunchState | null>(null);
const draft = ref<InstanceLaunchOptions | null>(null);
const jvmText = ref("");
const gameText = ref("");
const saving = ref(false);
const saved = ref(false);
const refreshing = ref(false);

let saveQueue: Promise<void> = Promise.resolve();
let saveTimer: number | undefined;
let savedTimer: number | undefined;
let pending = false;

function clone<T>(value: T): T {
  return JSON.parse(JSON.stringify(value)) as T;
}

function splitLines(text: string): string[] {
  return text.split(/\n/).map(line => line.trim()).filter(Boolean);
}

function setDraft(instance: InstanceLaunchOptions | null) {
  draft.value = instance ? clone(instance) : null;
  jvmText.value = instance?.jvm_args.join("\n") ?? "";
  gameText.value = instance?.game_args.join("\n") ?? "";
}

async function load() {
  if (!versionId) {
    loading.value = false;
    return;
  }
  loading.value = true;
  error.value = "";
  try {
    state.value = await getInstanceLaunchOptions(versionId);
    setDraft(state.value.instance);
    await loadConfig();
  } catch (e) {
    error.value = errMsg(e);
  } finally {
    loading.value = false;
  }
}

onMounted(load);
onBeforeUnmount(() => {
  window.clearTimeout(saveTimer);
  window.clearTimeout(savedTimer);
  if (pending) void saveNow();
});

/** 所有写入串行排队，避免快速切换字段时旧值后落盘。 */
function enqueueSave(options: InstanceLaunchOptions | null): Promise<void> {
  if (!versionId) return saveQueue;
  const next = saveQueue.catch(() => undefined).then(() =>
    saveInstanceLaunchOptions(versionId, options),
  );
  saveQueue = next;
  return next;
}

function scheduleSave() {
  if (!draft.value) return;
  pending = true;
  window.clearTimeout(saveTimer);
  saveTimer = window.setTimeout(() => void saveNow(), 150);
}

async function saveNow() {
  if (!draft.value) return;
  pending = false;
  draft.value.jvm_args = splitLines(jvmText.value);
  draft.value.game_args = splitLines(gameText.value);
  const snapshot = clone(draft.value);
  saving.value = true;
  try {
    await enqueueSave(snapshot);
    if (state.value) {
      state.value.instance = snapshot;
      state.value.effective = clone(snapshot);
    }
    saved.value = true;
    window.clearTimeout(savedTimer);
    savedTimer = window.setTimeout(() => (saved.value = false), 1500);
  } catch (e) {
    showToast(errMsg(e));
  } finally {
    saving.value = false;
    if (pending) void saveNow();
  }
}

function customize() {
  if (!state.value) return;
  setDraft(state.value.global);
  scheduleSave();
}

async function reset() {
  if (!state.value) return;
  pending = false;
  window.clearTimeout(saveTimer);
  saving.value = true;
  try {
    await enqueueSave(null);
    state.value.instance = null;
    state.value.effective = clone(state.value.global);
    setDraft(null);
  } catch (e) {
    showToast(errMsg(e));
  } finally {
    saving.value = false;
  }
}

async function refreshJava() {
  refreshing.value = true;
  try {
    await refreshJdks();
  } finally {
    refreshing.value = false;
  }
}

// ── Java ──

const javaOptions = computed<SelOption[]>(() => {
  const options: SelOption[] = [
    { value: AUTO_VALUE, label: t("settings.jdk.autoLabel"), desc: t("settings.jdk.autoDesc") },
    ...jdkList.value.map(jdk => ({
      value: jdk.path,
      label: jdk.version,
      desc: t("settings.jdk.optionDesc", { vendor: jdk.vendor, bits: jdk.bits, path: jdk.path }),
    })),
  ];
  const manual = draft.value?.java;
  if (manual?.mode === "manual" && !jdkList.value.some(jdk => jdk.path === manual.path)) {
    options.push({ value: manual.path, label: manual.path, desc: "" });
  }
  return options;
});

const javaValue = computed(() =>
  draft.value?.java.mode === "manual" ? draft.value.java.path : AUTO_VALUE,
);

function pickJava(value: string) {
  if (!draft.value) return;
  draft.value.java = value === AUTO_VALUE
    ? { mode: "auto" }
    : { mode: "manual", path: value };
  scheduleSave();
}

// ── 内存 ──

const memMode = computed<"Auto" | "Custom">({
  get: () => (draft.value?.memory === "Auto" ? "Auto" : "Custom"),
  set: (mode) => {
    if (!draft.value) return;
    draft.value.memory = mode === "Auto"
      ? "Auto"
      : { Custom: memMb.value };
  },
});

const memMb = computed<number>({
  get: () => (draft.value && draft.value.memory !== "Auto" ? draft.value.memory.Custom : 4096),
  set: (value) => {
    if (draft.value && draft.value.memory !== "Auto") {
      draft.value.memory = { Custom: value };
    }
  },
});

const minMem = computed<number | null>({
  get: () => draft.value?.min_memory ?? null,
  set: (value) => {
    if (draft.value) draft.value.min_memory = value;
  },
});

function toggleMinMem() {
  minMem.value = minMem.value === null ? Math.min(512, memMb.value) : null;
  scheduleSave();
}

const memUsedMb = computed(() => Math.max(0, memState.totalMb - memState.availMb));
const memGameMb = computed(() => {
  const configured = draft.value?.memory;
  const requested = configured && configured !== "Auto"
    ? configured.Custom
    : Math.max(2048, Math.floor(memState.availMb * 0.60));
  return Math.min(requested, memState.availMb || Number.MAX_SAFE_INTEGER);
});
const memUsedPct = computed(() =>
  memState.totalMb ? (memUsedMb.value / memState.totalMb) * 100 : 0);
const memGamePct = computed(() => {
  const remain = Math.max(0, memState.totalMb - memUsedMb.value);
  const game = Math.min(memGameMb.value, remain);
  return memState.totalMb ? (game / memState.totalMb) * 100 : 0;
});

// ── 窗口 / 下拉 ──

const priorityOptions = computed<SelOption[]>(() => [
  { value: "Low", label: t("settings.launch.priorityLow") },
  { value: "Normal", label: t("settings.launch.priorityNormal") },
  { value: "High", label: t("settings.launch.priorityHigh") },
  { value: "RealTime", label: t("settings.launch.priorityRealtime") },
]);

const winsizeOptions = computed<SelOption[]>(() => [
  { value: "Default", label: t("settings.launch.windowDefault") },
  { value: "Custom", label: t("settings.launch.windowCustom") },
]);

const gameLangOptions = computed<SelOption[]>(() => [
  { value: "auto_zh_CN", label: t("settings.launch.gameLanguageAuto") },
]);

const gcSelectOptions = computed<SelOption[]>(() =>
  gcOptions.map(option => ({ value: option.value, label: t(option.labelKey) })),
);

const winMode = computed<"Default" | "Custom">({
  get: () => (draft.value?.window_size === "Default" ? "Default" : "Custom"),
  set: (mode) => {
    if (!draft.value) return;
    draft.value.window_size = mode === "Default"
      ? "Default"
      : { Custom: [winW.value, winH.value] };
  },
});

const winW = computed<number>({
  get: () => (draft.value && draft.value.window_size !== "Default"
    ? draft.value.window_size.Custom[0]
    : 1280),
  set: (value) => {
    if (draft.value && draft.value.window_size !== "Default") {
      draft.value.window_size = { Custom: [value, winH.value] };
    }
  },
});

const winH = computed<number>({
  get: () => (draft.value && draft.value.window_size !== "Default"
    ? draft.value.window_size.Custom[1]
    : 720),
  set: (value) => {
    if (draft.value && draft.value.window_size !== "Default") {
      draft.value.window_size = { Custom: [winW.value, value] };
    }
  },
});

function setPriority(value: string) {
  if (!draft.value) return;
  draft.value.process_priority = value as ProcessPriority;
  scheduleSave();
}

function setGc(value: string) {
  if (!draft.value) return;
  draft.value.gc_type = value as GcType;
  scheduleSave();
}

function setWindowMode(value: string) {
  winMode.value = value as "Default" | "Custom";
  scheduleSave();
}
</script>

<template>
  <div class="vs-page">
    <div v-if="loading" class="stg-card">
      <LoadingSpinner :text="t('settings.version.setup.launch.loading')" />
    </div>

    <div v-else-if="error" class="stg-card err">{{ error }}</div>

    <template v-else-if="state">
      <div class="stg-card">
        <div class="stg-card-hdr">
          <h3>{{ t("settings.version.setup.launch.title") }}</h3>
          <div class="stg-card-sub">
            {{ draft ? t("settings.version.setup.launch.customized") : t("settings.version.setup.launch.followGlobalDesc") }}
          </div>
        </div>

        <div v-if="!draft" class="inst-follow">
          <span>{{ t("settings.version.setup.launch.followGlobal") }}</span>
          <button class="tool-btn" @click="customize">
            {{ t("settings.version.setup.launch.customize") }}
          </button>
        </div>

        <template v-else>
          <div class="inst-toolbar">
            <span class="inst-status">
              <span v-if="saving">{{ t("settings.version.setup.launch.saving") }}</span>
              <span v-else-if="saved" class="inst-saved">{{ t("settings.version.setup.launch.saved") }}</span>
            </span>
            <button class="tool-btn" @click="reset">
              <Undo2 :size="14" />
              {{ t("settings.version.setup.launch.reset") }}
            </button>
          </div>
          <p class="inst-note">{{ t("settings.version.setup.launch.note") }}</p>
        </template>
      </div>

      <template v-if="draft">
        <div class="stg-card">
          <div class="stg-card-hdr">
            <h3>{{ t("settings.version.setup.launch.javaTitle") }}</h3>
            <div class="stg-card-sub">{{ t("settings.version.setup.launch.javaDesc") }}</div>
          </div>
          <div class="lo-row">
            <div class="inst-java-select">
              <DropSelect
                :model-value="javaValue"
                :options="javaOptions"
                width="100%"
                @update:model-value="pickJava"
              />
            </div>
            <button class="tool-btn" :disabled="refreshing" @click="refreshJava">
              <RefreshCw :size="14" />
              {{ t("settings.jdk.refresh") }}
            </button>
          </div>
        </div>

        <div class="stg-card">
          <div class="stg-card-hdr">
            <h3>{{ t("settings.version.setup.launch.gameTitle") }}</h3>
            <div class="stg-card-sub">{{ t("settings.launch.subtitle") }}</div>
          </div>
          <div class="launch-opts">
            <div class="lo-row">
              <label class="lo-label" for="inst-title">{{ t("settings.launch.windowTitle") }}</label>
              <input
                id="inst-title"
                v-model="draft.window_title"
                class="lo-input"
                type="text"
                :placeholder="t('settings.launch.windowTitlePlaceholder')"
                maxlength="64"
                autocomplete="off"
                @change="scheduleSave"
              />
            </div>

            <div class="lo-row">
              <label class="lo-label" for="inst-prio">{{ t("settings.launch.priority") }}</label>
              <DropSelect
                id="inst-prio"
                :model-value="draft.process_priority"
                :options="priorityOptions"
                width="140px"
                @update:model-value="setPriority"
              />
            </div>

            <div class="lo-row">
              <label class="lo-label" for="inst-winsize">{{ t("settings.launch.windowSize") }}</label>
              <div class="lo-winsize">
                <DropSelect
                  id="inst-winsize"
                  :model-value="winMode"
                  :options="winsizeOptions"
                  width="140px"
                  @update:model-value="setWindowMode"
                />
                <Transition name="fade">
                  <div v-if="winMode === 'Custom'" class="lo-win-custom">
                    <input v-model.number="winW" class="lo-input lo-num" type="number" min="320" max="7680" autocomplete="off" @change="scheduleSave" />
                    <span class="lo-times">×</span>
                    <input v-model.number="winH" class="lo-input lo-num" type="number" min="240" max="4320" autocomplete="off" @change="scheduleSave" />
                  </div>
                </Transition>
              </div>
            </div>

            <div class="lo-row">
              <label class="lo-label" for="inst-lang">{{ t("settings.launch.gameLanguage") }}</label>
              <DropSelect
                id="inst-lang"
                :model-value="draft.game_language"
                :options="gameLangOptions"
                width="150px"
                @update:model-value="draft.game_language = $event; scheduleSave()"
              />
            </div>

            <div class="lo-row">
              <CheckToggle v-model="draft.verify_ssl" @change="scheduleSave">
                {{ t("settings.launch.verifySsl") }}
              </CheckToggle>
            </div>
          </div>
        </div>

        <div class="stg-card">
          <div class="stg-card-hdr">
            <h3>{{ t("settings.launch.memoryTitle") }}</h3>
            <div class="stg-card-sub">{{ t("settings.launch.memorySubtitle") }}</div>
          </div>
          <div class="launch-opts">
            <div class="lo-row">
              <CheckToggle v-model="memMode" type="radio" value="Auto" @change="scheduleSave">
                {{ t("settings.launch.memoryAuto") }}
              </CheckToggle>
            </div>
            <div class="lo-row">
              <CheckToggle v-model="memMode" type="radio" value="Custom" @change="scheduleSave">
                {{ t("settings.launch.memoryCustom") }}
              </CheckToggle>
              <input
                v-model.number="memMb"
                class="mem-slider"
                :class="{ disabled: memMode !== 'Custom' }"
                type="range"
                :min="1024"
                :max="memState.availMb || 4096"
                step="256"
                :disabled="memMode !== 'Custom'"
                :style="{ '--range-pct': ((memMb - 1024) / ((memState.availMb || 4096) - 1024) * 100) + '%' }"
                autocomplete="off"
                @change="scheduleSave"
              />
              <span class="mem-slider-val" :class="{ disabled: memMode !== 'Custom' }">{{ fmtGb(memMb) }}</span>
            </div>
            <div class="lo-row">
              <CheckToggle
                :model-value="minMem !== null"
                :disabled="memMode !== 'Custom'"
                :dim="memMode !== 'Custom'"
                @change="toggleMinMem"
              >
                {{ t("settings.launch.minMemory") }}
              </CheckToggle>
              <input
                v-if="minMem !== null"
                v-model.number="minMem"
                class="mem-slider"
                type="range"
                :min="256"
                :max="memMb"
                step="128"
                :disabled="memMode !== 'Custom'"
                :style="{ '--range-pct': ((minMem - 256) / (memMb - 256) * 100) + '%' }"
                autocomplete="off"
                @change="scheduleSave"
              />
              <span v-if="minMem !== null" class="mem-slider-val">{{ minMem }} MB</span>
            </div>
            <div class="lo-row">
              <CheckToggle v-model="draft.pre_launch_memory_optimize" @change="scheduleSave">
                {{ t("settings.launch.memoryOptimize") }}
              </CheckToggle>
            </div>
            <div class="mem-status">
              <div class="mem-bar-labels">
                <span class="mem-bar-label">{{ t("settings.launch.memoryUsed", { size: fmtGb(memUsedMb) }) }}</span>
                <span v-if="memGamePct > 0" class="mem-bar-label" :style="{ left: memLabelLeft(memUsedPct) }">{{ t("settings.launch.memoryGame", { size: fmtGb(memGameMb) }) }}</span>
              </div>
              <div class="mem-bar">
                <div class="mem-seg mem-used" :style="{ width: memUsedPct + '%' }"></div>
                <div class="mem-seg mem-game" :style="{ width: memGamePct + '%' }"></div>
                <div class="mem-seg mem-free"></div>
              </div>
              <div class="mem-total">{{ fmtGb(memUsedMb) }} / {{ fmtGb(memState.totalMb) }}</div>
            </div>
          </div>
        </div>

        <div class="stg-card">
          <div class="stg-card-hdr">
            <h3>{{ t("settings.version.setup.launch.advancedTitle") }}</h3>
            <div class="stg-card-sub">{{ t("settings.version.setup.launch.advancedDesc") }}</div>
          </div>
          <div class="launch-opts">
            <div class="lo-row lo-top">
              <label class="lo-label" for="inst-jvm">{{ t("settings.jdk.jvmArgs") }}</label>
              <textarea
                id="inst-jvm"
                v-model="jvmText"
                class="lo-textarea"
                rows="4"
                wrap="off"
                :placeholder="t('settings.jdk.jvmArgsPlaceholder')"
                spellcheck="false"
                autocomplete="off"
                @change="scheduleSave"
              ></textarea>
            </div>
            <div class="lo-row lo-top">
              <label class="lo-label" for="inst-game">{{ t("settings.jdk.gameArgs") }}</label>
              <textarea
                id="inst-game"
                v-model="gameText"
                class="lo-textarea"
                rows="2"
                wrap="off"
                :placeholder="t('settings.jdk.gameArgsPlaceholder')"
                spellcheck="false"
                autocomplete="off"
                @change="scheduleSave"
              ></textarea>
            </div>
            <div class="lo-row">
              <label class="lo-label" for="inst-gc">{{ t("settings.jdk.memoryManagement") }}</label>
              <DropSelect
                id="inst-gc"
                :model-value="draft.gc_type"
                :options="gcSelectOptions"
                width="180px"
                @update:model-value="setGc"
              />
            </div>
            <div class="lo-checks">
              <CheckToggle v-model="draft.disable_launch_wrapper" @change="scheduleSave">
                {{ t("settings.jdk.disableWrapper") }}
              </CheckToggle>
              <CheckToggle v-model="draft.disable_lwjgl_unsafe_agent" @change="scheduleSave">
                {{ t("settings.jdk.disableAgent") }}
              </CheckToggle>
              <CheckToggle v-model="draft.use_high_performance_gpu" @change="scheduleSave">
                {{ t("settings.jdk.highGpu") }}
              </CheckToggle>
            </div>
          </div>
        </div>
      </template>
    </template>
  </div>
</template>

<style>
@import "../../styles/settings-shared.css";
</style>

<style scoped>
.vs-page {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  overflow-x: hidden;
}

.inst-follow,
.inst-toolbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
}

.inst-follow {
  padding: 4px 0 2px;
  color: var(--text-secondary);
  font-size: 13px;
}

.inst-toolbar {
  min-height: 34px;
}

.inst-status {
  min-width: 0;
  font-size: 12px;
  color: var(--text-tertiary);
}

.inst-saved {
  color: var(--color-success);
}

.inst-note {
  margin: 8px 0 0;
  font-size: 12px;
  line-height: 1.6;
  color: var(--text-tertiary);
}

.mem-slider {
  flex: 1;
  cursor: pointer;
}

.mem-slider.disabled {
  opacity: 0.45;
  cursor: not-allowed;
}

.mem-slider-val {
  flex: none;
  width: 72px;
  font-size: 12.5px;
  color: var(--text-secondary);
  text-align: right;
}

.mem-slider-val.disabled {
  opacity: 0.45;
}

.mem-status {
  margin-top: 2px;
}

.mem-bar-labels {
  position: relative;
  height: 16px;
}

.mem-bar-label {
  position: absolute;
  font-size: 10.5px;
  color: var(--text-tertiary);
  white-space: nowrap;
}

.mem-bar {
  display: flex;
  height: 10px;
  border-radius: 5px;
  overflow: hidden;
  background: var(--border-default);
}

.mem-seg {
  height: 100%;
  transition: width 0.4s ease;
}

.mem-used {
  background: var(--color-danger);
}

.mem-game {
  background: var(--color-success);
}

.mem-free {
  flex: 1;
}

.mem-total {
  margin-top: 6px;
  font-size: 12px;
  color: var(--text-secondary);
}

.lo-win-custom {
  display: flex;
  align-items: center;
  gap: 6px;
}

.inst-java-select {
  flex: 1;
  min-width: 0;
}
</style>
