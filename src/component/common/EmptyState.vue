<script setup lang="ts">
// 通用空态容器：占满父容器完全居中，默认插槽自定义表现（推荐：图标 + 文本，颜色由插槽或 --text-tertiary 决定）。
</script>

<template>
  <div class="es-box">
    <slot />
  </div>
</template>

<style scoped>
.es-box {
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
  color: var(--text-tertiary);
  text-align: center;
  user-select: none;
}

.es-box :deep(p) {
  font-size: 13px;
  line-height: 1.7;
  margin: 0;
}
</style>
