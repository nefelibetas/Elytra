<script setup lang="ts">
// 交易物品修饰编辑器：名称（16 色/§x 自定义色）+ 附魔列表。
// form 为父组件 reactive 对象，直接修改其字段（项目内无 props 变更 lint）。
import { ref } from "vue";
import { computed } from "vue";
import { Plus, Trash2 } from "@lucide/vue";
import DropSelect, { type SelOption } from "./DropSelect.vue";
import { ENCHANTMENTS, filterEnchantsByVersion, type EnchantInput, type GiveFormat } from "../utils/give";
import { MC_COLORS, MC_COLOR_KEYS, MC_COLOR_NAMES, hexToSectionX } from "../utils/motd";

/** 交易物品修饰表单（与 ItemMods 对应，省略不常用字段）。 */
export interface ModsForm {
  name: string;
  colorCode: string;
  enchants: EnchantInput[];
}

const props = defineProps<{
  form: ModsForm;
  /** 命令格式（附魔按版本过滤）。 */
  format: GiveFormat;
}>();

const newEnch = ref("");
const newEnchLevel = ref(1);

const enchOptions = computed<SelOption[]>(() =>
  filterEnchantsByVersion(ENCHANTMENTS, props.format).map((e) => ({
    value: e.id,
    label: `${e.name}（原版最高 ${e.max}）`,
  })),
);

function addEnchant() {
  const item = ENCHANTMENTS.find((e) => e.id === newEnch.value);
  if (!item) return;
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
      <span class="nb-label">名称</span>
      <input
        v-model="form.name"
        class="nb-input"
        placeholder="留空使用默认名称"
        spellcheck="false"
      />
    </div>
    <div class="nb-row">
      <span class="nb-label">名称颜色</span>
      <button
        v-for="key in MC_COLOR_KEYS"
        :key="key"
        class="me-color"
        :class="{ active: form.colorCode === `§${key}` }"
        :style="{ backgroundColor: MC_COLORS[key] }"
        :title="`§${key} ${MC_COLOR_NAMES[key]}`"
        @click="form.colorCode = `§${key}`"
      />
      <input
        type="color"
        class="me-color-pick"
        :class="{ active: form.colorCode.startsWith('§x') }"
        title="自定义颜色（§x，需 1.16+）"
        @input="onCustomColor"
      />
      <button
        class="me-clear"
        title="清除颜色"
        @click="form.colorCode = ''"
      >无</button>
    </div>
    <div class="nb-row">
      <span class="nb-label">附魔</span>
      <DropSelect
        :model-value="newEnch"
        :options="enchOptions"
        width="150px"
        placeholder="选择附魔"
        @update:model-value="(v: string) => (newEnch = v)"
      />
      <input v-model.number="newEnchLevel" type="number" class="nb-num" min="1" max="255" />
      <button class="tool-btn nb-add-btn" :disabled="!newEnch" @click="addEnchant">
        <Plus :size="14" />
        添加
      </button>
    </div>
    <div v-if="form.enchants.length > 0" class="nb-tags">
      <span v-for="(e, i) in form.enchants" :key="i" class="nb-tag">
        {{ ENCHANTMENTS.find((x) => x.id === e.id)?.name ?? e.id }} {{ e.level }}
        <button class="nb-tag-x" @click="removeEnchant(i)">
          <Trash2 :size="12" />
        </button>
      </span>
    </div>
  </div>
</template>

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

.nb-tag-x {
  display: inline-flex;
  align-items: center;
  padding: 2px;
  border: none;
  background: transparent;
  color: var(--text-tertiary);
  cursor: pointer;
  border-radius: 4px;
}

.nb-tag-x:hover {
  color: var(--text-danger, #e5484d);
  background-color: var(--surface-hover);
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
</style>
