<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, useSlots } from "vue";

const props = withDefaults(defineProps<{
  title?: string;
  message?: string;
  /** 是否显示「复制错误信息」按钮（仅复杂错误需要） */
  copyable?: boolean;
  modelValue: boolean;
}>(), {
  title: "提示",
  message: "",
  copyable: false,
});

const emit = defineEmits<{
  "update:modelValue": [value: boolean];
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
        <div class="dialog-body">
          <slot>
            <p class="dialog-message">{{ shortMessage }}</p>
          </slot>
        </div>
        <div v-if="hasFooter" class="dialog-footer">
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
  z-index: var(--z-modal, 1100);
  display: flex;
  align-items: center;
  justify-content: center;
  background: var(--overlay);
  backdrop-filter: blur(2px);
}

/* WebKitGTK 低版本无 backdrop-filter（遮罩无模糊）：提升不透明度补偿可读性
 * （色值与 --overlay 的 rgba(15, 23, 42, …) 同源，仅提升 alpha） */
@supports not (backdrop-filter: blur(1px)) {
  .dialog-overlay {
    background: rgba(15, 23, 42, 0.68);
  }
}

.dialog-card {
  width: 440px;
  max-width: calc(100vw - 48px);
  max-height: calc(100vh - 48px);
  display: flex;
  flex-direction: column;
  background: var(--surface-card);
  border-radius: var(--radius-lg);
  box-shadow: var(--shadow-lg);
  overflow: hidden;
  user-select: none;
}

.dialog-header {
  position: relative;
  flex: none;
  padding: 14px 20px 12px;
  /* 标题与正文、正文与操作区使用同一条分隔线：1px、text-tertiary（比 border-* 深一档），
     左右留 20px 与内容对齐（border-* 过浅，视觉上无法承托分区） */
}

.dialog-header::after {
  content: "";
  position: absolute;
  left: 20px;
  right: 20px;
  bottom: 0;
  height: 1px;
  background: var(--text-tertiary);
}

.dialog-title {
  font-size: 16px;
  font-weight: 700;
  color: var(--color-brand);
}


.dialog-body {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  padding: 14px 20px;
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
  position: relative;
  flex: none;
  display: flex;
  justify-content: flex-end;
  align-items: center;
  gap: 8px;
  padding: 12px 20px;
}

.dialog-footer::before {
  content: "";
  position: absolute;
  left: 20px;
  right: 20px;
  top: 0;
  height: 1px;
  background: var(--text-tertiary);
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
