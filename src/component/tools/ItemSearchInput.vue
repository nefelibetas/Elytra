<script setup lang="ts">
// 物品搜索选择输入：输入中文名或 ID 关键字 → 下拉匹配 → 点击选中；再次输入即清除原选择（emit 空串）。
// 用于村民交易等需手填物品 ID 的场景（替代裸文本输入）。
import { computed, ref, watch } from "vue";
import { Search } from "@lucide/vue";
import { GIVE_ITEMS } from "../../utils/give";

const props = withDefaults(
  defineProps<{
    modelValue: string;
    placeholder?: string;
    /** 额外允许的物品（不在 GIVE_ITEMS 中，如 emerald）。 */
    extraIds?: string[];
    /** 版本上限（主版本号，如 20 = ≤1.20.x；null = 不过滤）。 */
    maxVersion?: number | null;
  }>(),
  {
    placeholder: "搜索物品（中文名或 ID）…",
    extraIds: () => [],
    maxVersion: null,
  },
);

const emit = defineEmits<{ (e: "update:modelValue", v: string): void }>();

/** 输入框文本：选中时显示中文名，未选中为用户输入。 */
const keyword = ref("");
const open = ref(false);

/** 选中物品中文名（含 extra 补丁）。 */
function nameOf(id: string): string {
  return GIVE_ITEMS.find((i) => i.id === id)?.name ?? id;
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

/** 匹配列表（最多 8 条；中文名/ID 模糊匹配）。 */
const matches = computed(() => {
  const kw = keyword.value.trim().toLowerCase();
  if (!kw) return [];
  const fromItems = GIVE_ITEMS.filter((i) => {
    if (props.maxVersion !== null && (i.since ?? 13) > props.maxVersion) return false;
    return i.id.includes(kw) || i.name.toLowerCase().includes(kw);
  }).slice(0, 8);
  const extras = props.extraIds
    .filter((id) => id.includes(kw) && !fromItems.some((i) => i.id === id))
    .map((id) => ({ id, name: nameOf(id) }));
  return [...fromItems, ...extras];
});

/** 用户输入：清空原选择并展开匹配。 */
function onInput() {
  emit("update:modelValue", "");
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
  }, 150);
}
</script>

<template>
  <div class="isi">
    <div class="isi-input-wrap">
      <Search :size="13" class="isi-icon" />
      <input
        v-model="keyword"
        class="isi-input"
        :placeholder="placeholder"
        spellcheck="false"
        @input="onInput"
        @focus="open = keyword.trim().length > 0"
        @blur="onBlur"
        autocomplete="off"
      />
    </div>
    <div v-if="open && matches.length > 0" class="isi-panel">
      <button
        v-for="m in matches"
        :key="m.id"
        class="isi-opt"
        type="button"
        @mousedown.prevent="pick(m.id)"
      >
        <span class="isi-opt-name">{{ m.name }}</span>
        <span class="isi-opt-id">{{ m.id }}</span>
      </button>
    </div>
    <div v-if="open && matches.length === 0" class="isi-panel">
      <span class="isi-empty">无匹配物品</span>
    </div>
  </div>
</template>

<style scoped>
.isi {
  position: relative;
  flex: 1;
  min-width: 120px;
  max-width: 220px;
}

.isi-input-wrap {
  position: relative;
  display: flex;
  align-items: center;
}

.isi-icon {
  position: absolute;
  left: 9px;
  color: var(--text-tertiary);
  pointer-events: none;
}

.isi-input {
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

.isi-input:hover,
.isi-input:focus {
  border-color: var(--color-brand);
}

.isi-panel {
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
  overflow: hidden;
}

.isi-opt {
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

.isi-opt:hover {
  background: var(--surface-hover);
}

.isi-opt-name {
  font-size: 12.5px;
  color: var(--text-primary);
}

.isi-opt-id {
  font-family: "Cascadia Mono", Consolas, "Courier New", monospace;
  font-size: 10.5px;
  color: var(--text-tertiary);
}

.isi-empty {
  padding: 8px 10px;
  font-size: 12px;
  color: var(--text-tertiary);
}
</style>
