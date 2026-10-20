<script setup lang="ts">
// 目标选择器组合器：变量（@e/@a/…）+ 条件行（type/distance/limit+sort/name/tag/gamemode）
// → 输出选择器文本；未识别条件（粘贴的 nbt=/scores= 等）保留在「额外条件」区原样拼回。
import { computed, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import { ChevronDown, ChevronUp } from "@lucide/vue";
import DropSelect, { type SelOption } from "../common/DropSelect.vue";
import CheckToggle from "../common/CheckToggle.vue";
import SearchSelect from "./SearchSelect.vue";
import {
  parseTargetSelector, serializeTargetSelector, type SelectorBase, type TargetSelector,
} from "../../utils/targetSelector";
import { SUMMON_ENTITIES, optionLabel } from "../../utils/commands";

const { t } = useI18n();

const props = defineProps<{
  modelValue: string;
}>();

const emit = defineEmits<{ (e: "update:modelValue", v: string): void }>();

const baseOptions = computed<SelOption[]>(() => [
  { value: "@e", label: t("tools.target.baseAllEntities") },
  { value: "@a", label: t("tools.target.baseAllPlayers") },
  { value: "@p", label: t("tools.target.baseNearestPlayer") },
  { value: "@s", label: t("tools.target.baseSelf") },
  { value: "@r", label: t("tools.target.baseRandomPlayer") },
]);
const sortOptions = computed<SelOption[]>(() => [
  { value: "", label: t("tools.target.sortDefault") },
  { value: "nearest", label: t("tools.target.sortNearest") },
  { value: "furthest", label: t("tools.target.sortFurthest") },
  { value: "random", label: t("tools.target.sortRandom") },
  { value: "arbitrary", label: t("tools.target.sortArbitrary") },
]);
const gamemodeOptions = computed<SelOption[]>(() => [
  { value: "", label: t("tools.target.gamemodeAny") },
  { value: "survival", label: t("tools.target.gamemodeSurvival") },
  { value: "creative", label: t("tools.target.gamemodeCreative") },
  { value: "adventure", label: t("tools.target.gamemodeAdventure") },
  { value: "spectator", label: t("tools.target.gamemodeSpectator") },
]);
const entityOptions = computed(() =>
  SUMMON_ENTITIES.map((o) => ({ id: o.value, name: optionLabel(o) })),
);

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
        {{ t("tools.target.conditions") }}
      </button>
      <code class="ts-preview">{{ output }}</code>
    </div>

    <div v-if="expanded" class="ts-panel">
      <div v-if="sel.base === '@e'" class="nb-row">
        <span class="nb-label">{{ t("tools.target.type") }}</span>
        <SearchSelect
          v-model="sel.type"
          :options="entityOptions"
          :allow-custom="true"
          :placeholder="t('tools.target.typePlaceholder')"
          max-width="240px"
        />
      </div>
      <div class="nb-row">
        <span class="nb-label">{{ t("tools.target.distance") }}</span>
        <input v-model.number="sel.distance.min" type="number" class="nb-num" :placeholder="t('tools.target.min')" autocomplete="off" />
        <span class="nb-hint">..</span>
        <input v-model.number="sel.distance.max" type="number" class="nb-num" :placeholder="t('tools.target.max')" autocomplete="off" />
        <span class="nb-hint">{{ t("tools.target.openRangeHint") }}</span>
      </div>
      <div class="nb-row">
        <span class="nb-label">{{ t("tools.target.count") }}</span>
        <input v-model.number="sel.limit" type="number" class="nb-num" min="0" max="2147483647" autocomplete="off" />
        <span class="nb-label nb-label-short">{{ t("tools.target.sort") }}</span>
        <DropSelect
          :model-value="sel.sort"
          :options="sortOptions"
          width="120px"
          @update:model-value="(v: string) => (sel.sort = v)"
        />
      </div>
      <div class="nb-row">
        <span class="nb-label">{{ t("tools.target.name") }}</span>
        <input v-model="sel.name.value" class="nb-input" :placeholder="t('tools.target.namePlaceholder')" spellcheck="false" autocomplete="off" />
        <CheckToggle v-model="sel.name.negate" :size="14">{{ t("tools.target.exclude") }}</CheckToggle>
      </div>
      <div class="nb-row">
        <span class="nb-label">{{ t("tools.target.tag") }}</span>
        <input v-model="sel.tag.value" class="nb-input" :placeholder="t('tools.target.tagPlaceholder')" spellcheck="false" autocomplete="off" />
        <CheckToggle v-model="sel.tag.negate" :size="14">{{ t("tools.target.exclude") }}</CheckToggle>
      </div>
      <div v-if="isPlayerBase" class="nb-row">
        <span class="nb-label">{{ t("tools.target.gamemode") }}</span>
        <DropSelect
          :model-value="sel.gamemode.value"
          :options="gamemodeOptions"
          width="120px"
          @update:model-value="(v: string) => (sel.gamemode.value = v)"
        />
        <CheckToggle v-model="sel.gamemode.negate" :size="14">{{ t("tools.target.exclude") }}</CheckToggle>
      </div>
      <div class="nb-row nb-row-top">
        <span class="nb-label">{{ t("tools.target.extra") }}</span>
        <textarea
          v-model="extraText"
          class="nb-textarea"
          rows="2"
          spellcheck="false"
          autocomplete="off"
          :placeholder="t('tools.target.extraPlaceholder')"
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
