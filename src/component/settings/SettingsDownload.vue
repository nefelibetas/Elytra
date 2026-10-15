<script setup lang="ts">
// 下载设置卡片：下载源主开关 + 细分折叠 + 网络代理 + 线程数；下拉统一用共享 DropSelect。
import { onMounted } from "vue";
import { ChevronDown } from "@lucide/vue";
import DropSelect from "../common/DropSelect.vue";
import CheckToggle from "../common/CheckToggle.vue";
import { fmtSpeed } from "../../utils/format";
import {
  dlGameMode, dlCommunityMode, dlConcurrency, dlMaxSpeed,
  dlJdkMode, dlNetworkMode, dlTranslateMode,
  dlError, dlShowReleaseOnly,
  sourceModeOptions, communitySourceOptions,
  jdkSourceOptions,
  networkModeOptions,
  translateDisplayOptions,
  concurrencyLocked,
  saveDownloadConfig, setGameMode, setCommunityMode, setJdkMode, setNetworkMode,
  setTranslateMode, loadDownloadConfig,
  dlSourceMaster, dlDetailOpen, setSourceMaster,
} from "../../store/config/downloaderConfig";

onMounted(loadDownloadConfig);
/** 主开关 custom 态文案（custom 值不在选项中，经 placeholder 呈现）。 */
const masterPlaceholder = "自定义（已单独设置）";

function onMaster(v: string) {
  setSourceMaster(v as Parameters<typeof setSourceMaster>[0]);
}
</script>

<template>
  <div class="stg-card">
    <div class="stg-card-hdr">
      <h3>管理游戏下载源</h3>
      <div class="stg-card-sub">游戏文件、版本列表、社区资源与 JDK 的下载来源与线程数</div>
    </div>

    <!-- 下载源（主开关）：切换三细分；明细区可展开单独调整 -->
    <div class="dl-row">
      <span class="dl-label">下载源</span>
      <div class="dl-field">
        <DropSelect
          :model-value="dlSourceMaster"
          :options="sourceModeOptions"
          :placeholder="masterPlaceholder"
          @update:model-value="onMaster"
        />
      </div>
      <button
        class="dl-detail-toggle"
        :class="{ open: dlDetailOpen }"
        v-tooltip="'单独调整各细分源'"
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
          <span class="dl-label">游戏下载源</span>
          <div class="dl-field">
            <DropSelect
              :model-value="dlGameMode"
              :options="sourceModeOptions"
              @update:model-value="setGameMode"
            />
          </div>
        </div>

        <div class="dl-row">
          <span class="dl-label">社区下载源</span>
          <div class="dl-field">
            <DropSelect
              :model-value="dlCommunityMode"
              :options="communitySourceOptions"
              @update:model-value="setCommunityMode"
            />
          </div>
        </div>

        <!-- JDK 下载源：auto 测速取最快；officialOnly 固定 Mojang→Azul；mirrorOnly 固定 Mojang(镜像)→Adoptium 仅 TUNA -->
        <div class="dl-row">
          <span class="dl-label">JDK 下载源</span>
          <div class="dl-field">
            <DropSelect
              :model-value="dlJdkMode"
              :options="jdkSourceOptions"
              @update:model-value="setJdkMode"
            />
          </div>
        </div>
      </div>
    </Transition>

    <!-- 网络代理：自动 / 强制系统代理 / 直连（TUN 由网络层接管） -->
    <div class="dl-row">
      <span class="dl-label">网络代理</span>
      <div class="dl-field">
        <DropSelect
          :model-value="dlNetworkMode"
          :options="networkModeOptions"
          @update:model-value="setNetworkMode"
        />
      </div>
    </div>

    <!-- 翻译显示 -->
    <div class="dl-row">
      <span class="dl-label">翻译显示</span>
      <div class="dl-field">
        <DropSelect
          :model-value="dlTranslateMode"
          :options="translateDisplayOptions"
          @update:model-value="(v: string) => setTranslateMode(v)"
        />
      </div>
    </div>

    <div class="dl-row">
      <CheckToggle v-model="dlShowReleaseOnly" @change="saveDownloadConfig">社区资源仅显示正式版（过滤快照/预发布版本）</CheckToggle>
    </div>

    <!-- 最大线程数：官方源并发；镜像模式固定 16，隐藏该行 -->
    <div v-if="!concurrencyLocked" class="dl-row">
      <span class="dl-label">最大线程数</span>
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
      <span class="dl-label">下载限速</span>
      <input
        class="dl-range"
        type="range"
        min="0"
        max="10240"
        step="256"
        v-model.number="dlMaxSpeed"
        :style="{ '--range-pct': (dlMaxSpeed / 10240 * 100) + '%' }"
        @input="saveDownloadConfig"
        autocomplete="off"
      />
      <span class="dl-val">{{ dlMaxSpeed === 0 ? '不限' : fmtSpeed(dlMaxSpeed) }}</span>
    </div>
    <!-- Auto 模式：官方源优先+镜像降级；并发仅作用官方源，镜像固定 16 -->
    <p v-if="dlGameMode === 'auto' && !concurrencyLocked" class="dl-note">
      该并发仅对官方源生效，镜像源固定 16 线程
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
  flex: 0 0 88px;
  font-size: 13.5px;
  font-weight: 600;
  color: var(--text-primary);
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

.dl-error {
  margin-top: 8px;
  font-size: 12px;
  color: var(--color-danger);
}

.dl-note {
  margin: -4px 0 4px 102px;
  font-size: 11px;
  color: var(--text-tertiary);
  line-height: 1.5;
}
</style>