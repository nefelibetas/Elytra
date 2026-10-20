<script setup lang="ts">
// 交易物品修饰编辑器：名称（16 色/§x 自定义色）+ 附魔列表。
// form 为父组件 reactive 对象，直接修改其字段（项目内无 props 变更 lint）。
import { ref } from "vue";
import { computed, watch } from "vue";
import { useI18n } from "vue-i18n";
import { Plus, Trash2 } from "@lucide/vue";
import DropSelect, { type SelOption } from "../common/DropSelect.vue";
import {
  ENCHANTMENTS, enchantName, filterEnchantsByVersion, supportsHexColor, syntaxOfVersion,
  type EnchantInput, type GiveVersion,
} from "../../utils/give";
import { checkNbtFragment } from "../../utils/nbtText";
import { MC_COLORS, MC_COLOR_KEYS, colorName, hexToSectionX } from "../../utils/motd";

/** 交易物品修饰表单（与 ItemMods 对应，省略不常用字段）。 */
export interface ModsForm {
  name: string;
  colorCode: string;
  enchants: EnchantInput[];
  /** 自定义 NBT（两档分开保存，切档不丢）：`{` 开头按完整物品栈覆盖，否则并入附加内容。 */
  rawLegacy: string;
  rawModern: string;
}

const { t } = useI18n();

const props = defineProps<{
  form: ModsForm;
  /** 目标版本区间（决定语法与可用附魔）。 */
  version: GiveVersion;
}>();

const syntax = computed(() => syntaxOfVersion(props.version));
const hexColorSupported = computed(() => supportsHexColor(props.version));

/** 当前档的自定义 NBT（单框双绑定；换档切到该档原文）。 */
const raw = computed({
  get: () => (syntax.value === "modern" ? props.form.rawModern : props.form.rawLegacy),
  set: (v: string) => {
    if (syntax.value === "modern") props.form.rawModern = v;
    else props.form.rawLegacy = v;
  },
});

/** 括号配平预检（不阻断输入，仅提示）。 */
const rawError = computed(() => (raw.value.trim() ? checkNbtFragment(raw.value) : null));

const newEnch = ref("");
const newEnchLevel = ref(1);

const enchOptions = computed<SelOption[]>(() =>
  filterEnchantsByVersion(ENCHANTMENTS, props.version).map((e) => ({
    value: e.id,
    label: t("tools.give.enchantOption", { name: enchantName(e.id), max: e.max }),
  })),
);

/** 版本切换：清掉该区间不存在的附魔与 §x 颜色。 */
watch(
  () => props.version,
  () => {
    const ids = new Set(enchOptions.value.map((o) => o.value));
    props.form.enchants = props.form.enchants.filter((e) => ids.has(e.id));
    if (newEnch.value && !ids.has(newEnch.value)) newEnch.value = "";
    if (!hexColorSupported.value && props.form.colorCode.startsWith("§x")) props.form.colorCode = "";
  },
  { immediate: true },
);

function addEnchant() {
  const item = ENCHANTMENTS.find((e) => e.id === newEnch.value);
  if (!item || !enchOptions.value.some((o) => o.value === newEnch.value)) return;
  props.form.enchants.push({
    id: item.id,
    // 附魔等级不受原版上限限制（clamp 到 255 = NBT byte 上限）
    level: Math.max(1, Math.min(newEnchLevel.value, 255)),
  });
  newEnch.value = "";
  newEnchLevel.value = 1;
}

function removeEnchant(idx: number) {
  props.form.enchants.splice(idx, 1);
}

function onCustomColor(e: Event) {
  const sx = hexToSectionX((e.target as HTMLInputElement).value);
  if (sx) props.form.colorCode = sx;
}
</script>

