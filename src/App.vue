<script setup lang="ts">
import { onMounted, onUnmounted } from "vue";
import TitleBar from "./component/TitleBar.vue";

function onKeyDown(e: KeyboardEvent) {
  if (e.key === "F12" || (e.ctrlKey && e.shiftKey && ["I", "J", "C"].includes(e.key.toUpperCase())) || (e.ctrlKey && e.key.toUpperCase() === "U")) {
    e.preventDefault();
  }
}

onMounted(() => {
  document.addEventListener("contextmenu", (e) => e.preventDefault());
  document.addEventListener("keydown", onKeyDown);
});

onUnmounted(() => {
  document.removeEventListener("keydown", onKeyDown);
});
</script>

<template>
  <div class="app-layout">
    <TitleBar />
    <main class="main-content">
      <router-view v-slot="{ Component }">
        <Transition name="route" mode="out-in">
          <component :is="Component" />
        </Transition>
      </router-view>
    </main>
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
}

#app {
  height: 100%;
  width: 100%;
}

:root {
  --titlebar-h: 48px;
  font-family: Inter, Avenir, Helvetica, Arial, sans-serif;
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
  background-color: var(--surface-page);
}

.route-enter-active, .route-leave-active { transition: opacity 0.25s ease; }
.route-enter-from, .route-leave-to { opacity: 0; }
</style>
