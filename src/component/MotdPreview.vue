<script setup lang="ts">
// MOTD 预览：把含 § 格式码的文本渲染成游戏内效果（16 色/自定义色/粗斜体/下划线/
// 删除线/乱码）。MOTD 生成器与服务器状态检测共用；乱码（§k）无法在浏览器复刻
// 随机跳动，用闪烁模拟并加说明性 title。
import { computed } from "vue";
import { parseMotd } from "../utils/motd";

const props = defineProps<{
  motd: string;
}>();

const segments = computed(() => parseMotd(props.motd));
</script>

<template>
  <div class="motd-preview" :class="{ empty: segments.length === 0 }">
    <template v-if="segments.length > 0">
      <span
        v-for="(seg, i) in segments"
        :key="i"
        class="motd-seg"
        :class="{
          bold: seg.bold,
          italic: seg.italic,
          underline: seg.underline,
          strike: seg.strikethrough,
          obf: seg.obfuscated,
        }"
        :style="seg.color ? { color: seg.color } : undefined"
        :title="seg.obfuscated ? '乱码效果（§k）：游戏内为随机字符跳动' : undefined"
      >{{ seg.text }}</span>
    </template>
    <span v-else class="motd-placeholder">（无文本）</span>
  </div>
</template>

<style scoped>
.motd-preview {
  /* 等宽字体栈：Minecraft 默认字体为等宽像素风格，monospace 最接近且三端一致 */
  font-family: "Cascadia Mono", Consolas, "Courier New", monospace;
  font-size: 14px;
  line-height: 1.7;
  padding: 10px 14px;
  min-height: 24px;
  background-color: rgba(0, 0, 0, 0.35);
  border-radius: var(--radius-sm);
  word-break: break-all;
  white-space: pre-wrap;
}

.motd-preview.empty {
  color: var(--text-tertiary);
}

/* 粗体：加粗 + 轻微阴影模拟 MC 粗体描边 */
.bold {
  font-weight: 700;
  text-shadow: 1px 1px 0 rgba(0, 0, 0, 0.4);
}

.italic {
  font-style: italic;
}

.underline {
  text-decoration: underline;
}

.strike {
  text-decoration: line-through;
}

/* 乱码：闪烁模拟（游戏内为随机字符跳动，无法复刻） */
.obf {
  animation: motd-obf 1.2s steps(1) infinite;
}

@keyframes motd-obf {
  0%, 60% { opacity: 1; }
  60.01%, 100% { opacity: 0.15; }
}

.motd-placeholder {
  font-style: italic;
  opacity: 0.6;
}
</style>
