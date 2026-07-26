<script setup lang="ts">
import { useRoute } from "vue-router";
import { getCurrentWindow } from "@tauri-apps/api/window";

const route = useRoute();
const appWindow = getCurrentWindow();

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
</script>

<template>
  <header class="titlebar" @mousedown="onMouseDown">
    <div class="tl">
      <span class="logo">E</span>
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
        <svg v-if="item.icon === 'settings'" viewBox="0 0 16 16" width="16" height="16" fill="currentColor">
          <path d="M8 1a1 1 0 01.5.18l.7.56a2.8 2.8 0 001.38.54l.88.08a.8.8 0 01.67.67l.08.88a2.8 2.8 0 00.54 1.38l.56.7a.8.8 0 010 1l-.56.7a2.8 2.8 0 00-.54 1.38l-.08.88a.8.8 0 01-.67.67l-.88.08a2.8 2.8 0 00-1.38.54l-.7.56a.8.8 0 01-1 0l-.7-.56a2.8 2.8 0 00-1.38-.54l-.88-.08a.8.8 0 01-.67-.67l-.08-.88a2.8 2.8 0 00-.54-1.38l-.56-.7a.8.8 0 010-1l.56-.7a2.8 2.8 0 00.54-1.38l.08-.88a.8.8 0 01.67-.67l.88-.08a2.8 2.8 0 001.38-.54l.7-.56A1 1 0 018 1zm0 3.2a3.2 3.2 0 100 6.4 3.2 3.2 0 000-6.4z" />
        </svg>
        <span class="label">{{ item.label }}</span>
      </router-link>
    </nav>

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

.tl { display: flex; align-items: center; width: 100px; }

.logo {
  font-size: 18px;
  font-weight: 800;
  color: var(--color-brand);
  letter-spacing: -1px;
  width: 30px;
  height: 30px;
  display: flex;
  align-items: center;
  justify-content: center;
  border-radius: var(--radius-md);
  background-color: rgba(255, 255, 255, 0.9);
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
</style>
