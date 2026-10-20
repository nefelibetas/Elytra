<script setup lang="ts">
// 启动进度面板（启动时替换左侧内容，游戏窗口出现后切回）：顶部启动动画 + 步骤 + 底部取消启动
import { useI18n } from "vue-i18n";
import { Rocket, X } from "@lucide/vue";
import StepDot from "../common/StepDot.vue";
import { launchState, cancelLaunch } from "../../store/app/launch";
import { selectedVersion } from "../../store/app/selectedVersion";

const { t } = useI18n();
</script>

<template>
  <div class="lp-panel">
    <div class="lp-main">
      <div class="lp-head">
        <div class="lp-title">{{ t("launch.progress.title") }}</div>
        <div class="lp-version">{{ selectedVersion }}</div>
      </div>

      <!-- 启动指示：火箭上浮 + 尾焰脉冲（线性动作，不用环形滚动，避免与「加载中」语义混淆） -->
      <div class="lp-hero" :class="{ done: launchState.phase === 'done' }">
        <Rocket class="lp-rocket" :size="34" :stroke-width="1.6" />
        <div class="lp-exhaust"><i></i><i></i><i></i></div>
      </div>

      <div class="lp-steps">
        <div v-for="(st, i) in launchState.steps" :key="i" class="lp-step" :class="st.status">
          <StepDot :status="st.status" />
          <span class="lp-name">{{ t(st.key) }}</span>
        </div>
      </div>

      <div v-if="launchState.phase === 'done'" class="lp-done">{{ t("launch.progress.done") }}</div>
    </div>

    <button v-if="launchState.phase === 'running'" class="lp-cancel" @click="cancelLaunch">
      <X :size="14" />
      <span>{{ t("launch.progress.cancel") }}</span>
    </button>
  </div>
</template>

<style scoped>
.lp-panel {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
  padding: 16px 8px 0;
}

.lp-main {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
  justify-content: center;
  gap: 22px;
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

/* ── 启动动画 ── */
.lp-hero {
  height: 92px;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 7px;
  color: var(--color-brand);
}

.lp-rocket {
  /* translateY 在前、rotate 在后：位移走屏幕坐标轴（竖直上浮），图标朝向固定斜向上 */
  animation: lp-lift 1.9s ease-in-out infinite alternate;
}

@keyframes lp-lift {
  from { transform: translateY(3px) rotate(-45deg); }
  to   { transform: translateY(-6px) rotate(-45deg); }
}

/* 尾焰：三段依次脉冲并向下消散 */
.lp-exhaust {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 3px;
}

.lp-exhaust i {
  width: 14px;
  height: 3px;
  border-radius: 2px;
  background: currentColor;
  animation: lp-thrust 1.15s ease-in-out infinite;
}

.lp-exhaust i:nth-child(2) { width: 11px; animation-delay: 0.14s; }
.lp-exhaust i:nth-child(3) { width: 8px; animation-delay: 0.28s; }

@keyframes lp-thrust {
  0%   { opacity: 0.12; transform: translateY(-2px) scaleX(0.7); }
  45%  { opacity: 1;    transform: translateY(0) scaleX(1); }
  100% { opacity: 0.08; transform: translateY(5px) scaleX(0.55); }
}

/* 窗口已出现：停机（喷焰消失、图标静止留在上方） */
.lp-hero.done .lp-rocket {
  animation: none;
  transform: translateY(-6px) rotate(-45deg);
  opacity: 0.55;
}

.lp-hero.done .lp-exhaust { display: none; }

/* ── 步骤 ── */
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

/* ── 底部取消（面板最下方）── */
.lp-cancel {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 6px;
  width: 100%;
  padding: 9px 0;
  margin-top: 12px;
  border: 1px solid var(--border-strong);
  border-radius: var(--radius-md);
  background-color: var(--surface-elevated);
  color: var(--text-secondary);
  font-size: 13px;
  font-weight: 500;
  cursor: pointer;
  transition: border-color 0.15s, color 0.15s, background-color 0.15s;
}

.lp-cancel:hover {
  border-color: var(--color-danger);
  color: var(--color-danger);
  background-color: var(--color-danger-bg);
}
</style>
