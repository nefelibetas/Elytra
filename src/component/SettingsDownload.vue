<script setup lang="ts">
// 下载设置卡片（管理游戏下载源）：文件/版本列表/社区源 + 线程数 + 速度限制
import { onMounted } from "vue";
import {
  dlFileMode, dlManifestMode, dlCommunityMode, dlConcurrency, dlMaxSpeed,
  dlFileOpen, dlManifestOpen, dlCommunityOpen, dlError,
  sourceModeOptions, communitySourceOptions, sourceModeLabel,
  concurrencyLocked, MIRROR_CONCURRENCY,
  saveDownloadConfig, setFileMode, setCommunityMode, setManifestMode,
  loadDownloadConfig,
} from "../store/downloaderConfig";

onMounted(loadDownloadConfig);
</script>

<template>
  <div class="stg-card">
    <div class="stg-card-hdr">
      <h3>管理游戏下载源</h3>
      <div class="stg-card-sub">游戏文件与版本列表的下载来源、线程数与速度控制</div>
    </div>

    <!-- 文件下载源：点击展开选项 -->
    <div class="dl-row">
      <span class="dl-label">文件下载源</span>
      <div class="dl-sel" :class="{ open: dlFileOpen }">
        <button class="dl-sel-btn" @click="dlFileOpen = !dlFileOpen">
          <span>{{ sourceModeLabel(dlFileMode) }}</span>
          <svg viewBox="0 0 12 12" width="12" height="12" fill="currentColor">
            <polygon points="3,4 9,4 6,9" />
          </svg>
        </button>
        <div v-if="dlFileOpen" class="dl-sel-menu">
          <button
            v-for="opt in sourceModeOptions"
            :key="opt.value"
            class="dl-sel-item"
            :class="{ active: dlFileMode === opt.value }"
            @click="setFileMode(opt.value)"
          >
            <span class="dl-sel-item-label">{{ opt.label }}</span>
            <span class="dl-sel-item-desc">{{ opt.desc }}</span>
          </button>
        </div>
      </div>
    </div>

    <!-- 版本列表源 -->
    <div class="dl-row">
      <span class="dl-label">版本列表源</span>
      <div class="dl-sel" :class="{ open: dlManifestOpen }">
        <button class="dl-sel-btn" @click="dlManifestOpen = !dlManifestOpen">
          <span>{{ sourceModeLabel(dlManifestMode) }}</span>
          <svg viewBox="0 0 12 12" width="12" height="12" fill="currentColor">
            <polygon points="3,4 9,4 6,9" />
          </svg>
        </button>
        <div v-if="dlManifestOpen" class="dl-sel-menu">
          <button
            v-for="opt in sourceModeOptions"
            :key="opt.value"
            class="dl-sel-item"
            :class="{ active: dlManifestMode === opt.value }"
            @click="setManifestMode(opt.value)"
          >
            <span class="dl-sel-item-label">{{ opt.label }}</span>
            <span class="dl-sel-item-desc">{{ opt.desc }}</span>
          </button>
        </div>
      </div>
    </div>

    <!-- 社区下载源：Modrinth 官方 vs MCIM 镜像（管理 Fabric API 等社区资源） -->
    <div class="dl-row">
      <span class="dl-label">社区下载源</span>
      <div class="dl-sel" :class="{ open: dlCommunityOpen }">
        <button class="dl-sel-btn" @click="dlCommunityOpen = !dlCommunityOpen">
          <span>{{ communitySourceOptions.find(o => o.value === dlCommunityMode)?.label ?? dlCommunityMode }}</span>
          <svg viewBox="0 0 12 12" width="12" height="12" fill="currentColor">
            <polygon points="3,4 9,4 6,9" />
          </svg>
        </button>
        <div v-if="dlCommunityOpen" class="dl-sel-menu">
          <button
            v-for="opt in communitySourceOptions"
            :key="opt.value"
            class="dl-sel-item"
            :class="{ active: dlCommunityMode === opt.value }"
            @click="setCommunityMode(opt.value)"
          >
            <span class="dl-sel-item-label">{{ opt.label }}</span>
            <span class="dl-sel-item-desc">{{ opt.desc }}</span>
          </button>
        </div>
      </div>
    </div>

    <!-- 最大线程数（官方源并发；仅镜像模式由镜像源规定并锁定） -->
    <div class="dl-row">
      <span class="dl-label">最大线程数</span>
      <input
        class="dl-range"
        type="range"
        min="1"
        :max="concurrencyLocked ? MIRROR_CONCURRENCY : 256"
        step="1"
        v-model.number="dlConcurrency"
        @input="saveDownloadConfig"
        :disabled="concurrencyLocked"
      />
      <span class="dl-val">{{ concurrencyLocked ? MIRROR_CONCURRENCY : dlConcurrency }} / 256</span>
    </div>

    <!-- 速度限制：0 = 无上限 -->
    <div class="dl-row">
      <span class="dl-label">速度限制</span>
      <input
        class="dl-range"
        type="range"
        min="0"
        max="100"
        step="1"
        v-model.number="dlMaxSpeed"
        @input="saveDownloadConfig"
      />
      <span class="dl-val">{{ dlMaxSpeed === 0 ? "无上限" : dlMaxSpeed + " MB/s" }} / 100 MB/s</span>
    </div>

    <div v-if="dlError" class="dl-error">{{ dlError }}</div>
  </div>
