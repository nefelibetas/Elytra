<script setup lang="ts">
// 图标/图片占位组件：加载前显示原版草方块图标，加载完成淡入真实图片，失败保持占位；由独立 Image() 探针驱动（先绑监听再赋 src）。
// Modrinth CDN 图标失败率高（307 目标节点概率性黑洞）；探针 6s 超时 + 重试 2 次（共 3 次尝试）后仍失败才显示占位。
import { onBeforeUnmount, ref, watch } from "vue";
import { Box } from "@lucide/vue";

const props = defineProps<{ src?: string | null; size?: number }>();

const loaded = ref(false);
const failed = ref(false);

const PROBE_TIMEOUT_MS = 6000;
const PROBE_RETRIES = 2;

// 探针取消：src 变化或卸载时 token 自增，旧回调仅 token 匹配才写状态（否则旧图超时结果覆盖新图，致翻页图标错乱）。
let probeToken = 0;
let probeTimer: ReturnType<typeof setTimeout> | null = null;

function cancelProbe() {
  probeToken++;
  if (probeTimer !== null) {
    clearTimeout(probeTimer);
    probeTimer = null;
  }
}

function clearProbeTimer() {
  if (probeTimer !== null) {
    clearTimeout(probeTimer);
    probeTimer = null;
  }
}

watch(
  () => props.src,
  (src) => {
    cancelProbe();
    loaded.value = false;
    failed.value = false;
    if (!src) return;
    const token = probeToken;
    let attempt = 0;
    const tryLoad = () => {
      if (token !== probeToken) return;
      const probe = new Image();
      probeTimer = setTimeout(() => {
        probeTimer = null;
        if (token !== probeToken) return;
        if (attempt < PROBE_RETRIES) {
          attempt++;
          tryLoad();
        } else {
          failed.value = true;
        }
      }, PROBE_TIMEOUT_MS);
      probe.onload = () => {
        if (token !== probeToken) return;
        clearProbeTimer();
        loaded.value = true;
      };
      probe.onerror = () => {
        if (token !== probeToken) return;
        clearProbeTimer();
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

onBeforeUnmount(cancelProbe);
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
