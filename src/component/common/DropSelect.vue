<script setup lang="ts">
// 通用自定义下拉（原生 <select> 替代品）：自定义面板统一风格（对齐 jdk-dropdown 视觉）。
// 纯展示控件：v-model 双向绑定 + 点击外部关闭。
import { ref, reactive, computed, watch, nextTick, onMounted, onUnmounted, useAttrs } from "vue";
import { useI18n } from "vue-i18n";
import { Check, ChevronDown } from "@lucide/vue";

export interface SelOption {
  value: string;
  label: string;
  desc?: string;
}

defineOptions({ inheritAttrs: false });

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
    placeholder: "",
    maxHeight: "280px",
  },
);

const emit = defineEmits<{ (e: "update:modelValue", v: string): void }>();
const attrs = useAttrs();
const rootClass = computed(() => attrs.class);
const buttonAttrs = computed(() => Object.fromEntries(
  Object.entries(attrs).filter(([key]) => key !== "class" && key !== "style"),
));
const { t } = useI18n();
const placeholderText = computed(() => props.placeholder || t("common.action.select"));

const open = ref(false);
const rootEl = ref<HTMLElement>();

/** 面板 fixed 视口定位（absolute 溢出会扩展滚动容器 scrollHeight）；scroll/resize 重定位，下方不足则上展；minW 显式传按钮宽。 */
const panelPos = reactive({ left: 0, top: 0, minW: 0 });
const panelEl = ref<HTMLElement>();

function positionPanel() {
  if (!open.value || !rootEl.value || !panelEl.value) return;
  const btnR = rootEl.value.getBoundingClientRect();
  const pr = panelEl.value.getBoundingClientRect();
  panelPos.minW = btnR.width;
  // 左缘对齐按钮；右缘超出视口时内收（fixed 下包含块=视口，按实际宽 clamp）
  panelPos.left = Math.min(btnR.left, Math.max(0, window.innerWidth - pr.width - 6));
  const spaceBelow = window.innerHeight - btnR.bottom - 4;
  panelPos.top = pr.height > spaceBelow ? Math.max(4, btnR.top - pr.height - 4) : btnR.bottom + 4;
  // minW 异步生效：下一帧按新宽重新校准左缘
  requestAnimationFrame(() => {
    if (!open.value || !rootEl.value || !panelEl.value) return;
    const w = panelEl.value.getBoundingClientRect().width;
    panelPos.left = Math.min(
      rootEl.value.getBoundingClientRect().left,
      Math.max(0, window.innerWidth - w - 6),
    );
  });
}

const current = computed(() => props.options.find((o) => o.value === props.modelValue));

// 打开后渲染完成再按按钮位置与面板高度定方向
watch(open, async (v) => {
  if (!v) return;
  await nextTick();
  positionPanel();
});

/** 点击组件外部关闭面板。 */
function onDocClick(e: MouseEvent) {
  if (open.value && rootEl.value && !rootEl.value.contains(e.target as Node)) {
    open.value = false;
  }
}

/** 捕获阶段先于全局 Esc 导航处理：下拉打开时只关闭下拉。 */
function onKeydown(e: KeyboardEvent) {
  if (!open.value || e.key !== "Escape") return;
  e.preventDefault();
  e.stopImmediatePropagation();
  open.value = false;
  rootEl.value?.querySelector<HTMLButtonElement>(".ds-btn")?.focus();
}
onMounted(() => {
  window.addEventListener("click", onDocClick);
  window.addEventListener("keydown", onKeydown, true);
  // capture：内部滚动容器的 scroll 也会触发（scroll 不冒泡）
  window.addEventListener("scroll", positionPanel, true);
  window.addEventListener("resize", positionPanel);
});
onUnmounted(() => {
  window.removeEventListener("click", onDocClick);
  window.removeEventListener("keydown", onKeydown, true);
  window.removeEventListener("scroll", positionPanel, true);
  window.removeEventListener("resize", positionPanel);
});

function select(o: SelOption) {
  emit("update:modelValue", o.value);
  open.value = false;
}
</script>

<template>
  <div ref="rootEl" class="ds" :class="rootClass" :style="{ width }">
    <button
      v-bind="buttonAttrs"
      class="ds-btn"
      :class="{ open }"
      :disabled="disabled"
      type="button"
      aria-haspopup="listbox"
      :aria-expanded="open"
      @click="open = !open"
    >
      <span class="ds-label">{{ current?.label ?? placeholderText }}</span>
      <ChevronDown class="ds-chev" :class="{ flip: open }" :size="12" />
    </button>

    <Transition name="drop">
      <div
        v-if="open"
        ref="panelEl"
        class="ds-panel"
        :style="{ left: panelPos.left + 'px', top: panelPos.top + 'px', maxHeight, '--ds-min-w': panelPos.minW + 'px' }"
      >
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
  position: fixed;
  z-index: 30;
  /* 坐标由 JS 按按钮 rect 计算（fixed 视口定位）；宽度按内容撑开，min-width 由 JS 传 CSS 变量
     （fixed 下 % 会相对视口）；max-width 兜底防超长文本撑破布局 */
  width: max-content;
  min-width: var(--ds-min-w, 0px);
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
