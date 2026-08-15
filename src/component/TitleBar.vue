<script setup lang="ts">
import { computed } from "vue";
import { useRoute, useRouter } from "vue-router";
import { ArrowLeft, Play, Download, ListChecks, Settings, Minus, X, Feather } from "@lucide/vue";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { detailTitle, goBackDetail } from "../store/modDetail";
import { dlState } from "../store/download";

const route = useRoute();
const router = useRouter();
const appWindow = getCurrentWindow();

const isDetail = computed(
  () =>
    route.path === "/mod-detail" ||
    route.path === "/version-select" ||
    route.path === "/version-setup",
);

/** 次级页面标题栏文案。 */
const barTitle = computed(() => {
  if (route.path === "/version-select") return "版本选择";
  if (route.path === "/version-setup") {
    return `版本设置 - ${String(route.query.name ?? "")}`;
  }
  if (route.path === "/mod-detail") {
    // Mod 详情：有译名时 "翻译名（原名）"，否则原名
    return detailTitle() || "Mod 详情";
  }
  return "";
});

const items = [
  { path: "/", label: "启动", icon: "play" },
  { path: "/download", label: "下载", icon: "download" },
  { path: "/download-detail", label: "下载管理", icon: "list" },
  { path: "/settings", label: "设置", icon: "settings" },
] as const;

/** 下载管理导航项：有活动任务时显示进行中徽标。 */
const activeTasks = computed(() => dlState.tasks.filter(t => t.status === "running" || t.status === "pending").length);

function isActive(p: string) { return route.path === p; }
function minimize() { appWindow.minimize(); }
function closeWindow() { appWindow.close(); }

function onMouseDown(e: MouseEvent) {
  const target = e.target as HTMLElement;
  if (target.closest("a, button")) return;
  appWindow.startDragging();
}

function goBack() {
  const p = router.currentRoute.value.path;
  if (p === "/version-select") {
    router.push("/");
    return;
  }
  if (p === "/version-setup") {
    router.push("/version-select");
    return;
  }
  if (p === "/mod-detail") {
    // Mod 详情：弹详情历史栈（前置依赖链）；栈空回下载页并恢复来源 tab
    goBackDetail();
    return;
  }
  // 其余详情页返回启动页
  router.push("/");
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
  color: rgba(255,255,255,0.85);
  cursor: pointer; border-radius: var(--radius-sm);
  font-size: 14px; font-weight: 500;
  transition: all 0.15s;
  outline: none;
}
.back-btn svg { flex-shrink: 0; }
.back-btn:hover { background-color: rgba(255,255,255,0.15); color: #fff; }
.back-btn { white-space: nowrap; }

.dt-title {
  font-size: 14px; font-weight: 600;
  color: rgba(255,255,255,0.9);
}

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
  color: rgba(255, 255, 255, 0.7);
  text-decoration: none;
  font-size: 13px;
  font-weight: 500;
  transition: all 0.15s;
  outline: none;
}

.nav:hover {
  color: #fff;
  background-color: rgba(255, 255, 255, 0.14);
}

.nav.active {
  color: #fff;
  background-color: rgba(255, 255, 255, 0.22);
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
  color: rgba(255, 255, 255, 0.65);
  cursor: pointer;
  border-radius: var(--radius-sm);
  transition: all 0.15s;
  outline: none;
}

.win:hover {
  background-color: rgba(255, 255, 255, 0.16);
  color: #fff;
}

.close:hover {
  background-color: rgba(255, 255, 255, 0.24);
  color: #fff;
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
