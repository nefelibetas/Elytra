<script setup lang="ts">
// 图标/图片占位组件：加载前显示大号原版游戏（草方块）图标占位，
// 加载完成后淡入真实图片；请求失败保持占位。
// 加载状态由独立 Image() 探针驱动（先绑监听再赋 src，事件必达）；
// 实测 Modrinth CDN 图标当前网络失败率高（307 目标节点概率性黑洞）：
// 探针带 6s 超时 + 失败重试 2 次（共 3 次尝试），仍失败才显示占位。
import { ref, watch } from "vue";
import { Box } from "@lucide/vue";

const props = defineProps<{ src?: string | null; size?: number }>();

const loaded = ref(false);
const failed = ref(false);

const PROBE_TIMEOUT_MS = 6000;
const PROBE_RETRIES = 2;

watch(
  () => props.src,
  (src) => {
    loaded.value = false;
    failed.value = false;
    if (!src) return;
    let attempt = 0;
    const tryLoad = () => {
      const probe = new Image();
      const timer = setTimeout(() => {
        if (attempt < PROBE_RETRIES) {
          attempt++;
          tryLoad();
        } else {
          failed.value = true;
        }
      }, PROBE_TIMEOUT_MS);
      probe.onload = () => {
        clearTimeout(timer);
        loaded.value = true;
      };
      probe.onerror = () => {
        clearTimeout(timer);
        if (attempt < PROBE_RETRIES) {
          attempt++;
          tryLoad();
        } else {
          failed.value = true;
        }
      };
      probe.src = src;
    };
    tryLoad();
  },
  { immediate: true }
);
</script>

<template>
  <span class="icon-box" :style="{ width: (size ?? 32) + 'px', height: (size ?? 32) + 'px' }">
    <!-- 占位：outline 草方块（与左侧"原版游戏"tab 同款描边风格），无背景；
         加载成功后必须移除（v-if）——透明背景 PNG 会透出下层占位 -->
    <Box
      v-if="!loaded && !failed"
      class="icon-ph"
      :stroke-width="1.2"
    />
    <img
      v-if="src && !failed"
      :src="src"
      :width="size ?? 32"
      :height="size ?? 32"
      :class="{ loaded }"
      alt=""
      decoding="async"
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
