<script setup lang="ts">
import { computed } from "vue";
import { useRoute, useRouter } from "vue-router";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { dlState } from "../store/download";

const route = useRoute();
const router = useRouter();
const appWindow = getCurrentWindow();

const isDetail = computed(
  () =>
    route.path === "/download-detail" ||
    route.path === "/version-select" ||
    route.path === "/version-setup",
);

/** 次级页面标题栏文案。 */
const barTitle = computed(() => {
  if (route.path === "/version-select") return "版本选择";
  if (route.path === "/version-setup") {
    return `版本设置 - ${String(route.query.name ?? "")}`;
  }
  return "下载管理";
});

const items = [
  { path: "/", label: "启动", icon: "play" },
  { path: "/download", label: "下载", icon: "download" },
  { path: "/settings", label: "设置", icon: "settings" },
] as const;

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
  const prev = dlState.previousRoute || "/";
  router.push(prev);
  dlState.previousRoute = "/";
}
</script>

<template>
  <header class="titlebar" @mousedown="onMouseDown">
    <Transition name="bar-slide" mode="out-in">
      <div v-if="isDetail" key="detail" class="bar-inner">
        <div class="tl">
          <button class="back-btn" @click="goBack">
            <svg viewBox="0 0 16 16" width="18" height="18" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M10 2L4 8l6 6" /></svg>
            <span>{{ barTitle }}</span>
          </button>
        </div>
        <div class="tc"></div>
      </div>
      <div v-else key="main" class="bar-inner">
        <div class="tl">
          <span class="logo">Elytra</span>
        </div>
        <nav class="tc">
          <router-link
            v-for="item in items" :key="item.path"
            :to="item.path"
            class="nav" :class="{ active: isActive(item.path) }"
          >
            <svg v-if="item.icon === 'play'" viewBox="0 0 16 16" width="16" height="16" fill="currentColor">
              <polygon points="3,1 14,8 3,15" />
            </svg>
            <svg v-if="item.icon === 'download'" viewBox="0 0 16 16" width="16" height="16" fill="currentColor">
              <rect x="2" y="12" width="12" height="2" rx="1" />
              <polygon points="7,1 9,1 9,7 11.5,4.5 13,6 8,11 3,6 4.5,4.5 7,7" />
            </svg>
            <svg v-if="item.icon === 'settings'" viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round">
              <line x1="21" y1="4" x2="14" y2="4"/>
              <line x1="10" y1="4" x2="3" y2="4"/>
              <line x1="21" y1="12" x2="12" y2="12"/>
              <line x1="8" y1="12" x2="3" y2="12"/>
              <line x1="21" y1="20" x2="16" y2="20"/>
              <line x1="12" y1="20" x2="3" y2="20"/>
              <line x1="14" y1="2" x2="14" y2="6"/>
              <line x1="8" y1="10" x2="8" y2="14"/>
              <line x1="16" y1="18" x2="16" y2="22"/>
            </svg>
            <span class="label">{{ item.label }}</span>
          </router-link>
        </nav>
      </div>
    </Transition>

    <div class="tr">
      <button class="win" @click="minimize" title="最小化">
        <svg viewBox="0 0 12 12" width="12" height="12" fill="currentColor">
          <rect x="1.5" y="5.5" width="9" height="1" rx="0.5" />
        </svg>
      </button>
      <button class="win close" @click="closeWindow" title="关闭">
        <svg viewBox="0 0 12 12" width="12" height="12" fill="currentColor">
          <polygon points="1,1 5,5 9,1 11,3 7,7 11,11 9,13 5,9 1,13 -1,11 3,7 -1,3" />
        </svg>
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
  background-color: var(--surface-header);
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
  border-radius: var(--radius-md);
  background-color: rgba(255, 255, 255, 0.9);
}

.back-btn {
  display: flex; align-items: center; gap: 6px;
  padding: 4px 10px;
  border: none; background: transparent;
  color: rgba(255,255,255,0.8);
  cursor: pointer; border-radius: var(--radius-sm);
  font-size: 14px; font-weight: 500;
  transition: all 0.15s;
  outline: none;
}
.back-btn svg { flex-shrink: 0; }
.back-btn:hover { background-color: rgba(255,255,255,0.12); color: #fff; }
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
  color: rgba(255, 255, 255, 0.65);
  text-decoration: none;
  font-size: 13px;
  font-weight: 500;
  transition: all 0.15s;
  outline: none;
}

.nav:hover {
  color: rgba(255, 255, 255, 0.9);
  background-color: rgba(255, 255, 255, 0.12);
}

.nav.active {
  color: #fff;
  background-color: rgba(255, 255, 255, 0.18);
}

.label { font-size: 13px; }

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
  color: rgba(255, 255, 255, 0.6);
  cursor: pointer;
  border-radius: var(--radius-sm);
  transition: all 0.15s;
  outline: none;
}

.win:hover {
  background-color: rgba(255, 255, 255, 0.15);
  color: #fff;
}

.close:hover {
  background-color: rgba(0, 0, 0, 0.25);
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
