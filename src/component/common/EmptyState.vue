<script setup lang="ts">
// 通用空态容器：占满父容器完全居中，默认插槽允许自定义表现。
// 推荐调用形式：图标 + 文本（图标/文本颜色由插槽内容或继承默认 --text-tertiary 决定）。
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
