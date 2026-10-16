<script setup lang="ts">
// 启动步骤状态标记（仅启动面板使用）：完成打勾 / 进行中脉冲点 / 未开始灰点。进行中不做环形滚动（与启动语义不符）
import { Check } from "@lucide/vue";
import type { StepStatus } from "../../store/task/download";

defineProps<{ status: StepStatus }>();
</script>

<template>
  <span class="sd-icon">
    <Check v-if="status === 'done'" :size="13" :stroke-width="2" />
    <span v-else-if="status === 'running'" class="sd-dot sd-pulse"></span>
    <span v-else class="sd-dot"></span>
  </span>
</template>

<style scoped>
.sd-icon {
  flex: none;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 16px;
  height: 16px;
  color: var(--color-success);
}

.sd-dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background-color: var(--text-tertiary);
  display: block;
}

/* 进行中：品牌色呼吸脉冲（用缩放/透明度，不用旋转环或外圈描边） */
.sd-pulse {
  background-color: var(--color-brand);
  animation: sd-pulse 1.4s ease-in-out infinite;
}

@keyframes sd-pulse {
  0%, 100% { transform: scale(1);   opacity: 1; }
  50%      { transform: scale(1.5); opacity: 0.45; }
}
</style>
