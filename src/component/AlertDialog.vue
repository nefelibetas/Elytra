<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, useSlots } from "vue";

const props = withDefaults(defineProps<{
  title?: string;
  message?: string;
  /** 是否显示「复制错误信息」按钮（仅复杂错误需要） */
  copyable?: boolean;
  /** body 与 actions 之间是否显示分割线 */
  divider?: boolean;
  modelValue: boolean;
}>(), {
  title: "提示",
  message: "",
  copyable: false,
  divider: true,
});

const emit = defineEmits<{
  "update:modelValue": [value: boolean];
  confirm: [];
}>();

const copied = ref(false);
const slots = useSlots();

/** 正文只显示第一行（友好短提示），详细技术信息由复制按钮带走。 */
const shortMessage = computed(() => props.message.split("\n")[0] || "");

/** footer 是否有内容：有复制按钮或外部 actions slot。无内容时不渲染 footer，取消分割线。 */
const hasFooter = computed(
  () => !!slots.actions || (props.message !== "" && props.copyable),
);

function close() {
  emit("update:modelValue", false);
  emit("confirm");
}

/** 复制错误信息到剪贴板（WebView2 下 clipboard API 可用，失败时降级 execCommand）。 */
async function copyMessage() {
  const text = props.message || props.title || "";
  if (!text) return;
  try {
    await navigator.clipboard.writeText(text);
  } catch {
    const ta = document.createElement("textarea");
    ta.value = text;
    ta.style.position = "fixed";
    ta.style.opacity = "0";
    document.body.appendChild(ta);
    ta.select();
    document.execCommand("copy");
    document.body.removeChild(ta);
  }
  copied.value = true;
  setTimeout(() => (copied.value = false), 1500);
  // 复制成功后自动关闭弹窗
  close();
}

function onKeydown(e: KeyboardEvent) {
  if (e.key === "Escape") close();
}

onMounted(() => document.addEventListener("keydown", onKeydown));
onUnmounted(() => document.removeEventListener("keydown", onKeydown));
</script>

<template>
  <Transition name="dialog">
    <div v-if="modelValue" class="dialog-overlay" @click.self="close">
      <div class="dialog-card">
        <div class="dialog-header">
          <span class="dialog-title">{{ title }}</span>
        </div>
        <div class="dialog-divider"></div>
        <div class="dialog-body">
          <slot>
            <p class="dialog-message">{{ shortMessage }}</p>
          </slot>
        </div>
        <div v-if="hasFooter" class="dialog-footer" :class="{ 'no-divider': !divider }">
          <button v-if="message && copyable" class="dialog-copy" @click="copyMessage">
            {{ copied ? "已复制" : "复制错误信息" }}
          </button>
          <slot name="actions" />
        </div>
      </div>
    </div>
  </Transition>
</template>

<style scoped>
.dialog-overlay {
  position: fixed;
  inset: 0;
  z-index: 1000;
  display: flex;
  align-items: center;
  justify-content: center;
  background: rgba(30, 27, 75, 0.35);
  backdrop-filter: blur(2px);
}

.dialog-card {
  width: 380px;
  background: var(--surface-card);
  border-radius: var(--radius-lg);
  box-shadow: 0 8px 32px rgba(30, 27, 75, 0.15);
  overflow: hidden;
  user-select: none;
}

.dialog-header {
  padding: 18px 24px 12px;
}

.dialog-title {
  font-size: 17px;
  font-weight: 700;
  color: var(--color-brand);
}

/* 标题与正文之间的分隔线：左右留间距、颜色加深，与标题左对齐 */
.dialog-divider {
  height: 1px;
  margin: 0 24px;
  background: var(--border-strong);
}

.dialog-body {
  padding: 16px 24px;
}

.dialog-body p {
  margin: 0;
  font-size: 14px;
  line-height: 1.6;
  color: var(--text-secondary);
  white-space: pre-wrap;
  word-break: break-word;
}

.dialog-footer {
  display: flex;
  justify-content: flex-end;
  align-items: center;
  gap: 8px;
  padding: 12px 24px;
  border-top: 1px solid var(--border-default);
}

.dialog-footer.no-divider {
  border-top: none;
}

.dialog-copy {
  padding: 5px 12px;
  border: 1px solid var(--border-strong);
  border-radius: var(--radius-sm);
  background: transparent;
  color: var(--text-secondary);
  font-size: 12px;
  cursor: pointer;
  transition: all 0.15s;
}

.dialog-copy:hover {
  border-color: var(--color-brand);
  color: var(--color-brand);
  background: var(--color-brand-bg);
}

.dialog-enter-active {
  transition: opacity 0.2s ease-out;
}

.dialog-enter-active .dialog-card {
  transition: opacity 0.2s ease-out, transform 0.2s ease-out;
}

.dialog-leave-active {
  transition: opacity 0.15s ease-in;
}

.dialog-leave-active .dialog-card {
  transition: opacity 0.15s ease-in, transform 0.15s ease-in;
}

.dialog-enter-from {
  opacity: 0;
}

.dialog-enter-from .dialog-card {
  opacity: 0;
  transform: scale(0.95);
}

.dialog-leave-to {
  opacity: 0;
}

.dialog-leave-to .dialog-card {
  opacity: 0;
  transform: scale(0.95);
}
</style>
