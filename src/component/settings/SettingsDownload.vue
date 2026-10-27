<script setup lang="ts">
// 下载设置卡片：下载源主开关 + 细分折叠 + 网络代理 + 线程数；下拉统一用共享 DropSelect。
import { computed, onMounted } from "vue";
import { useI18n } from "vue-i18n";
import { ChevronDown } from "@lucide/vue";
import { openUrl } from "@tauri-apps/plugin-opener";
import DropSelect, { type SelOption } from "../common/DropSelect.vue";
import CheckToggle from "../common/CheckToggle.vue";
import { fmtSpeed } from "../../utils/format";
import { errMsg } from "../../utils/error";
import { showToast } from "../../utils/toast";
import {
  dlGameMode, dlCommunityMode, dlConcurrency, dlMaxSpeed,
  dlJdkMode, dlNetworkMode, dlTranslateMode,
  dlError, dlShowReleaseOnly,
  dlCurseforgeKeySource, dlCurseforgeKeyInput, dlCurseforgeKeySaving,
  saveCurseforgeKey, clearCurseforgeKey,
  sourceModeOptions, communitySourceOptions,
  jdkSourceOptions,
  networkModeOptions,
  translateDisplayOptions,
  concurrencyLocked,
  saveDownloadConfig, setGameMode, setCommunityMode, setJdkMode, setNetworkMode,
  setTranslateMode, loadDownloadConfig,
  dlSourceMaster, dlDetailOpen, setSourceMaster,
  type OptionKeys,
} from "../../store/config/downloaderConfig";

const { t } = useI18n();

/** key 列表 → 渲染用文案（locale 切换时 computed 自动重算）。 */
function localized(options: OptionKeys[]): SelOption[] {
  return options.map(o => ({ value: o.value, label: t(o.labelKey), desc: t(o.descKey) }));
}

const sourceOptions = computed(() => localized(sourceModeOptions));
const communityOptions = computed(() => localized(communitySourceOptions));
const jdkOptions = computed(() => localized(jdkSourceOptions));
const networkOptions = computed(() => localized(networkModeOptions));
const translateOptions = computed(() => localized(translateDisplayOptions));

onMounted(loadDownloadConfig);
/** 主开关 custom 态文案（custom 值不在选项中，经 placeholder 呈现）。 */
const masterPlaceholder = computed(() => t("settings.download.masterCustom"));

// 界面单位 KB/s，配置存储 bytes/s；上限 10 MB/s。
const maxSpeedKb = computed({
  get: () => Math.round(dlMaxSpeed.value / 1024),
  set: (value: number) => {
    dlMaxSpeed.value = Math.max(0, value) * 1024;
  },
});

const curseforgeKeyPlaceholder = computed(() => {
  switch (dlCurseforgeKeySource.value) {
    case "user":
      return t("settings.download.curseforgeKey.placeholderUser");
    case "builtIn":
      return t("settings.download.curseforgeKey.placeholderBuiltIn");
    default:
      return t("settings.download.curseforgeKey.placeholderNone");
  }
});

function onMaster(v: string) {
  setSourceMaster(v as Parameters<typeof setSourceMaster>[0]);
}

async function saveCurseforgeKeyFromInput() {
  try {
    await saveCurseforgeKey(dlCurseforgeKeyInput.value);
    showToast(t("settings.download.curseforgeKey.saved"));
  } catch (e) {
    showToast(errMsg(e));
  }
}

async function clearCurseforgeKeyFromUi() {
  try {
    await clearCurseforgeKey();
    showToast(t("settings.download.curseforgeKey.cleared"));
  } catch (e) {
    showToast(errMsg(e));
  }
}

function openCurseforgeConsole() {
  void openUrl("https://console.curseforge.com/");
}
</script>

