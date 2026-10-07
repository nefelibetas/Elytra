<script setup lang="ts">
// /give 生成器：搜索选择物品 → 配置（数量/名称颜色/背景故事/附魔/属性/选项）→
// 按所选语法格式（1.20.5+ 组件 / 1.13–1.20.4 NBT）生成 /give 命令。
// 物品 ID 采用 1.13+ 命名空间格式（主流版本稳定）；1.12 及以下数字 ID 不支持。
import { computed, ref } from "vue";
import { Copy, Plus, Trash2, Search } from "@lucide/vue";
import DropSelect, { type SelOption } from "../common/DropSelect.vue";
import CheckToggle from "../common/CheckToggle.vue";
import {
  GIVE_ITEMS, GIVE_CATEGORIES, ENCHANTMENTS, ATTRIBUTES,
  GIVE_FORMAT_OPTIONS, buildGiveCommand, filterItems,
  filterItemsByVersion, filterEnchantsByVersion,
  type GiveItem, type GiveFormat, type EnchantInput, type AttributeInput,
} from "../../utils/give";
import { MC_COLORS, MC_COLOR_KEYS, MC_COLOR_NAMES, hexToSectionX } from "../../utils/motd";
import { copyText } from "../../utils/clipboard";

/** 语法格式（modern = 1.20.5+ 组件 / legacy = 1.13–1.20.4 NBT）。 */
const format = ref<GiveFormat>("modern");
const formatOptions: SelOption[] = GIVE_FORMAT_OPTIONS.map((o) => ({
  value: o.value,
  label: o.label,
}));

/** 当前分类（all + 各分类）。 */
const cat = ref("all");
const catOptions: SelOption[] = [
  { value: "all", label: "全部" },
  ...GIVE_CATEGORIES.map((c) => ({ value: c.key, label: c.label })),
];
const keyword = ref("");
const selectedItem = ref<GiveItem | null>(null);

/** 过滤后的物品列表。 */
const items = computed(() =>
  filterItems(filterItemsByVersion(GIVE_ITEMS, format.value), cat.value, keyword.value),
);

function selectItem(it: GiveItem) {
  selectedItem.value = it;
}

// ── 配置 ──
const count = ref(1);
const colorCode = ref("");
const customName = ref("");
const loreText = ref("");
const unbreakable = ref(false);
const hideTooltip = ref(false);

/** 自定义颜色当前值（#RRGGBB；从 §x 前缀解析回显）。 */
const customColor = computed(() => {
  if (!colorCode.value.startsWith("§x")) return "#ffaa00";
  const hex = colorCode.value.slice(2).replace(/§/g, "");
  return `#${hex}`;
});

/** 自定义色输入 → §x 前缀（1.16+ 客户端渲染）。 */
function onCustomColor(e: Event) {
  const v = (e.target as HTMLInputElement).value;
  const sx = hexToSectionX(v);
  if (sx) colorCode.value = sx;
}

/** 背景故事逐行（textarea 按行拆分）。 */
function loreLines(): string[] {
  return loreText.value
    .split("\n")
    .map((l) => l.trim())
    .filter((l) => l.length > 0);
}

// ── 附魔 ──
const enchantments = ref<EnchantInput[]>([]);
const newEnch = ref("");
const newEnchLevel = ref(1);

const enchOptions = computed<SelOption[]>(() =>
  filterEnchantsByVersion(ENCHANTMENTS, format.value).map((e) => ({
    value: e.id,
    label: `${e.name}（原版最高 ${e.max}）`,
  })),
);

function addEnchant() {
  if (!newEnch.value) return;
  const item = ENCHANTMENTS.find((e) => e.id === newEnch.value);
  if (!item) return;
  enchantments.value.push({
    id: item.id,
    // give 命令附魔不受原版等级上限限制（clamp 到 255 = NBT byte 上限）
    level: Math.max(1, Math.min(newEnchLevel.value, 255)),
  });
  newEnch.value = "";
  newEnchLevel.value = 1;
}

function removeEnchant(idx: number) {
  enchantments.value.splice(idx, 1);
}

function enchantName(id: string): string {
  return ENCHANTMENTS.find((e) => e.id === id)?.name ?? id;
}

// ── 属性 ──
const attributes = ref<AttributeInput[]>([]);
const newAttr = ref("");
const newAttrAmount = ref(1);

