<script setup lang="ts">
// 物品 NBT 生成：搜索选择物品 → 配置（名称颜色/背景故事/附魔/属性/选项/附加 NBT）→ 输出单个物品栈片段（供村民交易等 NBT 场景引用）。
// 版本区间决定语法与可用选项（1.7.10–1.20.4 为 NBT；1.20.5+ 为组件）。
import { computed, ref, watch } from "vue";
import { Copy, Plus, Trash2 } from "@lucide/vue";
import DropSelect, { type SelOption } from "../common/DropSelect.vue";
import CheckToggle from "../common/CheckToggle.vue";
import SearchSelect from "./SearchSelect.vue";
import FireworkEditor from "./FireworkEditor.vue";
import PotionEditor from "./PotionEditor.vue";
import {
  GIVE_ITEMS, ENCHANTMENTS, ATTRIBUTES,
  GIVE_VERSION_OPTIONS, DEFAULT_GIVE_VERSION, buildTradeItem,
  filterEnchantsByVersion, filterAttributesByVersion,
  supportsHexColor, syntaxOfVersion, maxVersionOf,
  type GiveVersion, type EnchantInput, type AttributeInput,
} from "../../utils/give";
import { checkNbtFragment, parseItemStackFragment } from "../../utils/nbtText";
import { MC_COLORS, MC_COLOR_KEYS, MC_COLOR_NAMES, hexToSectionX } from "../../utils/motd";
import { copyText } from "../../utils/clipboard";

/** 目标版本区间（决定语法与可用 NBT）。 */
const version = ref<GiveVersion>(DEFAULT_GIVE_VERSION);
const versionOptions: SelOption[] = GIVE_VERSION_OPTIONS.map((o) => ({
  value: o.value,
  label: o.label,
}));
const syntax = computed(() => syntaxOfVersion(version.value));
const hexColorSupported = computed(() => supportsHexColor(version.value));
/** 1.13 前使用扁平化前物品 ID/Damage。 */
const legacyItems = computed(() => maxVersionOf(version.value) < 13);
/** Unbreakable / HideFlags 自 1.8 起存在。 */
const unbreakableSupported = computed(() => maxVersionOf(version.value) >= 8);
/** Potion 标签自 1.9 起；更早版本药水类型由 Damage 决定。 */
const potionEditorSupported = computed(() => maxVersionOf(version.value) >= 9);
const versionRange = computed(
  () => GIVE_VERSION_OPTIONS.find((o) => o.value === version.value)?.range ?? version.value,
);

/** 物品 ID（搜索选择；空串 = 未选）。 */
const itemId = ref("");

