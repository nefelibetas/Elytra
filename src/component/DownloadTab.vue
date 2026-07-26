<script setup lang="ts">
defineProps<{ active?: boolean }>();
const emit = defineEmits<{ click: []; refresh: [] }>();
</script>

<template>
  <div class="tab-row">
    <button class="tab" :class="{ active }" @click="emit('click')">
      <slot name="icon" />
      <span><slot /></span>
    </button>
    <button class="tab-refresh" @click.stop="emit('refresh')" title="刷新">
      <svg viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
        <path d="M14 8a6 6 0 01-11 3.5"/>
        <polygon points="3,14 2.5,10 7,11" stroke="none" fill="currentColor"/>
        <path d="M2 8a6 6 0 0111-3.5"/>
        <polygon points="13,2 13.5,6 9,5" stroke="none" fill="currentColor"/>
      </svg>
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
  transition: all 0.15s;
  opacity: 0;
}

.tab-row:hover .tab-refresh { opacity: 1; }

.tab-refresh:hover {
  color: var(--color-brand);
  background-color: var(--color-brand-bg);
}
</style>
