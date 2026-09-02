<script lang="ts">
// 工具 tab 状态：模块级持久（路由切换不丢），对齐 settingsTab 先例。
// 放普通 <script> 块：<script setup> 不允许 export。
import { ref } from "vue";

export type ToolsTab = "status" | "motd" | "give" | "potion" | "firework" | "villager" | "online";

export const toolsTab = ref<ToolsTab>("status");
</script>

<script setup lang="ts">
// 工具页容器：左侧导航 + 各工具编排（业务在组件内，本文件不做工具逻辑）。
// 工具：服务器状态 / MOTD / /give / 药水 / 烟花 / 村民交易 / 联机；后续工具加条目即可。
import { Server, Type, Link2, Sword, FlaskConical, Rocket, Store } from "@lucide/vue";
import ServerStatusTool from "../component/tools/ServerStatusTool.vue";
import MotdGeneratorTool from "../component/tools/MotdGeneratorTool.vue";
import GiveGeneratorTool from "../component/tools/GiveGeneratorTool.vue";
import PotionGeneratorTool from "../component/tools/PotionGeneratorTool.vue";
import FireworkGeneratorTool from "../component/tools/FireworkGeneratorTool.vue";
import VillagerGeneratorTool from "../component/tools/VillagerGeneratorTool.vue";
import OnlineTool from "../component/tools/OnlineTool.vue";
</script>

<template>
  <div class="tools">
    <nav class="tools-nav">
      <div class="nav-divider"><span>工具</span></div>
      <button
        class="tools-tab"
        :class="{ active: toolsTab === 'status' }"
        @click="toolsTab = 'status'"
      >
        <Server :size="16" />
        服务器状态
      </button>
      <button
        class="tools-tab"
        :class="{ active: toolsTab === 'motd' }"
        @click="toolsTab = 'motd'"
      >
        <Type :size="16" />
        MOTD 生成器
      </button>
      <button
        class="tools-tab"
        :class="{ active: toolsTab === 'give' }"
        @click="toolsTab = 'give'"
      >
        <Sword :size="16" />
        /give 生成器
      </button>
      <button
        class="tools-tab"
        :class="{ active: toolsTab === 'potion' }"
        @click="toolsTab = 'potion'"
      >
        <FlaskConical :size="16" />
        药水
      </button>
      <button
        class="tools-tab"
        :class="{ active: toolsTab === 'firework' }"
        @click="toolsTab = 'firework'"
      >
        <Rocket :size="16" />
        烟花
      </button>
      <button
        class="tools-tab"
        :class="{ active: toolsTab === 'villager' }"
        @click="toolsTab = 'villager'"
      >
        <Store :size="16" />
        村民交易
      </button>
      <button
        class="tools-tab"
        :class="{ active: toolsTab === 'online' }"
        @click="toolsTab = 'online'"
      >
        <Link2 :size="16" />
        联机
      </button>
    </nav>

    <div class="tools-body">
      <Transition name="page" mode="out-in">
        <ServerStatusTool v-if="toolsTab === 'status'" key="status" />
        <MotdGeneratorTool v-else-if="toolsTab === 'motd'" key="motd" />
        <GiveGeneratorTool v-else-if="toolsTab === 'give'" key="give" />
        <PotionGeneratorTool v-else-if="toolsTab === 'potion'" key="potion" />
        <FireworkGeneratorTool v-else-if="toolsTab === 'firework'" key="firework" />
        <VillagerGeneratorTool v-else-if="toolsTab === 'villager'" key="villager" />
        <OnlineTool v-else key="online" />
      </Transition>
    </div>

    <ScrollTopFab />
  </div>
</template>

<style scoped>
.tools {
  display: flex;
  height: 100%;
}

/* ===== 左侧导航 ===== */
.tools-nav {
  flex: 0 0 180px;
  min-width: 160px;
  background-color: var(--surface-elevated);
  border-right: 1px solid var(--border-default);
  padding: 12px 8px;
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.nav-divider {
  display: flex;
  align-items: center;
  padding: 8px 12px 6px;
  user-select: none;
}

.nav-divider span {
  font-size: 11px;
  font-weight: 500;
  color: var(--text-tertiary);
  text-transform: uppercase;
  letter-spacing: 0.8px;
}

.tools-tab {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px 12px;
  border: none;
  border-radius: var(--radius-sm);
  background: transparent;
  color: var(--text-secondary);
  font-size: 13px;
  font-weight: 500;
  cursor: pointer;
  transition: all 0.15s;
  text-align: left;
}

.tools-tab:hover {
  color: var(--text-primary);
  background-color: var(--surface-hover);
}

.tools-tab.active {
  color: var(--color-brand);
  background-color: var(--color-brand-bg);
}

/* ===== 右侧内容 ===== */
.tools-body {
  flex: 1;
  position: relative;
  padding: 20px 24px;
  overflow-y: auto;
  overflow-x: hidden;
  scrollbar-gutter: stable;
  background: var(--surface-page);
}

/* 页面过渡 */
.page-enter-active, .page-leave-active { transition: all 0.15s ease-out; }
.page-enter-from { opacity: 0; transform: translateY(8px); }
.page-leave-to { opacity: 0; transform: translateY(-8px); }
</style>
