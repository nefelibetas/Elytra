<script setup lang="ts">
// 通用错误态：固定错误图标 + 危险色文本，占满父容器完全居中（与加载/空态布局一致）。
import { AlertCircle } from "@lucide/vue";

defineProps<{ text?: string }>();
</script>

<template>
  <div class="er-box">
    <AlertCircle :size="40" :stroke-width="1.5" />
    <p v-if="text">{{ text }}</p>
    <slot v-else />
  </div>
</template>

<style scoped>
.er-box {
  flex: 1;
  height: 100%;
  min-height: 120px;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 14px;
  padding: 20px;
  box-sizing: border-box;
  color: var(--color-danger);
  text-align: center;
  user-select: none;
}

.er-box :deep(p) {
  font-size: 13px;
  line-height: 1.7;
  margin: 0;
}
</style>
