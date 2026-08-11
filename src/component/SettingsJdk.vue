<script setup lang="ts">
// Java 运行时卡片：选择器下拉 + 扫描/自定义目录 + 高级项（JVM 参数/GC/复选框）
import { onMounted } from "vue";
import {
  jdkList, autoMode, activeJdk, pickerOpen, loading, jdkError,
  pickerLabel, pickerSub, refreshJdks, selectCustomDir, selectAuto, selectJdk,
  loadConfig,
} from "../store/jdkConfig";
import {
  loJvmArgs, loGameArgs, loPreCmd, loGc, loDisableWrapper, loDisableAgent, loHighGpu,
  gcOptions, saveLaunchOptions,
} from "../store/launchOptions";

onMounted(loadConfig);
</script>

<template>
  <div class="stg-card">
    <div class="stg-card-hdr">
      <h3>Java 运行时</h3>
      <span class="stg-card-sub">选择用于启动游戏的 Java 版本，或由启动器自动匹配</span>
    </div>

    <!-- 选择行：下拉 + 右侧按钮 -->
    <div class="jdk-toolbar">
      <div class="jdk-picker-wrap">
        <button class="jdk-picker" :class="{ open: pickerOpen }" @click="pickerOpen = !pickerOpen">
          <span class="jdk-picker-main">
            <svg viewBox="0 0 16 16" width="16" height="16" fill="currentColor">
              <path d="M11 1L5 7v8h6V7L7 3l4-4V1z" />
            </svg>
            {{ pickerLabel }}
          </span>
          <span class="jdk-picker-sub">{{ pickerSub }}</span>
          <svg class="jdk-chevron" :class="{ flip: pickerOpen }" viewBox="0 0 16 16" width="12" height="12" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <path d="M3 6l5 5 5-5" />
          </svg>
        </button>

        <!-- 展开列表 -->
        <Transition name="drop">
          <div v-if="pickerOpen" class="jdk-dropdown">
            <button class="jdk-opt" :class="{ selected: autoMode }" @click="selectAuto">
              <div class="jdk-opt-main">
                <span class="jdk-opt-title">自动选择</span>
                <span class="jdk-opt-sub">根据游戏版本需求自动匹配</span>
              </div>
              <svg v-if="autoMode" viewBox="0 0 14 14" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M2 7l3.5 3.5L12 3" /></svg>
            </button>
            <div v-if="jdkList.length" class="jdk-drop-divider"></div>
            <button
              v-for="jdk in jdkList"
              :key="jdk.path"
              class="jdk-opt"
              :class="{ selected: !autoMode && activeJdk && activeJdk.path === jdk.path }"
              @click="selectJdk(jdk)"
            >
              <div class="jdk-opt-main">
                <span class="jdk-opt-title">{{ jdk.version }}</span>
                <span class="jdk-opt-sub">{{ jdk.vendor }} · {{ jdk.bits }} 位</span>
                <span class="jdk-opt-path">{{ jdk.path }}</span>
              </div>
              <svg v-if="!autoMode && activeJdk && activeJdk.path === jdk.path" viewBox="0 0 14 14" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M2 7l3.5 3.5L12 3" /></svg>
            </button>
            <div v-if="!jdkList.length" class="jdk-drop-empty">暂无扫描结果，点击右上角「刷新」</div>
          </div>
        </Transition>
      </div>

      <button class="tool-btn" :disabled="loading" @click="refreshJdks">
        <svg viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
          <path d="M14 8a6 6 0 01-11 3.5"/>
          <polygon points="3,14 2.5,10 7,11" stroke="none" fill="currentColor"/>
          <path d="M2 8a6 6 0 0111-3.5"/>
          <polygon points="13,2 13.5,6 9,5" stroke="none" fill="currentColor"/>
        </svg>
        刷新
      </button>
      <button class="tool-btn" :disabled="loading" @click="selectCustomDir">
        <svg viewBox="0 0 16 16" width="14" height="14" fill="currentColor">
          <path d="M2 3a1 1 0 011-1h3l2 2h6a1 1 0 011 1v8a1 1 0 01-1 1H3a1 1 0 01-1-1V3z" />
        </svg>
        自定义目录
      </button>
    </div>

    <div v-if="loading" class="loading-hint">正在扫描...</div>
    <div v-else-if="jdkError" class="err">{{ jdkError }}</div>

    <div class="jdk-divider"></div>

    <!-- Java 高级项 -->
    <div class="launch-opts jdk-opts">
      <div class="lo-row lo-top">
        <label class="lo-label" for="lo-jvm">Java 虚拟机参数</label>
        <textarea
          id="lo-jvm"
          v-model="loJvmArgs"
          class="lo-textarea"
          rows="4"
          wrap="off"
          placeholder="每行一个 JVM 参数"
          spellcheck="false"
          @change="saveLaunchOptions"
        ></textarea>
      </div>

      <div class="lo-row lo-top">
        <label class="lo-label" for="lo-game">游戏参数</label>
        <textarea
          id="lo-game"
          v-model="loGameArgs"
          class="lo-textarea"
          rows="2"
          wrap="off"
          placeholder="每行一个游戏参数"
          spellcheck="false"
          @change="saveLaunchOptions"
        ></textarea>
      </div>

      <div class="lo-row">
        <label class="lo-label" for="lo-pre">启动前执行命令</label>
        <input
          id="lo-pre"
          v-model="loPreCmd"
          class="lo-input"
          type="text"
          placeholder="启动游戏前执行的命令"
          @change="saveLaunchOptions"
        />
      </div>

      <div class="lo-row">
        <label class="lo-label" for="lo-gc">内存管理</label>
        <select
          id="lo-gc"
          v-model="loGc"
          class="lo-select"
          @change="saveLaunchOptions"
        >
          <option v-for="g in gcOptions" :key="g.value" :value="g.value">{{ g.label }}</option>
        </select>
      </div>

      <div class="lo-checks">
        <label class="lo-check">
          <input v-model="loDisableWrapper" type="checkbox" @change="saveLaunchOptions" />
          <span>禁用 Java Launch Wrapper</span>
        </label>
        <label class="lo-check">
          <input v-model="loDisableAgent" type="checkbox" @change="saveLaunchOptions" />
          <span>禁用 LWJGL Unsafe Agent</span>
        </label>
        <label class="lo-check">
          <input v-model="loHighGpu" type="checkbox" @change="saveLaunchOptions" />
          <span>使用高性能显卡</span>
        </label>
      </div>
    </div>
  </div>
