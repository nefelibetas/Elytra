<script setup lang="ts">
import { computed } from "vue";
import { useRoute } from "vue-router";

import { goBack } from "../../store/nav";
import { ArrowLeft, Play, Download, ListChecks, Settings, Minus, X, Feather, Wrench } from "@lucide/vue";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { detailTitle } from "../../store/resourceDetail";
import { dlState } from "../../store/download";

const route = useRoute();
const appWindow = getCurrentWindow();

const isDetail = computed(
  () =>
    route.path === "/resource-detail" ||
    route.path === "/version-select" ||
    route.path === "/version-setup" ||
    route.path === "/custom-theme",
);

/** 次级页面标题栏文案。 */
const barTitle = computed(() => {
  if (route.path === "/version-select") return "版本选择";
  if (route.path === "/version-setup") {
    return `版本设置 - ${String(route.query.name ?? "")}`;
  }
  if (route.path === "/custom-theme") {
    // 新建（无 id）与编辑（有 id）区分标题
    return route.query.id ? "编辑自定义外观" : "新建自定义外观";
  }
  if (route.path === "/resource-detail") {
    // 资源详情：有译名时 "翻译名（原名）"，否则原名
    return detailTitle() || "资源详情";
  }
  return "";
});

const items = [
  { path: "/", label: "启动", icon: "play" },
  { path: "/download", label: "资源中心", icon: "download" },
  { path: "/download-detail", label: "任务中心", icon: "list" },
  { path: "/tools", label: "工具", icon: "tools" },
  { path: "/settings", label: "设置", icon: "settings" },
] as const;

/** 任务中心导航项：有活动任务时显示进行中徽标。 */
const activeTasks = computed(() => dlState.tasks.filter(t => t.status === "running" || t.status === "pending").length);

function isActive(p: string) { return route.path === p; }
function minimize() { appWindow.minimize(); }
function closeWindow() { appWindow.close(); }

function onMouseDown(e: MouseEvent) {
  const target = e.target as HTMLElement;
  if (target.closest("a, button")) return;
  appWindow.startDragging();
}

</script>

<template>
  <header class="titlebar" @mousedown="onMouseDown">
    <Transition name="bar-slide" mode="out-in">
      <div v-if="isDetail" key="detail" class="bar-inner">
        <div class="tl">
          <button class="back-btn" @click="goBack">
            <ArrowLeft :size="18" />
            <span>{{ barTitle }}</span>
          </button>
        </div>
        <div class="tc"></div>
      </div>
      <div v-else key="main" class="bar-inner">
        <div class="tl">
          <span class="logo">
            <Feather :size="15" class="logo-wing" />
            Elytra
          </span>
        </div>
        <nav class="tc">
          <router-link
            v-for="item in items" :key="item.path"
            :to="item.path"
            class="nav" :class="{ active: isActive(item.path) }"
          >
            <Play v-if="item.icon === 'play'" :size="16" />
            <Download v-if="item.icon === 'download'" :size="16" />
            <ListChecks v-if="item.icon === 'list'" :size="16" />
            <Settings v-if="item.icon === 'settings'" :size="16" />
            <Wrench v-if="item.icon === 'tools'" :size="16" />
            <span class="label">{{ item.label }}</span>
            <span
              v-if="item.path === '/download-detail' && activeTasks > 0"
              class="nav-badge"
            >{{ activeTasks }}</span>
          </router-link>
        </nav>
      </div>
    </Transition>

    <div class="tr">
      <button class="win" @click="minimize" v-tooltip="'最小化'">
        <Minus :size="12" />
      </button>
      <button class="win close" @click="closeWindow" v-tooltip="'关闭'">
        <X :size="12" />
      </button>
    </div>
  </header>
</template>

<style scoped>
.titlebar {
  position: relative;
  z-index: 10;
  height: var(--titlebar-h);
  min-height: var(--titlebar-h);
  background: var(--color-brand-gradient);
  display: flex;
  align-items: center;
  padding: 0 12px;
  gap: 16px;
  user-select: none;
  box-shadow: var(--shadow-sm);
}

.tl { display: flex; align-items: center; width: auto; min-width: 0; }

.logo {
  font-size: 18px;
  font-weight: 800;
  color: var(--color-brand);
  letter-spacing: -0.5px;
  padding: 0 10px;
  height: 30px;
  display: flex;
  align-items: center;
  gap: 6px;
  border-radius: var(--radius-md);
  background-color: rgba(255, 255, 255, 0.95);
}

.logo-wing {
  color: var(--color-brand-dark);
}

.back-btn {
  display: flex; align-items: center; gap: 6px;
  padding: 4px 10px;
  border: none; background: transparent;
  color: var(--titlebar-fg-dim);
  color: color-mix(in srgb, var(--titlebar-fg) 85%, transparent);
  cursor: pointer; border-radius: var(--radius-sm);
  font-size: 14px; font-weight: 500;
  transition: all 0.15s;
  outline: none;
}
.back-btn svg { flex-shrink: 0; }
.back-btn:hover { background-color: var(--titlebar-hover); background-color: color-mix(in srgb, var(--titlebar-fg) 15%, transparent); color: var(--titlebar-fg); }
.back-btn { white-space: nowrap; }

.tc {
  flex: 1;
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 4px;
  height: 100%;
}

.nav {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 6px 14px;
  border-radius: var(--radius-sm);
  color: var(--titlebar-fg-dim);
  text-decoration: none;
  font-size: 13px;
  font-weight: 500;
  transition: all 0.15s;
  outline: none;
}

.nav:hover {
  color: var(--titlebar-fg);
  background-color: var(--titlebar-hover);
  background-color: color-mix(in srgb, var(--titlebar-fg) 14%, transparent);
}

.nav.active {
  color: var(--titlebar-fg);
  background-color: var(--titlebar-active);
  background-color: color-mix(in srgb, var(--titlebar-fg) 22%, transparent);
}

.label { font-size: 13px; }

.nav-badge {
  min-width: 16px;
  height: 16px;
  padding: 0 4px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  border-radius: 8px;
  background-color: rgba(255, 255, 255, 0.9);
  color: var(--color-brand-dark);
  font-size: 10.5px;
  font-weight: 700;
  line-height: 1;
  font-variant-numeric: tabular-nums;
}

.tr {
  display: flex;
  align-items: center;
  gap: 4px;
  width: 100px;
  justify-content: flex-end;
}

.win {
  width: 30px; height: 30px;
  display: flex; align-items: center; justify-content: center;
  border: none; background: transparent;
  color: var(--titlebar-fg-dim);
  color: color-mix(in srgb, var(--titlebar-fg) 65%, transparent);
  cursor: pointer;
  border-radius: var(--radius-sm);
  transition: all 0.15s;
  outline: none;
}

.win:hover {
  background-color: var(--titlebar-hover);
  background-color: color-mix(in srgb, var(--titlebar-fg) 16%, transparent);
  color: var(--titlebar-fg);
}

.close:hover {
  background-color: var(--titlebar-active);
  background-color: color-mix(in srgb, var(--titlebar-fg) 24%, transparent);
  color: var(--titlebar-fg);
}

.bar-inner {
  display: flex; align-items: center; gap: 16px;
  width: 100%; height: 100%;
}

.bar-slide-enter-active, .bar-slide-leave-active {
  transition: all 0.2s ease;
}
.bar-slide-enter-from {
  opacity: 0; transform: translateX(24px);
}
.bar-slide-leave-to {
  opacity: 0; transform: translateX(-24px);
}
</style>
