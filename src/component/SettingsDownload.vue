<script setup lang="ts">
// 下载设置卡片（管理游戏下载源）：文件/版本列表/社区源 + 网络代理 + 线程数
import { onMounted } from "vue";
import { ChevronDown } from "@lucide/vue";
import {
  dlFileMode, dlManifestMode, dlCommunityMode, dlConcurrency,
  dlFileOpen, dlManifestOpen, dlCommunityOpen, dlNetworkOpen, dlTranslateOpen, dlTranslateMode,
  dlError, dlShowReleaseOnly, dlNetworkMode,
  sourceModeOptions, communitySourceOptions, sourceModeLabel,
  networkModeOptions, networkModeLabel,
  translateDisplayOptions, translateModeLabel,
  concurrencyLocked,
  saveDownloadConfig, setFileMode, setCommunityMode, setManifestMode, setNetworkMode,
  setTranslateMode, toggleDlOpen, loadDownloadConfig,
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
        <button class="dl-sel-btn" @click="toggleDlOpen('file')">
          <span>{{ sourceModeLabel(dlFileMode) }}</span>
          <ChevronDown :size="12" />
        </button>
        <Transition name="dl-drop">
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
        </Transition>
      </div>
    </div>

    <!-- 版本列表源 -->
    <div class="dl-row">
      <span class="dl-label">版本列表源</span>
      <div class="dl-sel" :class="{ open: dlManifestOpen }">
        <button class="dl-sel-btn" @click="toggleDlOpen('manifest')">
          <span>{{ sourceModeLabel(dlManifestMode) }}</span>
          <ChevronDown :size="12" />
        </button>
        <Transition name="dl-drop">
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
        </Transition>
      </div>
    </div>

    <!-- 社区下载源：Modrinth 官方 vs MCIM 镜像（管理 Fabric API 等社区资源） -->
    <div class="dl-row">
      <span class="dl-label">社区下载源</span>
      <div class="dl-sel" :class="{ open: dlCommunityOpen }">
        <button class="dl-sel-btn" @click="toggleDlOpen('community')">
          <span>{{ communitySourceOptions.find(o => o.value === dlCommunityMode)?.label ?? dlCommunityMode }}</span>
          <ChevronDown :size="12" />
        </button>
        <Transition name="dl-drop">
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
        </Transition>
      </div>
    </div>

    <!-- 网络代理：自动检测系统代理 / 强制系统代理 / 直连（TUN 由网络层接管） -->
    <div class="dl-row">
      <span class="dl-label">网络代理</span>
      <div class="dl-sel" :class="{ open: dlNetworkOpen }">
        <button class="dl-sel-btn" @click="toggleDlOpen('network')">
          <span>{{ networkModeLabel(dlNetworkMode) }}</span>
          <ChevronDown :size="12" />
        </button>
        <Transition name="dl-drop">
          <div v-if="dlNetworkOpen" class="dl-sel-menu">
            <button
              v-for="opt in networkModeOptions"
              :key="opt.value"
              class="dl-sel-item"
              :class="{ active: dlNetworkMode === opt.value }"
              @click="setNetworkMode(opt.value)"
            >
              <span class="dl-sel-item-label">{{ opt.label }}</span>
              <span class="dl-sel-item-desc">{{ opt.desc }}</span>
            </button>
          </div>
        </Transition>
      </div>
    </div>

    <!-- 翻译显示：社区搜索结果简介的显示语言（仅中文/中英对照/仅英文） -->
    <div class="dl-row">
      <span class="dl-label">翻译显示</span>
      <div class="dl-sel" :class="{ open: dlTranslateOpen }">
        <button class="dl-sel-btn" @click="toggleDlOpen('translate')">
          <span>{{ translateModeLabel(dlTranslateMode) }}</span>
          <ChevronDown :size="12" />
        </button>
        <Transition name="dl-drop">
          <div v-if="dlTranslateOpen" class="dl-sel-menu">
            <button
              v-for="opt in translateDisplayOptions"
              :key="opt.value"
              class="dl-sel-item"
              :class="{ active: dlTranslateMode === opt.value }"
              @click="setTranslateMode(opt.value)"
            >
              <span class="dl-sel-item-label">{{ opt.label }}</span>
              <span class="dl-sel-item-desc">{{ opt.desc }}</span>
            </button>
          </div>
        </Transition>
      </div>
    </div>

    <!-- 社区资源版本过滤：快照/预发布/候选等非正式版本（默认开启过滤） -->
    <div class="dl-row">
      <label class="lo-check">
        <input v-model="dlShowReleaseOnly" type="checkbox" @change="saveDownloadConfig" />
        <span>社区资源仅显示正式版（过滤快照/预发布版本）</span>
      </label>
    </div>

    <!-- 最大线程数：官方源并发（仅镜像模式由镜像源固定 8，隐藏该行——无操作意义） -->
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
      />
      <span class="dl-val">{{ dlConcurrency }} / 256</span>
    </div>
    <!-- Auto（智能）模式：官方源优先 + 镜像降级，并发仅作用于官方源，镜像源固定 8 -->
    <p v-if="dlFileMode === 'auto' && !concurrencyLocked" class="dl-note">
      该并发仅对官方源生效，镜像源固定 8 线程
    </p>

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

/* 线程数备注：Auto 模式说明并发作用域 */
.dl-note {
  margin: -4px 0 4px 102px;
  font-size: 11px;
  color: var(--text-tertiary);
  line-height: 1.5;
}

/* 下拉面板展开/收起动画 */
.dl-drop-enter-active,
.dl-drop-leave-active {
  transition: opacity 0.15s ease, transform 0.15s ease;
}

.dl-drop-enter-from,
.dl-drop-leave-to {
  opacity: 0;
  transform: translateY(-6px);
}
</style>