</template>

<style>
@import "../styles/settings-shared.css";
</style>

<style scoped>
/* ===== JDK 工具栏 ===== */
.jdk-toolbar {
  display: flex;
  align-items: flex-start;
  gap: 10px;
}

.jdk-picker-wrap {
  flex: 1;
  min-width: 0;
  position: relative;
}

.jdk-picker {
  width: 100%;
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 10px 14px;
  border: 1px solid var(--border-strong);
  border-radius: var(--radius-md);
  background: var(--surface-page);
  cursor: pointer;
  transition: all 0.15s;
}

.jdk-picker:hover {
  border-color: var(--color-brand);
}

.jdk-picker.open {
  border-color: var(--color-brand);
  box-shadow: 0 0 0 3px var(--color-brand-bg);
}

.jdk-picker-main {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 14px;
  font-weight: 600;
  color: var(--text-primary);
  white-space: nowrap;
}

.jdk-picker-main svg {
  color: var(--color-brand);
}

.jdk-picker-sub {
  flex: 1;
  min-width: 0;
  font-size: 12px;
  color: var(--text-tertiary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.jdk-chevron {
  flex-shrink: 0;
  color: var(--text-tertiary);
  transition: transform 0.15s;
}

.jdk-chevron.flip {
  transform: rotate(180deg);
}

/* 下拉列表 */
.jdk-dropdown {
  position: absolute;
  top: calc(100% + 6px);
  left: 0;
  right: 0;
  max-height: 280px;
  overflow-y: auto;
  overflow-x: hidden;
  overscroll-behavior-x: none;
  touch-action: pan-y;
  background: var(--surface-card);
  border: 1px solid var(--border-strong);
  border-radius: var(--radius-md);
  box-shadow: 0 8px 24px rgba(30, 27, 75, 0.12);
  z-index: 100;
  padding: 6px;
}

.jdk-opt {
  width: 100%;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
  padding: 10px 12px;
  border: none;
  border-radius: var(--radius-sm);
  background: transparent;
  cursor: pointer;
  text-align: left;
  transition: background 0.1s;
}

.jdk-opt:hover {
  background: var(--surface-hover);
}

.jdk-opt.selected {
  background: var(--color-brand-bg);
}

.jdk-opt.selected .jdk-opt-title {
  color: var(--color-brand);
}

.jdk-opt-main {
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 1px;
}

.jdk-opt-title {
  font-size: 13px;
  font-weight: 600;
  color: var(--text-primary);
}

.jdk-opt-sub {
  font-size: 11px;
  color: var(--text-tertiary);
}

.jdk-opt-path {
  font-size: 11px;
  color: var(--text-tertiary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.jdk-opt > svg {
  flex-shrink: 0;
  color: var(--color-brand);
}

.jdk-drop-divider {
  height: 1px;
  background: var(--border-default);
  margin: 4px 8px;
}

.jdk-drop-empty {
  padding: 14px;
  text-align: center;
  font-size: 12px;
  color: var(--text-tertiary);
}

/* 下拉动画 */
.drop-enter-active { transition: all 0.15s ease-out; }
.drop-leave-active { transition: all 0.1s ease-in; }
.drop-enter-from { opacity: 0; transform: translateY(-6px); }
.drop-leave-to { opacity: 0; transform: translateY(-6px); }

.jdk-divider {
  height: 1px;
  background: var(--border-default);
  margin: 16px 0;
}
</style>
