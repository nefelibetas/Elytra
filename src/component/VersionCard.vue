<script setup lang="ts">
import { computed } from "vue";
import { ChevronDown } from "@lucide/vue";

const props = defineProps<{
  title: string;
  count: number;
  expanded: boolean;
}>();

const emit = defineEmits<{ toggle: [] }>();

const bodyHeight = computed(() => {
  if (!props.expanded) return "0";
  return Math.min(props.count * 100 + 24, 99999) + "px";
});
</script>

<template>
  <div class="card expandable" :class="{ open: expanded }">
    <div class="card-title" @click="emit('toggle')">
      <span>{{ title }}</span>
      <span class="card-count">{{ count }}</span>
      <ChevronDown class="chevron" :size="14" />
    </div>
    <div class="card-body" :style="{ maxHeight: bodyHeight }">
      <slot />
    </div>
  </div>
</template>

<style scoped>
.card {
  background-color: var(--surface-card);
  border: 1px solid var(--border-default);
  border-radius: var(--radius-md);
}

.card-title {
  display: flex; align-items: center; gap: 8px;
  padding: 12px 16px;
  font-size: 14px; font-weight: 600;
  color: var(--text-primary);
  user-select: none;
  outline: none;
  cursor: pointer;
}

.card-body {
  overflow: hidden;
  /* 底部圆角贴合 .card 的 border-radius：列表最后一项 hover 的直角背景
     被 overflow 裁剪时不再"顶着"卡片圆角形成方角 */
  border-radius: 0 0 var(--radius-md) var(--radius-md);
  transition: max-height 0.3s ease, opacity 0.25s ease;
  opacity: 1;
}
/* 标题与 items 之间的分割线：左右留白 16px */
.card-body::before {
  content: "";
  display: block;
  margin: 0 16px;
  border-top: 1px solid var(--border-default);
}
.card:not(.open) .card-body { opacity: 0; }

.expandable:hover { box-shadow: var(--shadow-sm); }

.card-count {
  font-size: 12px; font-weight: 500;
  color: var(--text-tertiary);
  margin-left: auto;
}

.chevron {
  color: var(--text-tertiary);
  transition: transform 0.2s;
}

.expandable.open .chevron { transform: rotate(180deg); }
</style>
