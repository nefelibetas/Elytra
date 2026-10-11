<script setup lang="ts">
import { ref, watch } from "vue";
import { ChevronDown } from "@lucide/vue";

const props = defineProps<{
  title: string;
  count: number;
  expanded: boolean;
}>();

const emit = defineEmits<{ toggle: [] }>();

// 惰性挂载：折叠分组在**首次展开前不渲染任何条目**（原版 manifest 全量 1000+
// 条，四组折叠时全部常驻 DOM 会让下载页首帧创建上千节点）。首次展开后保持挂载
// （配合 v-show 切换 display），避免反复重建列表。
const everExpanded = ref(props.expanded);
watch(
  () => props.expanded,
  (v) => {
    if (v) everExpanded.value = true;
  }
);

// 展开高度用真实内容高度驱动（过渡前后各量一次），不再按 count × ROW_H 估算——
// 估算值在 snapshots 组可达数万 px，max-height 过渡会提前"跑完"（后半段无视觉变化）
// 且过渡期间仍要为大列表布局。
// before-enter / enter 都量一次：before-enter 时元素尚未布局（scrollHeight=0）也无妨，
// enter 钩子在同一帧（enter-to 已加、尚未绘制）补齐真实高度。
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
  /* 底部圆角贴合 .card 的 border-radius：列表最后一项 hover 的直角背景
     被 overflow 裁剪时不再"顶着"卡片圆角形成方角 */
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
