<script setup lang="ts">
// 离线皮肤设置卡片 + 皮肤种类选择 dialog
import { onMounted } from "vue";
import { Save, RefreshCw, Pencil, RotateCcw } from "@lucide/vue";
import AlertDialog from "./AlertDialog.vue";
import {
  skinOptions, skinType, skinPlayerName, skinCustomPath, skinCustomModel,
  skinModelDialog, skinError, currentSkinDesc,
  onSkinTypeChange, saveOfflineSkin, savePlayerSkin, refreshPlayerSkin,
  pickCustomSkin, resetCustomSkin, pickModel, loadOfflineSkin,
} from "../store/offlineSkinConfig";

onMounted(loadOfflineSkin);
</script>

<template>
  <div class="stg-card">
    <div class="stg-card-hdr">
      <h3>离线皮肤</h3>
      <div class="stg-card-sub">离线模式使用的皮肤<span class="stg-warn">仅保证于 1.19.2 版本以下可用</span></div>
    </div>
    <div class="skin-opts">
      <label
        v-for="opt in skinOptions"
        :key="opt.value"
        class="skin-opt"
        :class="{ selected: skinType === opt.value }"
      >
        <input type="radio" :value="opt.value" v-model="skinType" @change="onSkinTypeChange" />
        <span class="skin-opt-label">{{ opt.label }}</span>
      </label>
    </div>
    <!-- 选中后显示详细信息 -->
    <div class="skin-detail">
      <div class="skin-desc">{{ currentSkinDesc }}</div>
      <div v-if="skinType === 'player'" class="skin-player-row">
        <input
          v-model="skinPlayerName"
          class="skin-input"
          type="text"
          placeholder="正版玩家用户名"
          maxlength="16"
          @change="saveOfflineSkin"
        />
        <button class="tool-btn" @click="savePlayerSkin">
          <Save :size="14" />
          保存皮肤
        </button>
        <button class="tool-btn" @click="refreshPlayerSkin">
          <RefreshCw :size="14" />
          刷新
        </button>
      </div>
      <div v-if="skinType === 'custom'" class="skin-sub-row">
        <template v-if="skinCustomPath">
          <button class="tool-btn" @click="pickCustomSkin">
            <Pencil :size="14" />
            更改皮肤
          </button>
          <button class="tool-btn" @click="resetCustomSkin">
            <RotateCcw :size="14" />
            重置皮肤
          </button>
          <span class="skin-model-note">模型：{{ skinCustomModel === 'alex' ? 'Alex（纤细）' : skinCustomModel === 'auto' ? '启动器决定' : 'Steve（经典）' }}</span>
        </template>
        <span v-else class="skin-desc">点击上方「自定义」打开文件选择器</span>
      </div>
    </div>
    <div v-if="skinError" class="err">{{ skinError }}</div>
  </div>

  <!-- 选择皮肤种类 dialog -->
  <AlertDialog v-model="skinModelDialog" title="选择皮肤种类" :divider="false">
    <div class="model-dialog-body">
      <div class="model-row">
        <span class="model-name">Steve（经典模型）</span>
        手臂较粗（4px），经典 Steve 风格，大多数皮肤适用
      </div>
      <div class="model-row">
        <span class="model-name">Alex（纤细模型）</span>
        手臂较细（3px），纤细 Alex 风格，slim 皮肤专用
      </div>
    </div>
    <template #actions>
      <button class="dialog-btn" @click="pickModel('steve')">Steve</button>
      <button class="dialog-btn" @click="pickModel('alex')">Alex</button>
      <button class="dialog-btn primary" @click="pickModel('auto')">启动器决定</button>
    </template>
  </AlertDialog>
</template>

<style>
@import "../styles/settings-shared.css";
</style>

<style scoped>
/* 横向 chip 选项：关键文字 + 圆形选择器 */
.skin-opts {
  display: flex;
  gap: 8px;
}

.skin-opt {
  flex: none;
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 6px;
  padding: 8px 14px;
  border: 1px solid var(--border-default);
  border-radius: 999px;
  cursor: pointer;
  white-space: nowrap;
  transition: all 0.15s;
}

.skin-opt:hover {
  border-color: var(--color-brand);
}

.skin-opt.selected {
  border-color: var(--color-brand);
  background: var(--color-brand-bg);
}

.skin-opt input[type="radio"] {
  accent-color: var(--color-brand);
  width: 14px;
  height: 14px;
  cursor: pointer;
  flex: none;
  margin: 0;
}

.skin-opt-label {
  font-size: 13px;
  font-weight: 500;
  color: var(--text-primary);
}

.skin-opt.selected .skin-opt-label {
  font-weight: 600;
  color: var(--color-brand);
}

/* 选中后的详细信息 */
.skin-detail {
  margin-top: 14px;
}

.skin-desc {
  font-size: 12px;
  color: var(--text-tertiary);
  line-height: 1.6;
}

.skin-sub-row {
  display: flex;
  align-items: center;
  gap: 10px;
  margin-top: 14px;
  flex-wrap: wrap;
}

.skin-input {
  flex: 1;
  min-width: 180px;
  padding: 8px 12px;
  border: 1px solid var(--border-strong);
  border-radius: var(--radius-md);
  background: var(--surface-page);
  color: var(--text-primary);
  font-size: 13px;
  outline: none;
  transition: border-color 0.15s;
}

.skin-input:focus {
  border-color: var(--color-brand);
}

.skin-model-note {
  font-size: 12px;
  color: var(--text-tertiary);
}

.skin-player-row {
  display: flex;
  align-items: center;
  gap: 10px;
  margin-top: 14px;
}

.skin-player-row .skin-input {
  flex: 1;
  min-width: 180px;
  margin: 0;
}

/* 模型选择 dialog */
.model-dialog-body {
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.model-row {
  font-size: 13px;
  line-height: 1.6;
  color: var(--text-secondary);
}

.model-name {
  display: block;
  font-weight: 600;
  color: var(--text-primary);
}

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
</style>
