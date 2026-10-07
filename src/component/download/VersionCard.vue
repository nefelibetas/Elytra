<script setup lang="ts">
import { computed } from "vue";
import { ChevronDown } from "@lucide/vue";

const props = defineProps<{
  title: string;
  count: number;
  expanded: boolean;
}>();

const emit = defineEmits<{ toggle: [] }>();

// 单行 VersionItem 的保守上界高度（px）：padding 10×2 + 内容块。
// 内容块 = max(32px 图标, ver-id 13px + gap 2 + ver-sub 11px ≈ 38px) ≈ 38px → 实际行高 ≈ 58px。
// 取 80px 预留折行 / 更大行高的余量；上界偏大只会让过渡略快，不会裁切。
const ROW_H = 80;
// 展开目标高度 = 条目数 × 单行上界 + 顶部 1px 分割线与余量。优先保证不裁切（宁可过渡略长）。
const expandHeight = computed(() => props.count * ROW_H + 24 + "px");
</script>

<template>
  <div class="card expandable" :class="{ open: expanded }">
    <div class="card-title" @click="emit('toggle')">
      <span>{{ title }}</span>
      <span class="card-count">{{ count }}</span>
      <ChevronDown class="chevron" :size="14" />
    </div>
    <!-- v-show 而非 v-if：条目可能很多，保持列表常驻避免反复重建；折叠时 display:none 不参与布局 -->
    <Transition name="expand">
      <div v-show="expanded" class="card-body" :style="{ '--expand-h': expandHeight }">
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
