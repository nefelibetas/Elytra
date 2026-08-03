<script setup lang="ts">
import { onMounted, onUnmounted } from "vue";
import { useRouter } from "vue-router";
import TitleBar from "./component/TitleBar.vue";
import DownloadFAB from "./component/DownloadFAB.vue";
import { dlState, ensureListen } from "./store/download";
import { startMemoryMonitor } from "./store/memory";
import { toastState, showToast } from "./utils/toast";

const router = useRouter();

dlState.onTaskDone = (taskName: string, prevRoute: string) => {
  showToast(`${taskName} 已完成`);
  if (prevRoute) router.push(prevRoute);
};

function onKeyDown(e: KeyboardEvent) {
  // 禁止打开开发者工具：F12 / Ctrl+Shift+I/J/C / Ctrl+U
  if (
    e.key === "F12" ||
    (e.ctrlKey && e.shiftKey && ["I", "J", "C"].includes(e.key.toUpperCase())) ||
    (e.ctrlKey && e.key.toUpperCase() === "U")
  ) {
    e.preventDefault();
    return;
  }
  // 禁止 Tab 切换控件焦点
  if (e.key === "Tab") {
    e.preventDefault();
    return;
  }
  // 禁止全选（Ctrl+A）
  if (e.ctrlKey && e.key.toLowerCase() === "a") {
    e.preventDefault();
    return;
  }
  // ESC：非启动页面且无弹窗时回到启动页
  if (e.key === "Escape" && !document.querySelector(".dialog-overlay")) {
    const path = router.currentRoute.value.path;
    if (path !== "/") router.push("/");
  }
}

function onContextMenu(e: MouseEvent) {
  e.preventDefault();
}

onMounted(() => {
  document.addEventListener("contextmenu", onContextMenu);
  document.addEventListener("keydown", onKeyDown);
  ensureListen();
  // 应用启动即持续监听系统内存（设置页/启动页共用）
  startMemoryMonitor();
});

onUnmounted(() => {
  document.removeEventListener("contextmenu", onContextMenu);
  document.removeEventListener("keydown", onKeyDown);
});
</script>

<template>
  <div class="app-layout">
    <TitleBar />
    <DownloadFAB />
    <main class="main-content">
      <router-view v-slot="{ Component, route }">
        <Transition :name="route.meta.transition ?? 'route'" mode="out-in">
          <component :is="Component" />
        </Transition>
      </router-view>
    </main>
    <Transition name="toast">
      <div v-if="toastState.msg" class="toast">{{ toastState.msg }}</div>
    </Transition>
  </div>
</template>

<style>
html, body {
  margin: 0;
  padding: 0;
  overflow: hidden;
  overscroll-behavior-x: none;
  background: transparent;
  height: 100%;
  width: 100%;
  /* 全局禁止选中（输入框/文本域内例外） */
  -webkit-user-select: none;
  user-select: none;
}

#app {
  height: 100%;
  width: 100%;
}

/* 输入控件保持可选中/可编辑 */
input, textarea {
  -webkit-user-select: text;
  user-select: text;
}

:root {
  --titlebar-h: 48px;
  font-family: Inter, "Segoe UI", "Microsoft YaHei UI", "Microsoft YaHei", "PingFang SC", sans-serif;
  font-size: 14px;
  line-height: 1.5;
  font-weight: 400;
  font-synthesis: none;
  text-rendering: optimizeLegibility;
  -webkit-font-smoothing: antialiased;
  -moz-osx-font-smoothing: grayscale;

  /* brand */
  --color-brand: #6366F1;
  --color-brand-dark: #4F46E5;
  --color-brand-hover: #5558E8;
  --color-brand-bg: rgba(99, 102, 241, 0.08);
  --color-brand-glow: rgba(99, 102, 241, 0.25);
  --color-brand-gradient: linear-gradient(135deg, #6366F1, #818CF8);

  /* text */
  --text-primary: #1E1B4B;
  --text-secondary: #6B7280;
  --text-tertiary: #9CA3AF;
  --text-inverse: #FFFFFF;

  /* surface */
  --surface-page: #F8F7FC;
  --surface-header: var(--color-brand);
  --surface-card: #FFFFFF;
  --surface-elevated: #FFFFFF;
  --surface-hover: rgba(0, 0, 0, 0.03);

  /* border */
  --border-default: #E5E7EB;
  --border-strong: #D1D5DB;

  /* shadow */
  --shadow-sm: 0 1px 2px rgba(30, 27, 75, 0.06);
  --shadow-md: 0 2px 8px rgba(30, 27, 75, 0.08);
  --shadow-lg: 0 4px 16px rgba(30, 27, 75, 0.10);

  /* radius */
  --radius-sm: 6px;
  --radius-md: 8px;
  --radius-lg: 12px;

  /* scrollbar */
  --scrollbar-thumb: #D1D5DB;
}

* {
  margin: 0;
  padding: 0;
  box-sizing: border-box;
}

::-webkit-scrollbar { width: 6px; }
::-webkit-scrollbar-track { background: transparent; }
::-webkit-scrollbar-thumb {
  background: var(--scrollbar-thumb);
  border-radius: 3px;
}

.app-layout {
  display: flex;
  flex-direction: column;
  height: 100vh;
  width: 100vw;
  overflow: hidden;
  border-radius: var(--radius-md);
  background-color: var(--surface-page);
  color: var(--text-primary);
}

.main-content {
  flex: 1;
  overflow-y: auto;
  overflow-x: hidden;
  scrollbar-gutter: stable;
  background-color: var(--surface-page);
}

/* Route transitions — simple opacity fade, normal flow, no scroll jitter */
.route-enter-active,
.route-leave-active,
.slide-left-enter-active,
.slide-left-leave-active,
.slide-right-enter-active,
.slide-right-leave-active {
  transition: opacity 0.18s ease;
}
.route-enter-from,
.slide-left-enter-from,
.slide-right-enter-from { opacity: 0; }
.route-leave-to,
.slide-left-leave-to,
.slide-right-leave-to { opacity: 0; }

.toast {
  position: fixed; bottom: 80px; left: 50%; transform: translateX(-50%);
  z-index: 9999;
  padding: 10px 24px; border-radius: var(--radius-md);
  background: #1E1B4B; color: #fff;
  font-size: 13px; font-weight: 500;
  white-space: nowrap; box-shadow: var(--shadow-lg);
  pointer-events: none;
}
.toast-enter-active { transition: all 0.3s ease-out; }
.toast-leave-active { transition: all 0.2s ease-in; }
.toast-enter-from { opacity: 0; transform: translateX(-50%) translateY(16px); }
.toast-leave-to   { opacity: 0; transform: translateX(-50%) translateY(16px); }
</style>
