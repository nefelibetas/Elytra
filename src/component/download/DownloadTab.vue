<script setup lang="ts">
import { RefreshCw } from "@lucide/vue";
defineProps<{ active?: boolean }>();
const emit = defineEmits<{ click: []; refresh: [] }>();
</script>

<template>
  <div class="tab-row">
    <button class="tab" :class="{ active }" @click="emit('click')">
      <slot name="icon" />
      <span><slot /></span>
    </button>
    <button class="tab-refresh" @click.stop="emit('refresh')" v-tooltip="'刷新'">
      <RefreshCw :size="14" />
    </button>
  </div>
</template>

<style scoped>
.tab-row {
  display: flex;
  align-items: center;
  gap: 0;
  position: relative;
}

.tab {
  flex: 1;
  display: flex; align-items: center; gap: 8px;
  padding: 8px 36px 8px 12px; border-radius: var(--radius-sm);
  border: none; background: transparent;
  color: var(--text-secondary); font-size: 13px; font-weight: 500;
  cursor: pointer; transition: all 0.15s;
  text-align: left;
}

.tab:hover { color: var(--text-primary); background-color: var(--surface-hover); }
.tab.active { color: var(--color-brand); background-color: var(--color-brand-bg); }

.tab-row:hover .tab:not(.active) {
  color: var(--text-primary);
  background-color: var(--surface-hover);
}

.tab-refresh {
  position: absolute;
  right: 4px;
  width: 28px; height: 28px;
  display: flex; align-items: center; justify-content: center;
  border: none; background: transparent;
  color: var(--text-tertiary);
  cursor: pointer;
  border-radius: var(--radius-sm);
  transition: opacity 0.15s, visibility 0.15s;
  opacity: 0;
  visibility: hidden; /* 隐藏态移出命中测试（WebView2 pointer-events 过渡不可靠） */
}

.tab-row:hover .tab-refresh {
  opacity: 1;
  visibility: visible;
}
.tab-refresh:hover {
  color: var(--color-brand);
  background-color: var(--color-brand-bg);
}
</style>
