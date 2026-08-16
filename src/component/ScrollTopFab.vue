<script setup lang="ts">
// 长列表滚动回顶 FAB：自动绑定最近的滚动祖先（overflow-y: auto/scroll 且内容可滚动），
// 滚动超过阈值出现、回到顶部附近隐藏，带出现/隐藏动画。
// 位置固定右下 24px（任务中心已为顶级路由，无全局 DownloadFAB 需避让）。
import { onBeforeUnmount, onMounted, ref } from "vue";
import { ArrowUp } from "@lucide/vue";

const visible = ref(false);
const THRESHOLD = 300;

const wrap = ref<HTMLElement | null>(null);
let container: HTMLElement | null = null;

function findScrollParent(node: HTMLElement | null): HTMLElement | null {
  let cur = node;
  while (cur) {
    const style = getComputedStyle(cur);
    const scrollable =
      style.overflowY === "auto" || style.overflowY === "scroll" ||
      style.overflow === "auto" || style.overflow === "scroll";
    if (scrollable && cur.scrollHeight > cur.clientHeight) return cur;
    cur = cur.parentElement;
  }
  return null;
}

function onScroll() {
  visible.value = (container?.scrollTop ?? 0) > THRESHOLD;
}

function scrollTop() {
  container?.scrollTo({ top: 0, behavior: "smooth" });
}

onMounted(() => {
  container = findScrollParent(wrap.value);
  container?.addEventListener("scroll", onScroll, { passive: true });
  onScroll();
});

onBeforeUnmount(() => {
  container?.removeEventListener("scroll", onScroll);
});
</script>

<template>
  <div ref="wrap" class="scroll-top-fab-wrap">
    <Transition name="fab-up">
      <button v-if="visible" class="scroll-top-fab" @click="scrollTop" v-tooltip="'回到顶部'">
        <ArrowUp :size="16" />
      </button>
    </Transition>
  </div>
</template>

<style scoped>
.scroll-top-fab-wrap { display: contents; }
.scroll-top-fab {
  position: fixed; z-index: 998;
  bottom: 24px; right: 24px;
  width: 40px; height: 40px;
  border: 1px solid var(--border-default); border-radius: 50%;
  background: var(--surface-card);
  box-shadow: var(--shadow-lg);
  cursor: pointer;
  display: flex; align-items: center; justify-content: center;
  color: var(--text-secondary);
  transition: color 0.15s, box-shadow 0.15s, transform 0.15s;
}
.scroll-top-fab:hover {
  color: var(--color-brand);
  box-shadow: 0 6px 24px var(--color-brand-glow);
  transform: scale(1.05);
}
.scroll-top-fab:active { transform: scale(0.95); }

.fab-up-enter-active { transition: all 0.25s ease-out; }
.fab-up-leave-active { transition: all 0.2s ease-in; }
.fab-up-enter-from { opacity: 0; transform: translateY(16px) scale(0.8); }
.fab-up-leave-to { opacity: 0; transform: translateY(16px) scale(0.8); }
</style>