<template>
  <div class="me">
    <div class="nb-row">
      <span class="nb-label">{{ t("tools.mods.name") }}</span>
      <input
        v-model="form.name"
        class="nb-input"
        :placeholder="t('tools.mods.namePlaceholder')"
        spellcheck="false"
        autocomplete="off"
      />
    </div>
    <div class="nb-row">
      <span class="nb-label">{{ t("tools.mods.nameColor") }}</span>
      <button
        v-for="key in MC_COLOR_KEYS"
        :key="key"
        class="me-color"
        :class="{ active: form.colorCode === `§${key}` }"
        :style="{ backgroundColor: MC_COLORS[key] }"
        :title="`§${key} ${colorName(key)}`"
        @click="form.colorCode = `§${key}`"
      />
      <input
        v-if="hexColorSupported"
        type="color"
        class="me-color-pick"
        :class="{ active: form.colorCode.startsWith('§x') }"
        :title="t('tools.mods.customColorTitle')"
        @input="onCustomColor"
        autocomplete="off"
      />
      <button
        class="me-clear"
        :title="t('tools.mods.clearColor')"
        @click="form.colorCode = ''"
      >{{ t("tools.mods.clearColor") }}</button>
    </div>
    <div class="nb-row">
      <span class="nb-label">{{ t("tools.mods.enchantments") }}</span>
      <DropSelect
        :model-value="newEnch"
        :options="enchOptions"
        width="150px"
        :placeholder="t('tools.give.chooseEnchant')"
        @update:model-value="(v: string) => (newEnch = v)"
      />
      <input v-model.number="newEnchLevel" type="number" class="nb-num" min="1" max="255" autocomplete="off" />
      <button class="tool-btn nb-add-btn" :disabled="!newEnch" @click="addEnchant">
        <Plus :size="14" />
        {{ t("common.action.add") }}
      </button>
    </div>
    <div v-if="form.enchants.length > 0" class="nb-tags">
      <span v-for="(e, i) in form.enchants" :key="i" class="nb-tag">
        {{ enchantName(e.id) }} {{ e.level }}
        <button class="nb-tag-x" @click="removeEnchant(i)">
          <Trash2 :size="12" />
        </button>
      </span>
    </div>
    <div class="nb-row me-raw-row">
      <span class="nb-label">{{ t("tools.mods.customNbt") }}</span>
      <textarea
        v-model="raw"
        class="me-raw"
        rows="2"
        spellcheck="false"
        autocomplete="off"
        :placeholder="syntax === 'modern'
          ? t('tools.mods.rawPlaceholderModern')
          : t('tools.mods.rawPlaceholderLegacy')"
      ></textarea>
    </div>
    <p v-if="rawError" class="me-raw-err">{{ rawError }}</p>
  </div>
</template>

<style>
@import "../../styles/tools-shared.css";
</style>

<style scoped>
.me {
  display: flex;
  flex-direction: column;
  gap: 2px;
  padding: 8px 10px;
  margin: 4px 0 8px;
  background: var(--surface-page);
  border: 1px solid var(--border-default);
  border-radius: var(--radius-sm);
}

.nb-row {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-bottom: 8px;
  flex-wrap: wrap;
}

/* label 固定宽：各修饰行（名称/名称颜色/附魔/自定义 NBT）控件左缘统一对齐 */
.me .nb-row > .nb-label {
  flex: 0 0 72px;
  white-space: nowrap;
}

.nb-label {
  flex: 0 0 auto;
  font-size: 12.5px;
  color: var(--text-secondary);
  min-width: 56px;
}

.nb-input {
  flex: 1;
  min-width: 80px;
  height: 30px;
  padding: 0 10px;
  font-size: 13px;
  color: var(--text-primary);
  background-color: var(--surface-elevated);
  border: 1px solid var(--border-default);
  border-radius: var(--radius-sm);
  outline: none;
}

.nb-input:hover,
.nb-input:focus {
  border-color: var(--color-brand);
}

.nb-num {
  width: 64px;
  height: 30px;
  padding: 0 8px;
  font-size: 13px;
  color: var(--text-primary);
  background-color: var(--surface-elevated);
  border: 1px solid var(--border-default);
  border-radius: var(--radius-sm);
  outline: none;
}

.nb-num:focus {
  border-color: var(--color-brand);
}

.nb-add-btn {
  font-size: 12px;
  padding: 4px 10px;
}

.nb-tags {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
}

.nb-tag {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  padding: 3px 6px 3px 10px;
  font-size: 12px;
  color: var(--text-secondary);
  background-color: var(--surface-elevated);
  border: 1px solid var(--border-default);
  border-radius: 999px;
}

.me-color {
  width: 18px;
  height: 18px;
  border: 1px solid var(--border-strong);
  border-radius: 3px;
  padding: 0;
  cursor: pointer;
  transition: transform 0.1s;
}

.me-color:hover {
  transform: scale(1.15);
}

.me-color.active {
  outline: 2px solid var(--color-brand);
  outline-offset: 1px;
}

.me-color-pick {
  width: 20px;
  height: 20px;
  padding: 0;
  border: 1px solid var(--border-strong);
  border-radius: 3px;
  background: none;
  cursor: pointer;
}

.me-color-pick.active {
  outline: 2px solid var(--color-brand);
  outline-offset: 1px;
}

.me-clear {
  height: 22px;
  padding: 0 8px;
  font-size: 11px;
  color: var(--text-secondary);
  background-color: var(--surface-elevated);
  border: 1px solid var(--border-default);
  border-radius: var(--radius-sm);
  cursor: pointer;
}

.me-clear:hover {
  border-color: var(--color-brand);
}

.me-raw-row {
  align-items: flex-start;
}

.me-raw {
  flex: 1;
  min-width: 220px;
  padding: 5px 8px;
  font-family: Consolas, "Courier New", monospace;
  font-size: 12px;
  color: var(--text-primary);
  background-color: var(--surface-elevated);
  border: 1px solid var(--border-default);
  border-radius: var(--radius-sm);
  resize: vertical;
}

.me-raw:focus {
  outline: none;
  border-color: var(--color-brand);
}

.me-raw-err {
  margin: 0 0 8px;
  font-size: 11px;
  color: var(--color-danger);
}
</style>
