<script setup lang="ts">
// 搜索选择输入：输入中文名/ID 关键字 → 下拉匹配（内容自适应高度，超出按 ~8 项封顶滚动）→ 点击选中。
// 两种模式：默认「限列表」（物品场景，值必须来自 options）；`allowCustom` 时支持自由输入
// （实体/效果等场景，输入文本直接为值，失焦时精确匹配中文名/ID 则归一化，否则原样保留）。
// 用于物品/实体/效果等需手填 ID 的场景（替代裸文本输入）。
import { computed, ref, watch } from "vue";
import { Search } from "@lucide/vue";

export interface SearchOption {
  id: string;
  name: string;
}

const props = withDefaults(
  defineProps<{
    modelValue: string;
    options: SearchOption[];
    placeholder?: string;
    /** 额外允许的选项（与 options 合并去重，如村民交易的 emerald）。 */
    extraIds?: string[];
    /** 宽度上限（宽面板场景可放宽）。 */
    maxWidth?: string;
    /** 允许自由输入：输入文本即为值（非列表项原样保留，精确匹配时归一化为 id）。 */
    allowCustom?: boolean;
  }>(),
  {
    placeholder: "搜索…",
    extraIds: () => [],
    maxWidth: "220px",
    allowCustom: false,
  },
);

const emit = defineEmits<{ (e: "update:modelValue", v: string): void }>();

/** 输入框文本：选中时显示中文名，未选中为用户输入。 */
const keyword = ref("");
const open = ref(false);

function nameOf(id: string): string {
  return props.options.find((o) => o.id === id)?.name ?? id;
}

// 父组件值变化（外部清空/重置）→ 同步显示
watch(
  () => props.modelValue,
  (id) => {
    if (!id) return;
    keyword.value = nameOf(id);
  },
  { immediate: true },
);

/** 匹配列表（中文名/ID 模糊匹配；显示条数由面板高度上限滚动，仅防御性割顶）。 */
const matches = computed(() => {
  const kw = keyword.value.trim().toLowerCase();
  if (!kw) return [];
  const filtered = props.options.filter(
    (o) => o.id.includes(kw) || o.name.toLowerCase().includes(kw),
  );
  const extras = props.extraIds
    .filter((id) => id.includes(kw) && !filtered.some((o) => o.id === id))
    .map((id) => ({ id, name: nameOf(id) }));
  return [...filtered, ...extras].slice(0, 50);
});

/** 用户输入：限列表模式清空原选择并展开匹配；自由模式值随输入（供构建实时反映）。 */
function onInput() {
  if (props.allowCustom) {
    emit("update:modelValue", keyword.value.trim());
  } else {
    emit("update:modelValue", "");
  }
  open.value = keyword.value.trim().length > 0;
}

function pick(id: string) {
  keyword.value = nameOf(id);
  open.value = false;
  emit("update:modelValue", id);
}

function onBlur() {
  // 延迟关闭：让点击选项的 mousedown 先执行
  setTimeout(() => {
    open.value = false;
    if (!props.allowCustom) return;
    // 自由模式：精确匹配中文名/ID 时归一化为 id（用户输入了中文名但没点建议）
    const text = keyword.value.trim();
    if (!text) return;
    const exact = props.options.find((o) => o.name === text || o.id === text);
    if (exact) {
      keyword.value = exact.name;
      emit("update:modelValue", exact.id);
    }
  }, 150);
}
</script>

<template>
  <div class="ss" :style="{ maxWidth }">
    <div class="ss-input-wrap">
      <Search :size="13" class="ss-icon" />
      <input
        v-model="keyword"
        class="ss-input"
        :placeholder="placeholder"
        spellcheck="false"
        @input="onInput"
        @focus="open = keyword.trim().length > 0"
        @blur="onBlur"
        autocomplete="off"
      />
    </div>
    <div v-if="open && matches.length > 0" class="ss-panel">
      <button
        v-for="m in matches"
        :key="m.id"
        class="ss-opt"
        type="button"
        @mousedown.prevent="pick(m.id)"
      >
        <span class="ss-opt-name">{{ m.name }}</span>
        <span class="ss-opt-id">{{ m.id }}</span>
      </button>
    </div>
    <div v-if="open && matches.length === 0 && !allowCustom" class="ss-panel">
      <span class="ss-empty">无匹配项</span>
    </div>
  </div>
</template>

<style scoped>
.ss {
  position: relative;
  flex: 1;
  min-width: 120px;
  /* 上限由 maxWidth prop 内联给出（宽面板放宽） */
}

.ss-input-wrap {
  position: relative;
  display: flex;
  align-items: center;
}

.ss-icon {
  position: absolute;
  left: 9px;
  color: var(--text-tertiary);
  pointer-events: none;
}

.ss-input {
  width: 100%;
  box-sizing: border-box;
  height: 30px;
  padding: 0 10px 0 28px;
  font-size: 13px;
  color: var(--text-primary);
  background-color: var(--surface-page);
  border: 1px solid var(--border-default);
  border-radius: var(--radius-sm);
  outline: none;
}

.ss-input:hover,
.ss-input.focus,
.ss-input:focus {
  border-color: var(--color-brand);
}

.ss-panel {
  position: absolute;
  left: 0;
  right: 0;
  top: 34px;
  z-index: 30;
  display: flex;
  flex-direction: column;
  background: var(--surface-card);
  border: 1px solid var(--border-default);
  border-radius: var(--radius-sm);
  box-shadow: 0 6px 20px rgba(0, 0, 0, 0.18);
  /* 内容自适应高度；超出按 8 项（8×36px=288px）封顶滚动 */
  max-height: 288px;
  overflow-y: auto;
}

.ss-opt {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  padding: 6px 10px;
  border: none;
  background: transparent;
  cursor: pointer;
  text-align: left;
}

.ss-opt:hover {
  background: var(--surface-hover);
}

.ss-opt-name {
  font-size: 12.5px;
  color: var(--text-primary);
}

.ss-opt-id {
  font-family: "Cascadia Mono", Consolas, "Courier New", monospace;
  font-size: 10.5px;
  color: var(--text-tertiary);
}

.ss-empty {
  padding: 8px 10px;
  font-size: 12px;
  color: var(--text-tertiary);
}
</style>
