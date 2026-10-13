<script setup lang="ts">
// 启动选项 + 内存分配卡片
import { onMounted } from "vue";
import { memState } from "../../store/app/memory";
import DropSelect, { type SelOption } from "../common/DropSelect.vue";
import AutoTasksEditor from "./AutoTasksEditor.vue";
import CheckToggle from "../common/CheckToggle.vue";
import {
  loWindowTitle, loPriority, loWinMode, loWinW, loWinH, loGameLang,
  loMemMode, loMemMb, loMinMem, loMemOptimize, loVerifySsl, launchError,
  memUsedMb, memGameMb, memUsedPct, memGamePct,
  saveLaunchOptions, loadLaunchOptions,
} from "../../store/config/launchOptions";
import { memLabelLeft, fmtGb } from "../../utils/format";

onMounted(loadLaunchOptions);

/** 最小内存开关：null = 不设置（JVM 默认），开启取 512MB。 */
function toggleMinMem() {
  loMinMem.value = loMinMem.value === null ? Math.min(512, loMemMb.value) : null;
  saveLaunchOptions();
}

const priorityOptions: SelOption[] = [
  { value: "Low", label: "低" },
  { value: "Normal", label: "中（平衡）" },
  { value: "High", label: "高" },
  { value: "RealTime", label: "实时" },
];

const winsizeOptions: SelOption[] = [
  { value: "Default", label: "默认（不设置）" },
  { value: "Custom", label: "自定义" },
];
</script>

<template>
  <div class="stg-card">
    <div class="stg-card-hdr">
      <h3>启动选项</h3>
      <div class="stg-card-sub">游戏启动行为设置</div>
    </div>
    <div class="launch-opts">
      <div class="lo-row">
        <label class="lo-label" for="lo-title">游戏窗口标题</label>
        <input
          id="lo-title"
          v-model="loWindowTitle"
          class="lo-input"
          type="text"
          placeholder="不修改（默认）"
          maxlength="64"
          @change="saveLaunchOptions"
          autocomplete="off"
        />
      </div>

      <div class="lo-row">
        <label class="lo-label" for="lo-prio">进程优先级</label>
        <DropSelect
          id="lo-prio"
          :model-value="loPriority"
          :options="priorityOptions"
          width="140px"
          @update:model-value="loPriority = $event as typeof loPriority; saveLaunchOptions()"
        />
      </div>

      <div class="lo-row">
        <label class="lo-label" for="lo-winsize">窗口大小</label>
        <div class="lo-winsize">
          <DropSelect
            id="lo-winsize"
            :model-value="loWinMode"
            :options="winsizeOptions"
            width="140px"
            @update:model-value="loWinMode = $event as typeof loWinMode; saveLaunchOptions()"
          />
          <Transition name="fade">
            <div v-if="loWinMode === 'Custom'" class="lo-win-custom">
              <input
                v-model.number="loWinW"
                class="lo-input lo-num"
                type="number"
                min="320"
                max="7680"
                placeholder="宽"
                @change="saveLaunchOptions"
                autocomplete="off"
              />
              <span class="lo-times">×</span>
              <input
                v-model.number="loWinH"
                class="lo-input lo-num"
                type="number"
                min="240"
                max="4320"
                placeholder="高"
                @change="saveLaunchOptions"
                autocomplete="off"
              />
            </div>
          </Transition>
        </div>
      </div>

      <div class="lo-row">
        <label class="lo-label" for="lo-lang">游戏语言</label>
        <DropSelect
          id="lo-lang"
          :model-value="loGameLang"
          :options="[{ value: 'auto_zh_CN', label: '自动设置为中文' }]"
          width="150px"
          @update:model-value="loGameLang = $event; saveLaunchOptions()"
        />
      </div>

      <div class="lo-row">
        <CheckToggle v-model="loVerifySsl" @change="saveLaunchOptions">在正版登录时验证 SSL 证书</CheckToggle>
      </div>
    </div>
    <div v-if="launchError" class="err">{{ launchError }}</div>
  </div>

  <div class="stg-card">
    <div class="stg-card-hdr">
      <h3>内存分配</h3>
      <div class="stg-card-sub">游戏内存分配与系统内存状态</div>
    </div>
    <div class="launch-opts">
      <div class="lo-row">
        <CheckToggle type="radio" value="Auto" v-model="loMemMode" @change="saveLaunchOptions">自动配置</CheckToggle>
      </div>
      <div class="lo-row">
        <CheckToggle type="radio" value="Custom" v-model="loMemMode" @change="saveLaunchOptions">自定义</CheckToggle>
        <input
          v-model.number="loMemMb"
          class="mem-slider"
          :class="{ disabled: loMemMode !== 'Custom' }"
          type="range"
          :min="1024"
          :max="memState.availMb || 4096"
          step="256"
          :disabled="loMemMode !== 'Custom'"
          :style="{ '--range-pct': ((loMemMb - 1024) / ((memState.availMb || 4096) - 1024) * 100) + '%' }"
          @change="saveLaunchOptions"
          autocomplete="off"
        />
        <span class="mem-slider-val" :class="{ disabled: loMemMode !== 'Custom' }">{{ fmtGb(loMemMb) }}</span>
      </div>
      <div class="lo-row">
        <CheckToggle :model-value="loMinMem !== null" :disabled="loMemMode !== 'Custom'" :dim="loMemMode !== 'Custom'" @change="toggleMinMem">最小内存（-Xms）</CheckToggle>
        <input
          v-if="loMinMem !== null"
          v-model.number="loMinMem"
          class="mem-slider"
          type="range"
          :min="256"
          :max="loMemMb"
          step="128"
          :disabled="loMemMode !== 'Custom'"
          :style="{ '--range-pct': ((loMinMem - 256) / (loMemMb - 256) * 100) + '%' }"
          @change="saveLaunchOptions"
          autocomplete="off"
        />
        <span v-if="loMinMem !== null" class="mem-slider-val" :class="{ disabled: loMemMode !== 'Custom' }">{{ loMinMem }} MB</span>
      </div>
      <div class="lo-row">
        <CheckToggle v-model="loMemOptimize" @change="saveLaunchOptions">启动游戏前进行内存优化</CheckToggle>
      </div>
      <div class="mem-status">
        <div class="mem-bar-labels">
          <span class="mem-bar-label">已使用 {{ fmtGb(memUsedMb) }}</span>
          <span v-if="memGamePct > 0" class="mem-bar-label" :style="{ left: memLabelLeft(memUsedPct) }">游戏分配 {{ fmtGb(memGameMb) }}</span>
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

  <AutoTasksEditor mode="global" />
</template>

<style>
@import "../../styles/settings-shared.css";
</style>

<style scoped>
/* ===== 内存分配 ===== */
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
  /* 文本起始紧贴色条左侧 */
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
</style>