const attrOptions: SelOption[] = ATTRIBUTES.map((a) => ({
  value: a.id,
  label: a.name,
}));

function addAttribute() {
  if (!newAttr.value) return;
  attributes.value.push({ id: newAttr.value, amount: newAttrAmount.value });
  newAttr.value = "";
  newAttrAmount.value = 1;
}

function removeAttribute(idx: number) {
  attributes.value.splice(idx, 1);
}

function attributeName(id: string): string {
  return ATTRIBUTES.find((a) => a.id === id)?.name ?? id;
}

// ── 命令输出 ──
const command = computed(() => {
  if (!selectedItem.value) return "";
  return buildGiveCommand(
    {
      itemId: selectedItem.value.id,
      count: count.value,
      colorCode: colorCode.value,
      customName: customName.value,
      lore: loreLines(),
      unbreakable: unbreakable.value,
      hideTooltip: hideTooltip.value,
      enchantments: enchantments.value,
      attributes: attributes.value,
    },
    format.value,
  );
});

async function copyCommand() {
  if (!command.value) return;
  await copyText(command.value, "命令已复制");
}
</script>

<template>
  <div class="tool-card">
    <div class="tool-card-hdr">
      <h3>/give 生成器</h3>
      <div class="tool-card-sub">物品 ID 按 1.13+ 命名空间格式（主流版本稳定）；1.12 及以下请勿使用</div>
    </div>

    <!-- 格式选择 -->
    <div class="give-row">
      <span class="give-label">命令格式</span>
      <DropSelect
        :model-value="format"
        :options="formatOptions"
        width="200px"
        @update:model-value="(v: string) => (format = v as GiveFormat)"
      />
    </div>

    <!-- 物品选择 -->
    <div class="give-sec">
      <div class="give-row">
        <span class="give-label">物品</span>
        <div class="give-search">
          <Search :size="13" class="give-search-icon" />
          <input
            v-model="keyword"
            class="give-input"
            placeholder="搜索物品（ID 或中文名）…"
            spellcheck="false"
            autocomplete="off"
          />
        </div>
        <DropSelect
          :model-value="cat"
          :options="catOptions"
          width="96px"
          @update:model-value="(v: string) => (cat = v)"
        />
      </div>
      <div class="give-list">
        <button
          v-for="it in items"
          :key="it.id"
          class="give-item"
          :class="{ active: selectedItem?.id === it.id }"
          @click="selectItem(it)"
        >
          <span class="give-item-name">{{ it.name }}</span>
          <span class="give-item-id">minecraft:{{ it.id }}</span>
        </button>
        <div v-if="items.length === 0" class="give-empty">无匹配物品</div>
      </div>
    </div>

    <template v-if="selectedItem">
      <!-- 基础配置 -->
      <div class="give-sec">
        <div class="give-row">
          <span class="give-label">数量</span>
          <input v-model.number="count" type="number" class="give-num" min="1" max="99" autocomplete="off" />
        </div>
        <div class="give-row">
          <span class="give-label">名称颜色</span>
          <button
            class="give-color-none"
            :class="{ active: colorCode === '' }"
            title="不染色"
            @click="colorCode = ''"
          >无</button>
          <button
            v-for="key in MC_COLOR_KEYS"
            :key="key"
            class="give-color"
            :class="{ active: colorCode === `§${key}` }"
            :style="{ backgroundColor: MC_COLORS[key] }"
            :title="`§${key} ${MC_COLOR_NAMES[key]}`"
            @click="colorCode = `§${key}`"
          />
          <input
            type="color"
            class="give-color-custom"
            :value="customColor"
            :class="{ active: colorCode.startsWith('§x') }"
            title="自定义颜色（§x，需 1.16+）"
            @input="onCustomColor"
            autocomplete="off"
          />
        </div>
        <div class="give-row">
          <span class="give-label">自定义名称</span>
          <input
            v-model="customName"
            class="give-input"
            placeholder="留空使用默认名称"
            spellcheck="false"
            autocomplete="off"
          />
        </div>
        <div class="give-row give-row-top">
          <span class="give-label">背景故事</span>
          <textarea
            v-model="loreText"
            class="give-textarea"
            rows="2"
            placeholder="每行一条（可多行）"
            spellcheck="false"
            autocomplete="off"
          ></textarea>
        </div>
        <div class="give-checks">
          <CheckToggle v-model="unbreakable" :size="14">破坏无效</CheckToggle>
          <CheckToggle v-model="hideTooltip" :size="14">隐藏信息框</CheckToggle>
        </div>
      </div>

      <!-- 附魔 -->
      <div class="give-sec">
        <div class="give-sec-title">附魔（不受原版限制）</div>
        <div class="give-row">
          <DropSelect
            :model-value="newEnch"
            :options="enchOptions"
            width="180px"
            placeholder="选择附魔"
            @update:model-value="(v: string) => (newEnch = v)"
          />
          <input v-model.number="newEnchLevel" type="number" class="give-num" min="1" max="255" autocomplete="off" />
          <button class="tool-btn give-add-btn" :disabled="!newEnch" @click="addEnchant">
            <Plus :size="14" />
            添加
          </button>
        </div>
        <div v-if="enchantments.length > 0" class="give-tags">
          <span v-for="(e, i) in enchantments" :key="i" class="give-tag">
            {{ enchantName(e.id) }} {{ e.level }}
            <button class="nb-tag-x" @click="removeEnchant(i)">
              <Trash2 :size="12" />
            </button>
          </span>
        </div>
      </div>

      <!-- 属性 -->
      <div class="give-sec">
        <div class="give-sec-title">属性（加到物品基础值上）</div>
        <div class="give-row">
          <DropSelect
            :model-value="newAttr"
            :options="attrOptions"
            width="180px"
            placeholder="选择属性"
            @update:model-value="(v: string) => (newAttr = v)"
          />
          <input v-model.number="newAttrAmount" type="number" class="give-num" min="0" autocomplete="off" />
          <button class="tool-btn give-add-btn" :disabled="!newAttr" @click="addAttribute">
            <Plus :size="14" />
            添加
          </button>
        </div>
        <div v-if="attributes.length > 0" class="give-tags">
          <span v-for="(a, i) in attributes" :key="i" class="give-tag">
            {{ attributeName(a.id) }} +{{ a.amount }}
            <button class="nb-tag-x" @click="removeAttribute(i)">
              <Trash2 :size="12" />
            </button>
          </span>
        </div>
      </div>

      <!-- 输出 -->
      <div class="give-output">
        <div class="give-output-head">
          <span>命令 /GIVE（{{ format === "modern" ? "1.20.5+" : "1.13–1.20.4" }}）</span>
          <button class="tool-btn give-copy-btn" @click="copyCommand">
            <Copy :size="14" />
            复制
          </button>
        </div>
        <code class="gen-output gen-output-cmd">{{ command }}</code>
        <div v-if="format === 'legacy' && attributes.length > 0" class="give-hint">
          属性修饰符每次生成随机 UUID（避免多次 give 相同属性不叠加）
        </div>
      </div>
    </template>

    <div v-else class="give-empty give-hint">请先选择物品</div>
  </div>
