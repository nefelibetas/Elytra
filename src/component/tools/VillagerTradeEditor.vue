<script setup lang="ts">
// 村民交易列表编辑器（村民交易生成器与「生物 NBT」工具共用）：原版预设回填 + 买/第二买/卖
// 自由配置（含每槽修饰与自定义 NBT）+ 已添加交易列表。trades 由父组件持有并直接变更。
import { computed, reactive, ref } from "vue";
import { Plus, SlidersHorizontal, Trash2 } from "@lucide/vue";
import DropSelect, { type SelOption } from "../common/DropSelect.vue";
import CheckToggle from "../common/CheckToggle.vue";
import SearchSelect from "./SearchSelect.vue";
import ModsEditor, { type ModsForm } from "./ModsEditor.vue";
import { isKnownItem, type SlotRaw, type VillagerTrade } from "../../utils/nbt";
import { VILLAGER_TRADE_PRESETS } from "../../utils/villagerTrades";
import { GIVE_ITEMS, ENCHANTMENTS, type ItemMods, type GiveVersion } from "../../utils/give";

const props = defineProps<{
  /** 已添加的交易列表（父持有；本组件直接 push/splice）。 */
  trades: VillagerTrade[];
  version: GiveVersion;
  /** 当前职业/等级：用于原版预设过滤（等级 ≤ 当前）。 */
  profession: string;
  level: number;
}>();

// 输入区
const buyId = ref("emerald");
const buyCount = ref(1);
const buyBId = ref("");
const buyBCount = ref(1);
const sellId = ref("book");
const sellCount = ref(1);
const maxUses = ref(12);
const rewardExp = ref(true);
const addError = ref("");

type TradeItemKey = "buy" | "buyB" | "sell";

/** 交易物品修饰表单（购买/第二购买/出售）。 */
const modsForms = reactive<Record<TradeItemKey, ModsForm>>({
  buy: { name: "", colorCode: "", enchants: [], rawLegacy: "", rawModern: "" },
  buyB: { name: "", colorCode: "", enchants: [], rawLegacy: "", rawModern: "" },
  sell: { name: "", colorCode: "", enchants: [], rawLegacy: "", rawModern: "" },
});
/** 当前展开的修饰编辑区（同一时间只展开一个）。 */
const editingKey = ref<TradeItemKey | null>(null);

function toggleEditing(key: TradeItemKey) {
  editingKey.value = editingKey.value === key ? null : key;
}

/** 表单 → ItemMods（全空返回 undefined = 不自定义）；自定义 NBT 走槽级 SlotRaw，不经 ItemMods。 */
function modsToItemMods(form: ModsForm): ItemMods | undefined {
  if (!form.name.trim() && !form.colorCode && form.enchants.length === 0) {
    return undefined;
  }
  return {
    colorCode: form.colorCode,
    customName: form.name.trim(),
    lore: [],
    unbreakable: false,
    hideTooltip: false,
    enchantments: form.enchants.map((e) => ({ ...e })),
    attributes: [],
    rawLegacy: "",
    rawModern: "",
  };
}

/** 表单 → 槽级自定义 NBT（两档分开；全空返回 undefined）。 */
function modsToSlotRaw(form: ModsForm): SlotRaw | undefined {
  if (!form.rawLegacy.trim() && !form.rawModern.trim()) return undefined;
  return { legacy: form.rawLegacy, modern: form.rawModern };
}

/** 原版交易预设：按当前职业 + 等级（≤ 当前等级，与该等级村民持有的交易池一致）过滤。 */
const presetOptions = computed<SelOption[]>(() =>
  VILLAGER_TRADE_PRESETS.filter((t) => t.p === props.profession && t.lv <= props.level).map(
    (t) => ({
      value: String(VILLAGER_TRADE_PRESETS.indexOf(t)),
      label: `[Lv${t.lv}] ${presetSide(t.want)} → ${presetSide(t.get)}`,
    }),
  ),
);

function itemNameOf(id: string): string {
  return GIVE_ITEMS.find((i) => i.id === id)?.name ?? id;
}

function presetSide(items: { id: string; n: number }[]): string {
  return items.map((i) => `${i.n}×${itemNameOf(i.id)}`).join(" + ");
}

/** 选中预设 → 回填买/第二买/卖/次数（不直接加入列表，用户可改后点添加）。 */
function applyPreset(indexStr: string) {
  const t = VILLAGER_TRADE_PRESETS[Number(indexStr)];
  if (!t) return;
  buyId.value = t.want[0]?.id ?? "";
  buyCount.value = t.want[0]?.n ?? 1;
  buyBId.value = t.want[1]?.id ?? "";
  buyBCount.value = t.want[1]?.n ?? 1;
  sellId.value = t.get[0]?.id ?? "";
  sellCount.value = t.get[0]?.n ?? 1;
  if (t.uses != null) maxUses.value = t.uses;
}

