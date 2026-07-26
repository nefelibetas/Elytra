<script setup lang="ts">
defineProps<{
  iconSrc: string;
  versionId: string;
  subtitle: string;
  badge?: string;
  badgeClass?: string;
}>();

const emit = defineEmits<{ action: [] }>();
</script>

<template>
  <div class="ver-item">
    <img :src="iconSrc" width="32" height="32" class="ver-icon" alt="" />
    <div class="ver-info">
      <div v-if="badge" class="ver-info-top">
        <span class="ver-id">{{ versionId }}</span>
        <span class="ver-badge" :class="badgeClass">{{ badge }}</span>
      </div>
      <span v-else class="ver-id">{{ versionId }}</span>
      <span class="ver-sub">{{ subtitle }}</span>
    </div>
    <button class="ver-action" title="下载服务端" @click="emit('action')">
      <svg viewBox="0 0 16 16" width="16" height="16" fill="currentColor">
        <rect x="3" y="1" width="10" height="2" rx="1" />
        <rect x="4" y="3" width="8" height="8" rx="1" />
        <rect x="5" y="5" width="6" height="1" />
        <rect x="5" y="7" width="4" height="1" />
        <polygon points="8,11 10.5,8.5 12,10 8,14 4,10 5.5,8.5" />
      </svg>
    </button>
  </div>
</template>

<style scoped>
.ver-item {
  display: flex; align-items: center; gap: 12px;
  padding: 10px 16px;
  font-size: 13px;
  transition: background 0.1s;
  user-select: none;
}

.ver-item:hover { background-color: var(--surface-hover); }

.ver-item + .ver-item { border-top: 1px solid var(--border-default); }

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

.ver-badge[class*="old"] { color: #7c3aed; background-color: rgba(124, 58, 237, 0.1); }

.ver-action {
  flex-shrink: 0;
  width: 32px; height: 32px;
  display: flex; align-items: center; justify-content: center;
  border: none; background: transparent;
  color: var(--text-tertiary);
  cursor: pointer;
  border-radius: var(--radius-sm);
  transition: all 0.15s;
  opacity: 0;
}

.ver-item:hover .ver-action { opacity: 1; }

.ver-action:hover {
  background-color: var(--color-brand-bg);
  color: var(--color-brand);
}
</style>
