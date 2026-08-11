<script setup lang="ts">
// 图标/图片占位组件：加载前显示大号原版游戏（草方块）图标占位，
// 加载完成后淡入真实图片；请求失败保持占位。
// 占位用 fill 矢量（无背景色块、无描边缩放毛边，任意尺寸锐利）
import { ref } from "vue";

defineProps<{ src?: string | null; size?: number }>();

const loaded = ref(false);
const failed = ref(false);
</script>

<template>
  <span class="icon-box" :style="{ width: (size ?? 32) + 'px', height: (size ?? 32) + 'px' }">
    <!-- 占位：outline 草方块（与左侧"原版游戏"tab 同款描边风格），无背景 -->
    <svg class="icon-ph" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.2" stroke-linecap="round" stroke-linejoin="round">
      <path d="M8 1.5L14 5l-6 3.5L2 5z" />
      <path d="M2 5v6l6 3.5V8.5" />
      <path d="M14 5v6l-6 3.5V8.5" />
    </svg>
    <img
      v-if="src && !failed"
      :src="src"
      :width="size ?? 32"
      :height="size ?? 32"
      :class="{ loaded }"
      alt=""
      loading="lazy"
      decoding="async"
      @load="loaded = true"
      @error="failed = true"
    />
  </span>
</template>

<style scoped>
.icon-box {
  flex-shrink: 0;
  position: relative;
  display: inline-block;
  color: var(--text-tertiary);
}

.icon-ph {
  position: absolute;
  inset: 0;
  margin: auto;
  width: 100%;
  height: 100%;
  padding: 4%;
  box-sizing: border-box;
  opacity: 0.75;
}

img {
  position: absolute;
  inset: 0;
  width: 100%;
  height: 100%;
  object-fit: cover;
  opacity: 0;
  transition: opacity 0.2s ease;
}

img.loaded {
  opacity: 1;
}
</style>
