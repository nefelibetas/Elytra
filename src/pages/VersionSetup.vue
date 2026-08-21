<script setup lang="ts">
// 版本设置页容器：左侧导航 + 各 tab 编排（业务在管理组件内）
import { computed, ref } from "vue";
import type { Component } from "vue";
import { useRoute } from "vue-router";
import { Puzzle, Map, Palette, Sun, Database, Upload, RefreshCw } from "@lucide/vue";
import VersionSetupExport from "../component/VersionSetupExport.vue";
import VersionSetupMods from "../component/VersionSetupMods.vue";
import VersionSetupSaves from "../component/VersionSetupSaves.vue";
import VersionSetupResource from "../component/VersionSetupResource.vue";
import {
  loadMods, loadModsQuick,
  loadWorlds, loadResourceDetailed, loadResourceQuick,
} from "../store/versionDetail";

type TabKey = "mods" | "saves" | "resourcepacks" | "shaderpacks" | "datapacks" | "export";

const tabs: { key: TabKey; label: string; icon: Component }[] = [
  { key: "mods", label: "Mod 管理", icon: Puzzle },
  { key: "saves", label: "存档管理", icon: Map },
  { key: "resourcepacks", label: "资源包管理", icon: Palette },
  { key: "shaderpacks", label: "光影包管理", icon: Sun },
  { key: "datapacks", label: "数据包管理", icon: Database },
  { key: "export", label: "导出", icon: Upload },
];

const activeTab = ref<TabKey>("mods");

const route = useRoute();

/** 游戏 id（版本目录名），由路由 query 传入。 */
const versionName = computed(() => String(route.query.name ?? ""));

/** 各管理 tab 的刷新动作（侧边 hover 刷新按钮，强制重载并更新缓存标记）。 */
const REFRESHERS: Record<TabKey, () => void> = {
  mods: () => {
    void loadMods(versionName.value);
    void loadModsQuick(versionName.value);
  },
  saves: () => {
    void loadWorlds(versionName.value);
  },
  resourcepacks: () => {
    void loadResourceDetailed(versionName.value, "resourcepacks");
    void loadResourceQuick(versionName.value, "resourcepacks");
  },
  shaderpacks: () => {
    void loadResourceDetailed(versionName.value, "shaderpacks");
    void loadResourceQuick(versionName.value, "shaderpacks");
  },
  datapacks: () => {
    void loadResourceDetailed(versionName.value, "datapacks");
    void loadResourceQuick(versionName.value, "datapacks");
  },
  export: () => {},
};
</script>

<template>
  <div class="vs">
    <!-- 左侧导航（标题栏由窗口 TitleBar 提供） -->
    <div class="vs-left">
      <div class="vs-nav-list">
        <div
          v-for="t in tabs"
          :key="t.key"
          class="vs-nav-item"
          :class="{ active: activeTab === t.key }"
          @click="activeTab = t.key"
        >
          <component :is="t.icon" :size="15" />
          <span>{{ t.label }}</span>
          <button
            v-if="t.key !== 'export'"
            type="button"
            class="vs-nav-refresh"
            v-tooltip="'刷新'"
            @click.stop="REFRESHERS[t.key]()"
          >
            <RefreshCw :size="12" />
          </button>
        </div>
      </div>
    </div>

    <!-- 右侧：当前 tab 内容（切换淡入淡出） -->
    <div class="vs-right">
      <Transition name="vs-switch" mode="out-in">
        <VersionSetupMods v-if="activeTab === 'mods'" key="mods" />

        <VersionSetupSaves v-else-if="activeTab === 'saves'" key="saves" />

        <VersionSetupResource
          v-else-if="activeTab === 'resourcepacks'"
          key="resourcepacks"
          kind="resourcepacks"
        />

        <VersionSetupResource
          v-else-if="activeTab === 'shaderpacks'"
          key="shaderpacks"
          kind="shaderpacks"
        />

        <VersionSetupResource
          v-else-if="activeTab === 'datapacks'"
          key="datapacks"
          kind="datapacks"
        />

        <VersionSetupExport v-else key="export" />
      </Transition>
    </div>
  </div>
</template>

<style scoped>
.vs {
  display: flex;
  height: 100%;
}

/* ===== 左侧导航 ===== */
.vs-left {
  flex: 0 0 240px;
  min-width: 220px;
  background-color: var(--surface-elevated);
  border-right: 1px solid var(--border-default);
  padding: 16px 12px;
  display: flex;
  flex-direction: column;
  overflow-y: auto;
  overflow-x: hidden;
}

.vs-nav-list {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.vs-nav-item {
  position: relative;
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 9px 10px;
  border-radius: var(--radius-sm);
  font-size: 13px;
  font-weight: 500;
  color: var(--text-secondary);
  cursor: pointer;
  transition: all 0.12s;
  user-select: none;
}

/* hover 显示的刷新按钮（切 Tab 缓存命中后手动刷新入口） */
.vs-nav-refresh {
  flex: none;
  margin-left: auto;
  width: 22px;
  height: 22px;
  display: flex;
  align-items: center;
  justify-content: center;
  border: none;
  border-radius: var(--radius-sm);
  background: transparent;
  color: var(--text-tertiary);
  cursor: pointer;
  opacity: 0;
  transition: opacity 0.15s, color 0.15s, background 0.15s;
}

.vs-nav-item:hover .vs-nav-refresh,
.vs-nav-refresh:focus-visible {
  opacity: 1;
}

.vs-nav-refresh:hover {
  color: var(--color-brand);
  background: var(--color-brand-bg);
}

.vs-nav-item:hover {
  background: var(--surface-hover);
  color: var(--text-primary);
}

.vs-nav-item.active {
  background: var(--color-brand-bg);
  color: var(--color-brand);
  font-weight: 600;
}

/* ===== 右侧 ===== */
.vs-right {
  flex: 1;
  min-width: 0;
  padding: 20px 24px;
  overflow: hidden;
  /* 子组件根 flex:1 占满剩余空间，列表滚动在卡片内部 */
  display: flex;
  flex-direction: column;
  background: var(--surface-page);
}

/* tab 内容切换动画 */
.vs-switch-enter-active,
.vs-switch-leave-active {
  transition: opacity 0.15s ease, transform 0.15s ease;
}

.vs-switch-enter-from {
  opacity: 0;
  transform: translateY(6px);
}

.vs-switch-leave-to {
  opacity: 0;
  transform: translateY(-6px);
}
</style>
