<script setup lang="ts">
import { ref, watch } from "vue";
import { ChevronDown } from "@lucide/vue";

const props = defineProps<{
  title: string;
  count: number;
  expanded: boolean;
}>();

const emit = defineEmits<{ toggle: [] }>();

// 惰性挂载：折叠组首次展开前不渲染条目，展开后持续挂载（配合 v-show），避免大列表拖慢首帧与反复重建。
const everExpanded = ref(props.expanded);
watch(
  () => props.expanded,
  (v) => {
    if (v) everExpanded.value = true;
  }
);

// 高度取真实 scrollHeight（不按 count 估算：估计值可达数万 px，max-height 过渡会提前跑完）；before-enter / enter 各量一次，enter 在同帧内（enter-to 已加、未绘制）补齐真实高度。
function measure(el: Element) {
  const h = el as HTMLElement;
  if (h.scrollHeight > 0) h.style.setProperty("--expand-h", `${h.scrollHeight}px`);
}
</script>

<template>
  <div class="card expandable" :class="{ open: expanded }">
    <div class="card-title" @click="emit('toggle')">
      <span>{{ title }}</span>
      <span class="card-count">{{ count }}</span>
      <ChevronDown class="chevron" :size="14" />
    </div>
    <Transition name="expand" @before-enter="measure" @enter="measure" @before-leave="measure">
      <div v-if="everExpanded" v-show="expanded" class="card-body">
        <slot />
      </div>
    </Transition>
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
  border-radius: 0 0 var(--radius-md) var(--radius-md);
}
/* 标题与 items 之间的分割线：左右留白 16px */
.card-body::before {
  content: "";
  display: block;
  margin: 0 16px;
  border-top: 1px solid var(--border-default);
}

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
