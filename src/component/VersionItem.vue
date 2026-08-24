<script setup lang="ts">
defineProps<{
  iconSrc: string;
  versionId: string;
  subtitle: string;
  badge?: string;
  badgeClass?: string;
}>();

const emit = defineEmits<{ select: [] }>();
</script>

<template>
  <div class="ver-item" @click="emit('select')">
    <img :src="iconSrc" width="32" height="32" class="ver-icon" alt="" />
    <div class="ver-info">
      <div v-if="badge" class="ver-info-top">
        <span class="ver-id">{{ versionId }}</span>
        <span class="ver-badge" :class="badgeClass">{{ badge }}</span>
      </div>
      <span v-else class="ver-id">{{ versionId }}</span>
      <span class="ver-sub">{{ subtitle }}</span>
    </div>
    <!-- 右侧操作区：由使用场景按需组合（hover 显示） -->
    <div class="ver-actions">
      <slot name="action" />
    </div>
  </div>
</template>

<style scoped>
.ver-item {
  display: flex; align-items: center; gap: 12px;
  padding: 10px 16px;
  font-size: 13px;
  transition: background 0.1s;
  user-select: none;
  cursor: pointer;
}

.ver-item:hover { background-color: var(--surface-hover); }

.ver-icon { flex-shrink: 0; border-radius: 3px; }

.ver-info {
  flex: 1;
  display: flex;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
}

.ver-info-top {
  display: flex;
  align-items: center;
  gap: 6px;
}

.ver-id { font-weight: 600; color: var(--text-primary); font-size: 13px; }

.ver-sub { font-size: 11px; color: var(--text-tertiary); }

.ver-badge {
  font-size: 10px; font-weight: 500;
  padding: 1px 6px; border-radius: 8px;
  white-space: nowrap;
}

.ver-badge[class*="old"] { color: var(--color-badge-alt); background-color: var(--color-badge-alt-bg); }

.ver-actions {
  flex-shrink: 0;
  display: flex;
  align-items: center;
  gap: 4px;
  opacity: 0;
  pointer-events: none; /* 隐藏态不拦截整行点击 */
  transition: opacity 0.12s;
}

.ver-item:hover .ver-actions {
  opacity: 1;
  pointer-events: auto;
}
</style>