// ── 配置 ──
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
  filterEnchantsByVersion(ENCHANTMENTS, version.value).map((e) => ({
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

const attrOptions = computed<SelOption[]>(() =>
  filterAttributesByVersion(ATTRIBUTES, version.value).map((a) => ({
    value: a.id,
    label: a.name,
  })),
);

function addAttribute() {
  if (!newAttr.value) return;
  if (!attrOptions.value.some((o) => o.value === newAttr.value)) return;
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

/** 切换版本区间：丢弃该档不存在的附魔/属性与 §x 颜色。 */
function onVersionChange(v: string) {
  version.value = v as GiveVersion;
  const enchantIds = new Set(filterEnchantsByVersion(ENCHANTMENTS, version.value).map((e) => e.id));
  enchantments.value = enchantments.value.filter((e) => enchantIds.has(e.id));
  if (newEnch.value && !enchantIds.has(newEnch.value)) newEnch.value = "";
  const attrIds = new Set(attrOptions.value.map((o) => o.value));
  attributes.value = attributes.value.filter((a) => attrIds.has(a.id));
  if (newAttr.value && !attrIds.has(newAttr.value)) newAttr.value = "";
  if (!hexColorSupported.value && colorCode.value.startsWith("§x")) colorCode.value = "";
  if (!unbreakableSupported.value) {
    unbreakable.value = false;
    hideTooltip.value = false;
  }
}

// ── 自定义 NBT 与输出 ──
// 附加内容（两档分开保存，切档不丢）：按物品栈/组件语法写（modern `"minecraft:key":value` / legacy `Key:value`）。
const rawModern = ref("");
const rawLegacy = ref("");

const rawText = computed({
  get: () => (syntax.value === "modern" ? rawModern.value : rawLegacy.value),
  set: (v: string) => {
    if (syntax.value === "modern") rawModern.value = v;
    else rawLegacy.value = v;
  },
});

const rawError = computed(() => (rawText.value.trim() ? checkNbtFragment(rawText.value) : null));

const rawPlaceholder = computed(() =>
  syntax.value === "modern"
    ? '附加组件（如 "minecraft:custom_model_data":7）'
    : "附加 NBT（如 CustomModelData:7）",
);

/** 烟花/药水专有编辑器产出的附加内文（随物品与格式；无条件块时为空）。 */
const editorInner = ref("");
watch([itemId, version], () => {
  editorInner.value = "";
});
/** 当前物品对应的专用编辑器类型（映射 give 物品 id）。 */
const editorKind = computed<"firework" | "potion" | null>(() => {
  if (!itemId.value) return null;
  if (itemId.value === "firework_rocket" || itemId.value === "firework_star") return "firework" as const;
  if (["potion", "splash_potion", "lingering_potion", "tipped_arrow"].includes(itemId.value)) {
    return "potion" as const;
  }
  return null;
});

/** 当前配置对应的 ItemMods（含附加 NBT；专有编辑器内文并入当前档，位于用户手写内容之前）。 */
function currentMods() {
  return {
    colorCode: colorCode.value,
    customName: customName.value,
    lore: loreLines(),
    unbreakable: unbreakable.value,
    hideTooltip: hideTooltip.value,
    enchantments: enchantments.value,
    attributes: attributes.value,
    rawModern:
      syntax.value === "modern"
        ? [editorInner.value.trim(), rawModern.value.trim()].filter((x) => x).join(",")
        : rawModern.value,
    rawLegacy:
      syntax.value === "legacy"
        ? [editorInner.value.trim(), rawLegacy.value.trim()].filter((x) => x).join(",")
        : rawLegacy.value,
  };
}

/** 单个物品栈片段：与村民交易槽同一构建（按版本区间输出）。 */
const output = computed(() => {
  if (!itemId.value) return "";
  return buildTradeItem(itemId.value, 1, currentMods(), version.value);
});

/** 附加 NBT 输入：粘贴完整物品栈片段（含 id）时自动导入——回填物品与组件内文，避免把整栈误当附加段。
 *  普通附加内容（无 id）原样保留。 */
function onRawInput(v: string) {
  rawText.value = v;
  const parsed = parseItemStackFragment(v);
  if (!parsed?.item) return;
  itemId.value = parsed.item;
  rawText.value = parsed.modernComponents ?? parsed.legacyTags ?? "";
}

async function copyOutput() {
  if (!output.value) return;
  await copyText(output.value, "物品栈片段已复制");
}

/** 完整指令：按版本档给出带同一物品栈的可用命令（1.13–1.20.4 的 /give 不支持 NBT，用掉落物形式）。 */
const outputCommand = computed(() => {
  const frag = output.value;
  if (!frag) return "";
  const modern = /^\{id:"([^"]+)",count:(\d+)(?:,components:\{(.*)\})?\}$/s.exec(frag);
  if (modern) {
    const [, id, , comps] = modern;
    return comps ? `/give @p ${id}[${comps}]` : `/give @p ${id}`;
  }
  const legacy = /^\{id:"([^"]+)",Count:(\d+)b(?:,Damage:(-?\d+)s)?(?:,tag:\{(.*)\})?\}$/s.exec(frag);
  if (legacy) {
    const [, id, count, damage, tag] = legacy;
    const oldGive = ["1.7.10", "1.8-1.8.9", "1.9-1.10.2", "1.11-1.11.2", "1.12.2"].includes(
      String(version.value),
    );
    if (oldGive) return `/give @p ${id} ${count} ${damage ?? "0"}${tag ? ` {${tag}}` : ""}`;
    return `/summon item ~ ~ ~ {Item:${frag}}`;
  }
  return "";
});

async function copyCommand() {
  if (!outputCommand.value) return;
  await copyText(outputCommand.value, "指令已复制");
}
</script>

<template>
  <div class="tool-card">
    <div class="tool-card-hdr">
      <h3>物品 NBT 生成</h3>
      <div class="tool-card-sub">构造物品栈片段，供指令生成器等场景引用</div>
    </div>

    <div class="give-row">
      <span class="give-label">版本区间</span>
      <DropSelect
        :model-value="version"
        :options="versionOptions"
        width="230px"
        @update:model-value="onVersionChange"
      />
    </div>

    <div class="give-sec">
      <div class="give-row">
        <span class="give-label">物品</span>
        <SearchSelect
          v-model="itemId"
          :options="GIVE_ITEMS"
          placeholder="搜索物品（ID 或中文名）…"
          max-width="360px"
        />
      </div>
    </div>

    <template v-if="itemId">
      <FireworkEditor
        v-if="editorKind === 'firework'"
        :item-kind="itemId"
        :version="version"
        @update:inner="(v: string) => (editorInner = v)"
      />
      <PotionEditor
        v-else-if="editorKind === 'potion' && potionEditorSupported"
        :item-kind="itemId"
        :version="version"
        @update:inner="(v: string) => (editorInner = v)"
      />
      <div v-else-if="editorKind === 'potion'" class="give-hint">
        1.9 之前的药水类型由物品 Damage 决定，当前只生成 1.9+ 的药水标签
      </div>

      <div class="give-sec">
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
            v-if="hexColorSupported"
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
        <div v-if="unbreakableSupported" class="give-checks">
          <CheckToggle v-model="unbreakable" :size="14">破坏无效</CheckToggle>
          <CheckToggle v-model="hideTooltip" :size="14">隐藏信息框</CheckToggle>
        </div>
        <div v-else class="give-hint">1.7.10 无 Unbreakable/HideFlags</div>
      </div>

      <div class="give-sec">
        <div class="give-sec-title">附魔</div>
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

      <div class="give-sec">
        <div class="give-sec-title">属性</div>
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

      <div class="give-output">
        <div class="give-output-head">
          <span>物品栈片段（{{ versionRange }}）</span>
          <button class="tool-btn give-copy-btn" @click="copyOutput">
            <Copy :size="14" />
            复制
          </button>
        </div>
        <div class="give-raw-row">
          <span class="give-raw-label">附加 NBT</span>
          <textarea
            :value="rawText"
            class="give-raw"
            rows="2"
            spellcheck="false"
            autocomplete="off"
            :placeholder="rawPlaceholder"
            @input="onRawInput(($event.target as HTMLTextAreaElement).value)"
          ></textarea>
        </div>
        <p v-if="rawError" class="give-raw-err">{{ rawError }}</p>
        <code class="gen-output gen-output-cmd">{{ output }}</code>
        <div class="give-output-head nb-output-gap">
          <span>完整指令（{{ versionRange }}）</span>
          <button class="tool-btn give-copy-btn" @click="copyCommand">
            <Copy :size="14" />
            复制
          </button>
        </div>
        <code class="gen-output gen-output-cmd">{{ outputCommand || "（配置后生成）" }}</code>
        <div class="give-hint">
          片段可直接粘贴到村民交易的「自定义 NBT」等 NBT 场景
        </div>
        <div v-if="syntax === 'legacy'" class="give-hint">
          旧版物品栈的自定义数据位于 tag 内；粘贴到 /give 附加段时会自动提取内层
        </div>
        <div v-if="legacyItems" class="give-hint">
          1.13 前使用扁平化前物品 ID/Damage；未映射或该版本之后新增的物品不可用
        </div>
        <div v-if="attributes.length > 0" class="give-hint">
          属性修饰符使用随机 UUID/id，避免同属性修饰不叠加
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
  /* 固定宽：label 随字数变宽会让各行控件左缘参差（实测 4 字 56px vs 5 字 63px） */
  flex: 0 0 64px;
  white-space: nowrap;
  font-size: 12.5px;
  color: var(--text-secondary);
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




.give-raw-row {
  display: flex;
  align-items: flex-start;
  gap: 8px;
  margin-bottom: 8px;
}

.give-raw-label {
  /* 与 .give-label 同宽，保证附加 NBT 的 textarea 与上方控件左缘对齐 */
  flex: 0 0 64px;
  white-space: nowrap;
  font-size: 12px;
  color: var(--text-secondary);
  line-height: 1.8;
}

.give-raw {
  flex: 1;
  padding: 5px 8px;
  font-family: Consolas, "Courier New", monospace;
  font-size: 12px;
  color: var(--text-primary);
  background-color: var(--surface-elevated);
  border: 1px solid var(--border-default);
  border-radius: var(--radius-sm);
  resize: vertical;
}

.give-raw:focus {
  outline: none;
  border-color: var(--color-brand);
}

.give-raw-err {
  margin: -4px 0 8px;
  font-size: 11px;
  color: var(--color-danger);
}
</style>