function addTrade() {
  addError.value = "";
  if (!isKnownItem(buyId.value.trim())) {
    addError.value = `购买物品不存在：${buyId.value.trim()}`;
    return;
  }
  if (!isKnownItem(sellId.value.trim())) {
    addError.value = `出售物品不存在：${sellId.value.trim()}`;
    return;
  }
  if (buyBId.value.trim() && !isKnownItem(buyBId.value.trim())) {
    addError.value = `第二购买物品不存在：${buyBId.value.trim()}`;
    return;
  }
  props.trades.push({
    buyId: buyId.value.trim(),
    buyCount: Math.max(1, Math.min(buyCount.value, 64)),
    buyMods: modsToItemMods(modsForms.buy),
    buyRaw: modsToSlotRaw(modsForms.buy),
    buyBId: buyBId.value.trim(),
    buyBCount: Math.max(1, Math.min(buyBCount.value, 64)),
    buyBMods: modsToItemMods(modsForms.buyB),
    buyBRaw: modsToSlotRaw(modsForms.buyB),
    sellId: sellId.value.trim(),
    sellCount: Math.max(1, Math.min(sellCount.value, 64)),
    sellMods: modsToItemMods(modsForms.sell),
    sellRaw: modsToSlotRaw(modsForms.sell),
    maxUses: Math.max(1, Math.min(maxUses.value, 9999)),
    rewardExp: rewardExp.value,
  });
}

function removeTrade(idx: number) {
  props.trades.splice(idx, 1);
}

function itemName(id: string): string {
  return GIVE_ITEMS.find((i) => i.id === id)?.name ?? id;
}

/** 修饰摘要（附魔等），如 （锋利5）。 */
function modsSummary(mods?: ItemMods): string {
  if (!mods || mods.enchantments.length === 0) return "";
  const list = mods.enchantments
    .map((e) => `${ENCHANTMENTS.find((x) => x.id === e.id)?.name ?? e.id}${e.level}`)
    .join(",");
  return `（${list}）`;
}
</script>

<template>
  <div class="nb-sec">
    <div class="nb-sec-title">交易</div>
    <div class="nb-row">
      <span class="nb-label">原版交易</span>
      <DropSelect
        v-if="presetOptions.length > 0"
        :model-value="''"
        :options="presetOptions"
        width="320px"
        placeholder="从该职业原版交易选择（回填下方）"
        @update:model-value="applyPreset"
      />
      <span v-else class="nb-note">该职业暂无原版交易预设</span>
    </div>
    <div class="nb-row">
      <span class="nb-label">购买</span>
      <SearchSelect v-model="buyId" :options="GIVE_ITEMS" :extra-ids="['emerald']" placeholder="绿宝石 / emerald" />
      <span class="nb-label nb-label-short">×</span>
      <input v-model.number="buyCount" type="number" class="nb-num" min="1" max="64" autocomplete="off" />
      <button
        class="tool-btn nb-mini-btn"
        :class="{ active: editingKey === 'buy' }"
        @click="toggleEditing('buy')"
      >
        <SlidersHorizontal :size="13" />
        自定义
      </button>
    </div>
    <ModsEditor v-if="editingKey === 'buy'" :form="modsForms.buy" :version="version" />
    <div class="nb-row">
      <span class="nb-label">第二购买</span>
      <SearchSelect v-model="buyBId" :options="GIVE_ITEMS" :extra-ids="['emerald']" placeholder="可选，搜索物品" />
      <span class="nb-label nb-label-short">×</span>
      <input v-model.number="buyBCount" type="number" class="nb-num" min="1" max="64" autocomplete="off" />
      <button
        class="tool-btn nb-mini-btn"
        :class="{ active: editingKey === 'buyB' }"
        @click="toggleEditing('buyB')"
      >
        <SlidersHorizontal :size="13" />
        自定义
      </button>
    </div>
    <ModsEditor v-if="editingKey === 'buyB'" :form="modsForms.buyB" :version="version" />
    <div class="nb-row">
      <span class="nb-label">出售</span>
      <SearchSelect v-model="sellId" :options="GIVE_ITEMS" placeholder="书 / book" />
      <span class="nb-label nb-label-short">×</span>
      <input v-model.number="sellCount" type="number" class="nb-num" min="1" max="64" autocomplete="off" />
      <button
        class="tool-btn nb-mini-btn"
        :class="{ active: editingKey === 'sell' }"
        @click="toggleEditing('sell')"
      >
        <SlidersHorizontal :size="13" />
        自定义
      </button>
    </div>
    <ModsEditor v-if="editingKey === 'sell'" :form="modsForms.sell" :version="version" />
    <div class="nb-row">
      <span class="nb-label">次数上限</span>
      <input v-model.number="maxUses" type="number" class="nb-num" min="1" max="9999" autocomplete="off" />
      <CheckToggle v-model="rewardExp" :size="14">奖励经验</CheckToggle>
      <button class="tool-btn nb-add-btn" @click="addTrade">
        <Plus :size="14" />
        添加交易
      </button>
    </div>
    <div v-if="addError" class="nb-error">{{ addError }}</div>
    <div v-if="trades.length > 0" class="nb-tags">
      <span v-for="(t, i) in trades" :key="i" class="nb-tag">
        {{ t.buyCount }}×{{ itemName(t.buyId) }}{{ modsSummary(t.buyMods) }}
        <template v-if="t.buyBId"> + {{ t.buyBCount }}×{{ itemName(t.buyBId) }}{{ modsSummary(t.buyBMods) }}</template>
        → {{ t.sellCount }}×{{ itemName(t.sellId) }}{{ modsSummary(t.sellMods) }}（{{ t.maxUses }} 次）
        <button class="nb-tag-x" @click="removeTrade(i)">
          <Trash2 :size="12" />
        </button>
      </span>
    </div>
  </div>
</template>

<style>
@import "../../styles/tools-shared.css";
</style>

<style scoped>
.nb-mini-btn {
  font-size: 12px;
  padding: 4px 10px;
}

.nb-add-btn {
  font-size: 12px;
  padding: 4px 10px;
}

.nb-error {
  font-size: 12px;
  color: var(--color-danger);
  margin-bottom: 8px;
}
</style>