<template>
  <div class="stg-card">
    <div class="stg-card-hdr">
      <h3>{{ t("settings.download.title") }}</h3>
      <div class="stg-card-sub">{{ t("settings.download.subtitle") }}</div>
    </div>

    <!-- 下载源（主开关）：切换三细分；明细区可展开单独调整 -->
    <div class="dl-row">
      <span class="dl-label">{{ t("settings.download.source") }}</span>
      <div class="dl-field">
        <DropSelect
          :model-value="dlSourceMaster"
          :options="sourceOptions"
          :placeholder="masterPlaceholder"
          @update:model-value="onMaster"
        />
      </div>
      <button
        class="dl-detail-toggle"
        :class="{ open: dlDetailOpen }"
        v-tooltip="t('settings.download.adjustDetail')"
        @click="dlDetailOpen = !dlDetailOpen"
      >
        <ChevronDown :size="14" />
      </button>
    </div>

    <!-- 细分源设置：三类各自独立 -->
    <Transition name="expand">
      <div v-if="dlDetailOpen" class="dl-source-detail">
        <!-- 游戏下载源：版本列表/json/资源/库/ModLoader/JDK runtime 统一走此源 -->
        <div class="dl-row">
          <span class="dl-label">{{ t("settings.download.gameSource") }}</span>
          <div class="dl-field">
            <DropSelect
              :model-value="dlGameMode"
              :options="sourceOptions"
              @update:model-value="setGameMode"
            />
          </div>
        </div>

        <div class="dl-row">
          <span class="dl-label">{{ t("settings.download.communitySource") }}</span>
          <div class="dl-field">
            <DropSelect
              :model-value="dlCommunityMode"
              :options="communityOptions"
              @update:model-value="setCommunityMode"
            />
          </div>
        </div>

        <!-- JDK 下载源：auto 测速取最快；officialOnly 固定 Mojang→Azul；mirrorOnly 固定 Mojang(镜像)→Adoptium 仅 TUNA -->
        <div class="dl-row">
          <span class="dl-label">{{ t("settings.download.jdkSource") }}</span>
          <div class="dl-field">
            <DropSelect
              :model-value="dlJdkMode"
              :options="jdkOptions"
              @update:model-value="setJdkMode"
            />
          </div>
        </div>
      </div>
    </Transition>

    <!-- CurseForge Key：独立行，自定义 key 优先于构建期内置 key -->
    <div class="dl-row dl-row-key">
      <span class="dl-label">{{ t("settings.download.curseforgeKey.label") }}</span>
      <div class="dl-field dl-key-field">
        <input
          v-model="dlCurseforgeKeyInput"
          class="dl-key-input"
          type="password"
          autocomplete="off"
          spellcheck="false"
          :placeholder="curseforgeKeyPlaceholder"
          @keydown.enter="saveCurseforgeKeyFromInput"
        />
        <button
          type="button"
          class="dl-key-btn"
          :disabled="dlCurseforgeKeySaving || !dlCurseforgeKeyInput.trim()"
          @click="saveCurseforgeKeyFromInput"
        >
          {{ t("settings.download.curseforgeKey.save") }}
        </button>
        <button
          v-if="dlCurseforgeKeySource === 'user'"
          type="button"
          class="dl-key-btn ghost"
          :disabled="dlCurseforgeKeySaving"
          @click="clearCurseforgeKeyFromUi"
        >
          {{ t("settings.download.curseforgeKey.clear") }}
        </button>
      </div>
    </div>
    <div class="dl-key-hint">
      <span>{{ t("settings.download.curseforgeKey.hint") }}</span>
      <button type="button" class="dl-link" @click="openCurseforgeConsole">
        {{ t("settings.download.curseforgeKey.getKey") }}
      </button>
    </div>

    <!-- 网络代理：自动 / 强制系统代理 / 直连（TUN 由网络层接管） -->
    <div class="dl-row">
      <span class="dl-label">{{ t("settings.download.network") }}</span>
      <div class="dl-field">
        <DropSelect
          :model-value="dlNetworkMode"
          :options="networkOptions"
          @update:model-value="setNetworkMode"
        />
      </div>
    </div>

    <!-- 翻译显示 -->
    <div class="dl-row">
      <span class="dl-label">{{ t("settings.download.translate") }}</span>
      <div class="dl-field">
        <DropSelect
          :model-value="dlTranslateMode"
          :options="translateOptions"
          @update:model-value="(v: string) => setTranslateMode(v)"
        />
      </div>
    </div>

    <div class="dl-row">
      <span class="dl-label">{{ t("settings.download.releaseOnlyLabel") }}</span>
      <div class="dl-field">
        <CheckToggle v-model="dlShowReleaseOnly" @change="saveDownloadConfig">{{ t("settings.download.releaseOnly") }}</CheckToggle>
      </div>
    </div>

    <!-- 最大线程数：官方源并发；镜像模式固定 16，隐藏该行 -->
    <div v-if="!concurrencyLocked" class="dl-row">
      <span class="dl-label">{{ t("settings.download.threads") }}</span>
      <input
        class="dl-range"
        type="range"
        min="1"
        max="256"
        step="1"
        v-model.number="dlConcurrency"
        :style="{ '--range-pct': (dlConcurrency / 256 * 100) + '%' }"
        @input="saveDownloadConfig"
        autocomplete="off"
      />
      <span class="dl-val">{{ dlConcurrency }} / 256</span>
    </div>
    <!-- 下载限速：0 = 不限制 -->
    <div class="dl-row">
      <span class="dl-label">{{ t("settings.download.speedLimit") }}</span>
      <input
        class="dl-range"
        type="range"
        min="0"
        max="10240"
        step="256"
        v-model.number="maxSpeedKb"
        :style="{ '--range-pct': (maxSpeedKb / 10240 * 100) + '%' }"
        @input="saveDownloadConfig"
        autocomplete="off"
      />
      <span class="dl-val">{{ dlMaxSpeed === 0 ? t('settings.download.unlimited') : fmtSpeed(dlMaxSpeed) }}</span>
    </div>
    <!-- Auto 模式：官方源优先+镜像降级；并发仅作用官方源，镜像固定 16 -->
    <p v-if="dlGameMode === 'auto' && !concurrencyLocked" class="dl-note">
      {{ t("settings.download.autoNote") }}
    </p>

    <div v-if="dlError" class="dl-error">{{ dlError }}</div>
  </div>