</template>

<style>
@import "../styles/settings-shared.css";
</style>

<style scoped>
.dl-row {
  display: flex;
  align-items: center;
  gap: 14px;
  padding: 9px 0;
}

.dl-label {
  flex: 0 0 88px;
  font-size: 13.5px;
  font-weight: 600;
  color: var(--text-primary);
}

/* 点击展开的选择器 */
.dl-sel {
  position: relative;
  flex: 1;
  min-width: 0;
}
.dl-sel-btn {
  width: 100%;
  height: 36px;
  box-sizing: border-box;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  padding: 0 12px;
  font-size: 13px;
  color: var(--text-primary);
  background-color: var(--surface-elevated);
  border: 1px solid var(--border-default);
  border-radius: var(--radius-sm);
  cursor: pointer;
  transition: border-color 0.15s;
}
.dl-sel-btn > span {
  /* 固定行高并单行省略：容器高度已固定，行高变化不会再引起高度抖动。
     1.4 保证 CJK 字形（ink box 超出 line box）不被上下裁剪 */
  line-height: 1.4;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  min-width: 0;
}
.dl-sel-btn:hover { border-color: var(--color-brand); }
.dl-sel-btn svg {
  color: var(--text-tertiary);
  transition: transform 0.2s ease;
}
.dl-sel.open .dl-sel-btn svg { transform: rotate(180deg); }

.dl-sel-menu {
  position: absolute;
  top: calc(100% + 4px);
  left: 0;
  right: 0;
  z-index: 10;
  background-color: var(--surface-card);
  border: 1px solid var(--border-default);
  border-radius: var(--radius-md);
  box-shadow: var(--shadow-md);
  overflow: hidden;
}
.dl-sel-item {
  width: 100%;
  height: 52px;
  box-sizing: border-box;
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  justify-content: center;
  gap: 3px;
  padding: 0 12px;
  background: transparent;
  border: none;
  cursor: pointer;
  text-align: left;
}
.dl-sel-item + .dl-sel-item {
  border-top: 1px solid var(--border-default);
}
.dl-sel-item:hover { background-color: var(--surface-hover); }
.dl-sel-item.active .dl-sel-item-label { color: var(--color-brand); }
.dl-sel-item-label {
  font-size: 13px;
  font-weight: 600;
  color: var(--text-primary);
  /* 行高 1.4：CJK 字形 ink 超出 line box，1 会被上下裁剪（容器高度已固定，不会抖动） */
  line-height: 1.4;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  max-width: 100%;
}
.dl-sel-item-desc {
  font-size: 11.5px;
  color: var(--text-tertiary);
  line-height: 1.4;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  max-width: 100%;
}

/* 滑块（进度条） */
.dl-range {
  flex: 1;
  min-width: 0;
  accent-color: var(--color-brand);
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
  color: #e74c3c;
}
</style>
