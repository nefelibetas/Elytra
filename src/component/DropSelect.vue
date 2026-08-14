<script setup lang="ts">
// 通用自定义下拉选择（DropSelect）：原生 <select> 替代品。
// 背景：UA 原生 select 的展开面板（浅蓝高亮/系统字体）与项目风格割裂，
// 统一为自定义面板（对齐 jdk-dropdown 视觉：surface-card + 边框 + hover/selected 态）。
// 无 props 链：纯展示控件，v-model 双向绑定 + 点击外部关闭。
import { ref, computed, onMounted, onUnmounted } from "vue";
import { Check, ChevronDown } from "@lucide/vue";

export interface SelOption {
  value: string;
  label: string;
  desc?: string;
}

const props = withDefaults(
  defineProps<{
    modelValue: string;
    options: SelOption[];
    disabled?: boolean;
    /** 触发按钮宽度 */
    width?: string;
    placeholder?: string;
    maxHeight?: string;
  }>(),
  {
    disabled: false,
    width: "100%",
    placeholder: "请选择",
    maxHeight: "280px",
  },
);

const emit = defineEmits<{ (e: "update:modelValue", v: string): void }>();

const open = ref(false);
const rootEl = ref<HTMLElement>();

const current = computed(() => props.options.find((o) => o.value === props.modelValue));

/** 点击组件外部关闭面板（挂载周期内注册，对齐 AccountSection 菜单模式）。 */
function onDocClick(e: MouseEvent) {
  if (open.value && rootEl.value && !rootEl.value.contains(e.target as Node)) {
    open.value = false;
  }
}
onMounted(() => window.addEventListener("click", onDocClick));
onUnmounted(() => window.removeEventListener("click", onDocClick));

function select(o: SelOption) {
  emit("update:modelValue", o.value);
  open.value = false;
}
</script>

<template>
  <div ref="rootEl" class="ds" :style="{ width }">
    <button
      class="ds-btn"
      :class="{ open }"
      :disabled="disabled"
      type="button"
      @click="open = !open"
    >
      <span class="ds-label">{{ current?.label ?? placeholder }}</span>
      <ChevronDown class="ds-chev" :class="{ flip: open }" :size="12" />
    </button>

    <Transition name="drop">
      <div v-if="open" class="ds-panel" :style="{ maxHeight }">
        <button
          v-for="o in options"
          :key="o.value"
          class="ds-opt"
          :class="{ selected: o.value === modelValue }"
          type="button"
          @click="select(o)"
        >
          <div class="ds-opt-main">
            <span class="ds-opt-label">{{ o.label }}</span>
            <span v-if="o.desc" class="ds-opt-desc">{{ o.desc }}</span>
          </div>
          <Check v-if="o.value === modelValue" :size="14" class="ds-opt-check" />
        </button>
      </div>
    </Transition>
  </div>
</template>

<style scoped>
.ds {
  position: relative;
  flex: none;
}

.ds-btn {
  width: 100%;
  height: 34px;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  padding: 0 10px;
  border: 1px solid var(--border-strong);
  border-radius: var(--radius-sm);
  background: var(--surface-card);
  color: var(--text-primary);
  font-size: 13px;
  cursor: pointer;
  transition: border-color 0.15s;
}

.ds-btn:hover,
.ds-btn.open {
  border-color: var(--color-brand);
}

.ds-btn:disabled {
  opacity: 0.6;
  cursor: not-allowed;
}

.ds-label {
  flex: 1;
  min-width: 0;
  text-align: left;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.ds-chev {
  flex: none;
  color: var(--text-tertiary);
  transition: transform 0.15s;
}

.ds-chev.flip {
  transform: rotate(180deg);
}

.ds-panel {
  position: absolute;
  top: calc(100% + 4px);
  left: 0;
  z-index: 30;
  /* 面板比按钮宽：按最宽选项自适应撑开（min-width 保底 = 按钮宽），选项单行完整可读；
     max-width 兜底防极端长文本（游戏版本/路径）撑破布局 */
  width: max-content;
  min-width: 100%;
  max-width: 380px;
  overflow-y: auto;
  background: var(--surface-card);
  border: 1px solid var(--border-default);
  border-radius: var(--radius-md);
  box-shadow: var(--shadow-md);
  padding: 6px;
}

.ds-opt {
  width: 100%;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
  padding: 8px 10px;
  border: none;
  border-radius: var(--radius-sm);
  background: transparent;
  color: var(--text-primary);
  font-size: 13px;
  text-align: left;
  white-space: nowrap;
  cursor: pointer;
  transition: background 0.1s;
}

.ds-opt:hover {
  background: var(--surface-hover);
}

.ds-opt.selected {
  background: var(--color-brand-bg);
}

.ds-opt.selected .ds-opt-label {
  color: var(--color-brand);
  font-weight: 600;
}

.ds-opt-main {
  min-width: 0;
}

.ds-opt-label {
  display: block;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.ds-opt-desc {
  display: block;
  font-size: 11px;
  color: var(--text-tertiary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.ds-opt-check {
  flex: none;
  color: var(--color-brand);
}

/* 面板展开动画（对齐 jdk-dropdown 的 drop） */
.drop-enter-active,
.drop-leave-active {
  transition: opacity 0.15s ease, transform 0.15s ease;
}

.drop-enter-from,
.drop-leave-to {
  opacity: 0;
  transform: translateY(-4px);
}
</style>