</template>

<style>
@import "../../styles/tools-shared.css";
</style>

<style scoped>
.tool-card {
  background: var(--surface-elevated);
  border: 1px solid var(--border-default);
  border-radius: var(--radius-md);
  padding: 16px 18px;
}

.tool-card-hdr {
  margin-bottom: 14px;
}

.tool-card-hdr h3 {
  margin: 0 0 4px;
  font-size: 15px;
  color: var(--text-primary);
}

.tool-card-sub {
  font-size: 12px;
  color: var(--text-tertiary);
}

/* ===== 通用行 ===== */
.give-row {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-bottom: 8px;
  flex-wrap: wrap;
}

.give-row-top {
  align-items: flex-start;
}

.give-label {
  flex: 0 0 auto;
  font-size: 12.5px;
  color: var(--text-secondary);
  min-width: 56px;
}

.give-input {
  flex: 1;
  min-width: 0;
  height: 30px;
  padding: 0 10px;
  font-size: 13px;
  color: var(--text-primary);
  background-color: var(--surface-page);
  border: 1px solid var(--border-default);
  border-radius: var(--radius-sm);
  outline: none;
}

.give-input:hover,
.give-input:focus {
  border-color: var(--color-brand);
}

.give-num {
  width: 64px;
  height: 30px;
  padding: 0 8px;
  font-size: 13px;
  color: var(--text-primary);
  background-color: var(--surface-page);
  border: 1px solid var(--border-default);
  border-radius: var(--radius-sm);
  outline: none;
}

