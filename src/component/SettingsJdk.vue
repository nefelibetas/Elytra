<script setup lang="ts">
// Java 运行时卡片：选择器下拉 + 扫描/自定义目录 + 下载缺失 JDK + 高级项（JVM 参数/GC/复选框）
import { onMounted, ref } from "vue";
import { Zap, ChevronDown, Check, RefreshCw, FolderPlus, Download } from "@lucide/vue";
import DropSelect, { type SelOption } from "./DropSelect.vue";
import {
  jdkList, autoMode, activeJdk, pickerOpen, loading, jdkError,
  pickerLabel, pickerSub, refreshJdks, selectCustomDir, selectAuto, selectJdk,
  installJdk, loadConfig,
} from "../store/jdkConfig";
import {
  loJvmArgs, loGameArgs, loPreCmd, loGc, loDisableWrapper, loDisableAgent, loHighGpu,
  gcOptions, saveLaunchOptions,
} from "../store/launchOptions";
import { showToast } from "../utils/toast";

onMounted(loadConfig);

/** 下载目标 Java 主版本（Mojang runtime 组件覆盖 8/17/21/25）。 */
const downloadMajor = ref(17);

/** 下载版本下拉选项（DropSelect 用）。 */
const jdkDownloadOptions: SelOption[] = [
  { value: "8", label: "Java 8" },
  { value: "17", label: "Java 17" },
  { value: "21", label: "Java 21" },
  { value: "25", label: "Java 25" },
];

async function handleInstallJdk() {
  try {
    await installJdk(downloadMajor.value);
    showToast(`Java ${downloadMajor.value} 下载并配置完成`);
  } catch (e) {
    showToast(String(e));
  }
}
</script>

<template>
  <div class="stg-card">
    <div class="stg-card-hdr">
      <h3>Java 运行时</h3>
      <span class="stg-card-sub">选择用于启动游戏的 Java 版本，或由启动器自动匹配</span>
    </div>

    <!-- 选择行：下拉 + 扫描类操作 -->
    <div class="jdk-toolbar">
      <div class="jdk-toolbar-row">
        <div class="jdk-picker-wrap">
          <button class="jdk-picker" :class="{ open: pickerOpen }" @click="pickerOpen = !pickerOpen">
            <span class="jdk-picker-main">
              <Zap :size="16" />
              {{ pickerLabel }}
            </span>
            <span class="jdk-picker-sub">{{ pickerSub }}</span>
            <ChevronDown class="jdk-chevron" :class="{ flip: pickerOpen }" :size="12" />
          </button>

          <!-- 展开列表 -->
          <Transition name="drop">
            <div v-if="pickerOpen" class="jdk-dropdown">
              <button class="jdk-opt" :class="{ selected: autoMode }" @click="selectAuto">
                <div class="jdk-opt-main">
                  <span class="jdk-opt-title">自动选择</span>
                  <span class="jdk-opt-sub">根据游戏版本需求自动匹配</span>
                </div>
                <Check v-if="autoMode" :size="14" />
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
                <Check v-if="!autoMode && activeJdk && activeJdk.path === jdk.path" :size="14" />
              </button>
              <div v-if="!jdkList.length" class="jdk-drop-empty">暂无扫描结果，点击「刷新」扫描</div>
            </div>
          </Transition>
        </div>

        <button class="tool-btn" :disabled="loading" @click="refreshJdks">
          <RefreshCw :size="14" />
          刷新
        </button>
        <button class="tool-btn" :disabled="loading" @click="selectCustomDir">
          <FolderPlus :size="14" />
          自定义目录
        </button>
      </div>

      <!-- 下载行：安装缺失 Java（Mojang 官方/备用源自动安装） -->
      <div class="jdk-dl-group">
        <span class="jdk-dl-label">下载缺失 JDK</span>
        <DropSelect
          :model-value="String(downloadMajor)"
          :options="jdkDownloadOptions"
          width="96px"
          @update:model-value="downloadMajor = Number($event)"
        />
        <button class="tool-btn jdk-dl-btn" :disabled="loading" @click="handleInstallJdk">
          <Download :size="14" />
          {{ loading ? "下载中..." : "下载" }}
        </button>
      </div>
    </div>

    <div v-if="loading" class="loading-hint">正在处理...</div>
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
        <DropSelect
          id="lo-gc"
          :model-value="loGc"
          :options="gcOptions.map(g => ({ value: g.value, label: g.label }))"
          @update:model-value="loGc = $event; saveLaunchOptions()"
        />
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
  flex-direction: column;
  gap: 10px;
}

/* 第一行：选择器 + 扫描类操作（刷新/自定义目录） */
.jdk-toolbar-row {
  display: flex;
  align-items: center;
  gap: 8px;
}

/* 第二行：下载安装容器（与扫描操作分组隔离） */
.jdk-dl-group {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px 10px;
  background: var(--surface-page);
  border: 1px solid var(--border-default);
  border-radius: var(--radius-md);
}

.jdk-dl-label {
  flex: none;
  font-size: 12px;
  font-weight: 600;
  color: var(--text-secondary);
}

.jdk-dl-btn {
  padding: 7px 14px;
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
  box-shadow: var(--shadow-md);
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
