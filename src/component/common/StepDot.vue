<script setup lang="ts">
import { Check } from "@lucide/vue";
import type { StepStatus } from "../../store/task/download";

defineProps<{ status: StepStatus }>();
</script>

<template>
  <span class="sd-icon">
    <Check v-if="status === 'done'" :size="13" :stroke-width="2" />
    <span v-else-if="status === 'running'" class="sd-spin"></span>
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

.sd-spin {
  width: 12px;
  height: 12px;
  border: 2px solid var(--border-strong);
  border-top-color: var(--text-secondary);
  border-radius: 50%;
  display: block;
  animation: sd-rotate 0.7s linear infinite;
}

@keyframes sd-rotate {
  to { transform: rotate(360deg); }
}
</style>