.give-num:focus {
  border-color: var(--color-brand);
}

.give-search {
  position: relative;
  flex: 1;
  min-width: 0;
}

.give-search-icon {
  position: absolute;
  left: 9px;
  top: 50%;
  transform: translateY(-50%);
  color: var(--text-tertiary);
  pointer-events: none;
}

.give-search .give-input {
  padding-left: 28px;
}

/* ===== 分区 ===== */
.give-sec {
  padding: 10px 0;
  border-top: 1px solid var(--border-default);
}

.give-sec-title {
  font-size: 13px;
  font-weight: 600;
  color: var(--text-primary);
  margin-bottom: 8px;
}

/* ===== 物品列表 ===== */
.give-list {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  max-height: 132px;
  overflow-y: auto;
  padding: 4px 2px;
}

.give-item {
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  gap: 1px;
  padding: 5px 10px;
  background-color: var(--surface-page);
  border: 1px solid var(--border-default);
  border-radius: var(--radius-sm);
  cursor: pointer;
  transition: all 0.15s;
  text-align: left;
}

.give-item:hover {
  border-color: var(--color-brand);
  color: var(--text-primary);
}

.give-item.active {
  background-color: var(--color-brand);
  border-color: var(--color-brand);
}

.give-item-name {
  font-size: 12px;
  color: var(--text-primary);
  line-height: 1.3;
}

.give-item.active .give-item-name {
  color: #fff;
}

.give-item-id {
  font-family: "Cascadia Mono", Consolas, "Courier New", monospace;
  font-size: 10px;
  color: var(--text-tertiary);
  line-height: 1.3;
}

.give-item.active .give-item-id {
  color: rgba(255, 255, 255, 0.75);
}

.give-empty {
  font-size: 12px;
  color: var(--text-tertiary);
  padding: 8px 0;
}

/* ===== 名称颜色 ===== */
.give-color-none {
  height: 22px;
  padding: 0 8px;
  font-size: 11px;
  color: var(--text-secondary);
  background-color: var(--surface-page);
  border: 1px solid var(--border-default);
  border-radius: var(--radius-sm);
  cursor: pointer;
}

.give-color-none.active {
  color: var(--color-brand);
  border-color: var(--color-brand);
}

.give-color {
  width: 18px;
  height: 18px;
  border: 1px solid var(--border-strong);
  border-radius: 3px;
  padding: 0;
  cursor: pointer;
  transition: transform 0.1s;
}

.give-color:hover {
  transform: scale(1.15);
}

.give-color.active {
  outline: 2px solid var(--color-brand);
  outline-offset: 1px;
}

.give-color-custom {
  width: 20px;
  height: 20px;
  padding: 0;
  border: 1px solid var(--border-strong);
  border-radius: 3px;
  background: none;
  cursor: pointer;
}

.give-color-custom.active {
  outline: 2px solid var(--color-brand);
  outline-offset: 1px;
}

/* ===== 背景故事 ===== */
.give-textarea {
  flex: 1;
  min-width: 0;
  padding: 6px 10px;
  font-size: 13px;
  line-height: 1.5;
  color: var(--text-primary);
  background-color: var(--surface-page);
  border: 1px solid var(--border-default);
  border-radius: var(--radius-sm);
  resize: none;
  outline: none;
}

.give-textarea:focus {
  border-color: var(--color-brand);
}

/* ===== 选项勾选 ===== */
.give-checks {
  display: flex;
  gap: 16px;
  padding: 2px 0;
}

/* ===== 附魔/属性标签 ===== */
.give-add-btn {
  font-size: 12px;
  padding: 4px 10px;
}

.give-tags {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  margin-top: 8px;
}

.give-tag {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  padding: 3px 6px 3px 10px;
  font-size: 12px;
  color: var(--text-secondary);
  background-color: var(--surface-page);
  border: 1px solid var(--border-default);
  border-radius: 999px;
}

/* ===== 输出 ===== */
.give-output-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  font-size: 11px;
  color: var(--text-tertiary);
  margin-bottom: 6px;
}

.give-copy-btn {
  font-size: 12px;
  padding: 3px 10px;
}

.give-hint {
  font-size: 11px;
  color: var(--text-tertiary);
  margin-top: 6px;
}
</style>
