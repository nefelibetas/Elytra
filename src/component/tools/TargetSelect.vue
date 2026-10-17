<script setup lang="ts">
// 目标选择器组合器：变量（@e/@a/…）+ 条件行（type/distance/limit+sort/name/tag/gamemode）
// → 输出选择器文本；未识别条件（粘贴的 nbt=/scores= 等）保留在「额外条件」区原样拼回。
import { computed, ref, watch } from "vue";
import { ChevronDown, ChevronUp } from "@lucide/vue";
import DropSelect, { type SelOption } from "../common/DropSelect.vue";
import CheckToggle from "../common/CheckToggle.vue";
import SearchSelect from "./SearchSelect.vue";
import {
  parseTargetSelector, serializeTargetSelector, type SelectorBase, type TargetSelector,
} from "../../utils/targetSelector";
import { SUMMON_ENTITIES } from "../../utils/commands";

const props = defineProps<{
  modelValue: string;
}>();

const emit = defineEmits<{ (e: "update:modelValue", v: string): void }>();

const baseOptions: SelOption[] = [
  { value: "@e", label: "@e 所有实体" },
  { value: "@a", label: "@a 所有玩家" },
  { value: "@p", label: "@p 最近玩家" },
  { value: "@s", label: "@s 自己" },
  { value: "@r", label: "@r 随机玩家" },
];
const sortOptions: SelOption[] = [
  { value: "", label: "（默认）" },
  { value: "nearest", label: "最近" },
  { value: "furthest", label: "最远" },
  { value: "random", label: "随机" },
  { value: "arbitrary", label: "任意" },
];
const gamemodeOptions: SelOption[] = [
  { value: "", label: "（不限定）" },
  { value: "survival", label: "生存" },
  { value: "creative", label: "创造" },
  { value: "adventure", label: "冒险" },
  { value: "spectator", label: "旁观" },
];
const entityOptions = SUMMON_ENTITIES.map((o) => ({ id: o.value, name: o.label }));

/** 内部模型：外部值变化（重挂载/粘贴）时重新解析。 */
const sel = ref<TargetSelector>(parseTargetSelector(props.modelValue || "@e"));
const expanded = ref(false);

watch(
  () => props.modelValue,
  (v) => {
    // 只在外部值与当前输出不一致时重新解析（避免自身输出触发重置 UI）
    if (v !== serializeTargetSelector(sel.value)) sel.value = parseTargetSelector(v || "@e");
  },
);

watch(
  sel,
  () => {
    emit("update:modelValue", serializeTargetSelector(sel.value));
  },
  { deep: true },
);

const isPlayerBase = computed(() => sel.value.base !== "@e");
const output = computed(() => serializeTargetSelector(sel.value));

/** "额外条件"原文（一行一条）。 */
const extraText = computed({
  get: () => sel.value.extra.join("\n"),
  set: (v: string) => {
    sel.value.extra = v
      .split("\n")
      .map((l) => l.trim())
      .filter((l) => l);
  },
});
</script>

<template>
  <div class="ts">
    <div class="ts-row">
      <DropSelect
        :model-value="sel.base"
        :options="baseOptions"
        width="150px"
        @update:model-value="(v: string) => (sel.base = v as SelectorBase)"
      />
      <button class="tool-btn ts-toggle" @click="expanded = !expanded">
        <ChevronUp v-if="expanded" :size="13" />
        <ChevronDown v-else :size="13" />
        条件
      </button>
      <code class="ts-preview">{{ output }}</code>
    </div>

    <div v-if="expanded" class="ts-panel">
      <div v-if="sel.base === '@e'" class="nb-row">
        <span class="nb-label">类型</span>
        <SearchSelect
          v-model="sel.type"
          :options="entityOptions"
          :allow-custom="true"
          placeholder="不限定 / 搜索实体"
          max-width="240px"
        />
      </div>
      <div class="nb-row">
        <span class="nb-label">距离</span>
        <input v-model.number="sel.distance.min" type="number" class="nb-num" placeholder="最小" autocomplete="off" />
        <span class="nb-hint">..</span>
        <input v-model.number="sel.distance.max" type="number" class="nb-num" placeholder="最大" autocomplete="off" />
        <span class="nb-hint">留空 = 开区间</span>
      </div>
      <div class="nb-row">
        <span class="nb-label">数量</span>
        <input v-model.number="sel.limit" type="number" class="nb-num" min="0" max="2147483647" autocomplete="off" />
        <span class="nb-label nb-label-short">排序</span>
        <DropSelect
          :model-value="sel.sort"
          :options="sortOptions"
          width="120px"
          @update:model-value="(v: string) => (sel.sort = v)"
        />
      </div>
      <div class="nb-row">
        <span class="nb-label">名称</span>
        <input v-model="sel.name.value" class="nb-input" placeholder="玩家名 / 自定义名" spellcheck="false" autocomplete="off" />
        <CheckToggle v-model="sel.name.negate" :size="14">排除</CheckToggle>
      </div>
      <div class="nb-row">
        <span class="nb-label">标签</span>
        <input v-model="sel.tag.value" class="nb-input" placeholder="tag" spellcheck="false" autocomplete="off" />
        <CheckToggle v-model="sel.tag.negate" :size="14">排除</CheckToggle>
      </div>
      <div v-if="isPlayerBase" class="nb-row">
        <span class="nb-label">模式</span>
        <DropSelect
          :model-value="sel.gamemode.value"
          :options="gamemodeOptions"
          width="120px"
          @update:model-value="(v: string) => (sel.gamemode.value = v)"
        />
        <CheckToggle v-model="sel.gamemode.negate" :size="14">排除</CheckToggle>
      </div>
      <div class="nb-row nb-row-top">
        <span class="nb-label">额外条件</span>
        <textarea
          v-model="extraText"
          class="nb-textarea"
          rows="2"
          spellcheck="false"
          autocomplete="off"
          placeholder="未识别的条件原样保留（如 nbt={NoAI:1b}，一行一条）"
        ></textarea>
      </div>
    </div>
  </div>
</template>

<style>
@import "../../styles/tools-shared.css";
</style>

<style scoped>
.ts {
  display: flex;
  flex-direction: column;
  gap: 6px;
  flex: 1;
  min-width: 0;
}

.ts-row {
  display: flex;
  align-items: center;
  gap: 8px;
}

.ts-toggle {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  font-size: 12px;
  padding: 4px 10px;
}

.ts-preview {
  font-family: Consolas, "Courier New", monospace;
  font-size: 11.5px;
  color: var(--text-tertiary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.ts-panel {
  padding: 8px 10px;
  background: var(--surface-page);
  border: 1px solid var(--border-default);
  border-radius: var(--radius-sm);
}
</style>
