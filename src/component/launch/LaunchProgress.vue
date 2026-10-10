<script setup lang="ts">
// 启动进度面板（启动时替换左侧内容，游戏窗口出现后切回）
import StepDot from "../common/StepDot.vue";
import { launchState } from "../../store/app/launch";
import { selectedVersion } from "../../store/app/selectedVersion";
</script>

<template>
  <div class="lp-panel">
    <div class="lp-head">
      <div class="lp-title">正在启动游戏</div>
      <div class="lp-version">{{ selectedVersion }}</div>
    </div>
    <div class="lp-steps">
      <div v-for="(st, i) in launchState.steps" :key="i" class="lp-step" :class="st.status">
        <StepDot :status="st.status" />
        <span class="lp-name">{{ st.name }}</span>
      </div>
    </div>
    <div v-if="launchState.phase === 'done'" class="lp-done">启动完成，游戏窗口即将出现</div>
  </div>
</template>

<style scoped>
.lp-panel {
  flex: 1;
  display: flex;
  flex-direction: column;
  justify-content: center;
  gap: 24px;
  padding: 20px 8px;
}

.lp-head {
  text-align: center;
}

.lp-title {
  font-size: 17px;
  font-weight: 600;
  color: var(--text-primary);
}

.lp-version {
  margin-top: 4px;
  font-size: 12px;
  color: var(--text-tertiary);
}

.lp-steps {
  display: flex;
  flex-direction: column;
  gap: 14px;
  padding: 0 20px;
}

.lp-step {
  display: flex;
  align-items: center;
  gap: 12px;
  font-size: 13px;
  color: var(--text-tertiary);
}

.lp-step.running .lp-name {
  color: var(--color-brand);
  font-weight: 500;
}

.lp-step.done {
  color: var(--text-secondary);
}

.lp-done {
  text-align: center;
  font-size: 13px;
  color: var(--color-success);
  font-weight: 500;
}
</style>
