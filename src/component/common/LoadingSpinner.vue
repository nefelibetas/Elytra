<script setup lang="ts">
// 通用加载态：环形加载器 + 可选说明文字，占满父容器完全居中。
// 环形样式与搜索结果组件一致（36px 品牌色）。
defineProps<{ text?: string }>();
</script>

<template>
  <div class="ls-box">
    <div class="ls-spinner"></div>
    <p v-if="text" class="ls-text">{{ text }}</p>
  </div>
</template>

<style scoped>
.ls-box {
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
}

.ls-spinner {
  width: 36px;
  height: 36px;
  border: 3px solid var(--border-default);
  border-top-color: var(--color-brand);
  border-radius: 50%;
  animation: ls-rotate 0.7s linear infinite;
}

.ls-text {
  font-size: 13px;
  color: var(--text-tertiary);
  user-select: none;
}

@keyframes ls-rotate {
  to {
    transform: rotate(360deg);
  }
}
</style>