</template>

<style>
@import "../../styles/settings-shared.css";
</style>

<style scoped>
.dl-row {
  display: flex;
  align-items: center;
  gap: 14px;
  padding: 9px 0;
}

.dl-field {
  flex: 1;
  min-width: 0;
}

.dl-detail-toggle {
  flex: 0 0 auto;
  width: 28px;
  height: 28px;
  display: flex;
  align-items: center;
  justify-content: center;
  color: var(--text-tertiary);
  background: transparent;
  border: 1px solid var(--border-default);
  border-radius: var(--radius-sm);
  cursor: pointer;
  transition: all 0.15s;
}
.dl-detail-toggle:hover {
  color: var(--text-primary);
  border-color: var(--color-brand);
}
.dl-detail-toggle svg {
  transition: transform 0.2s ease;
}
.dl-detail-toggle.open svg {
  transform: rotate(180deg);
}

.dl-source-detail {
  margin-left: 12px;
  padding-left: 14px;
  border-left: 1px solid var(--border-default);
  /* expand 目标高度（共享骨架见 styles/transitions.css） */
  --expand-h: 1000px;
}
.dl-source-detail .dl-row {
  padding: 6px 0;
}
.dl-source-detail .dl-label {
  flex-basis: 84px;
  font-size: 12.5px;
  color: var(--text-secondary);
}

.dl-label {
  flex: 0 0 112px;
  font-size: 13.5px;
  font-weight: 600;
  color: var(--text-primary);
}

.dl-row-key .dl-label {
  white-space: nowrap;
}

.dl-range {
  flex: 1;
  min-width: 0;
}
.dl-val {
  flex: 0 0 auto;
  min-width: 86px;
  font-size: 12px;
  color: var(--text-secondary);
  text-align: right;
  font-variant-numeric: tabular-nums;
  white-space: nowrap;
}

.dl-key-field {
  display: flex;
  align-items: center;
  gap: 8px;
}

.dl-key-input {
  flex: 1;
  min-width: 0;
  height: 30px;
  padding: 0 9px;
  font-size: 12px;
  color: var(--text-primary);
  background: var(--surface-input, var(--surface-card));
  border: 1px solid var(--border-default);
  border-radius: var(--radius-sm);
  outline: none;
}
.dl-key-input:focus {
  border-color: var(--color-brand);
}

.dl-key-btn {
  flex: 0 0 auto;
  height: 30px;
  padding: 0 10px;
  font-size: 12px;
  color: var(--text-primary);
  background: var(--surface-hover);
  border: 1px solid var(--border-default);
  border-radius: var(--radius-sm);
  cursor: pointer;
}
.dl-key-btn:hover:not(:disabled) {
  border-color: var(--color-brand);
}
.dl-key-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}
.dl-key-btn.ghost {
  color: var(--text-secondary);
  background: transparent;
}

.dl-key-hint {
  display: flex;
  align-items: center;
  gap: 8px;
  margin: -2px 0 4px 126px;
  font-size: 10px;
  line-height: 1.4;
  color: var(--text-secondary);
}

.dl-link {
  padding: 0;
  font-size: inherit;
  color: var(--color-brand);
  background: none;
  border: none;
  cursor: pointer;
}
.dl-link:hover {
  text-decoration: underline;
}

.dl-error {
  margin-top: 8px;
  font-size: 12px;
  color: var(--color-danger);
}

.dl-note {
  margin: -4px 0 4px 126px;
  font-size: 11px;
  color: var(--text-tertiary);
  line-height: 1.5;
}
</style>