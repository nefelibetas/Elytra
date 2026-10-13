<script setup lang="ts">
// 村民交易生成器：职业/等级/类型 + 自定义交易列表（买/卖物品），双版本语法输出。
// 实体 NBT 未组件化（VillagerData/Offers/Recipes 两代通用），差异仅在物品槽格式。
import { computed, ref } from "vue";
import { Copy, Plus, Trash2, SlidersHorizontal } from "@lucide/vue";
import { reactive } from "vue";
import DropSelect, { type SelOption } from "../common/DropSelect.vue";
import CheckToggle from "../common/CheckToggle.vue";
import ItemSearchInput from "./ItemSearchInput.vue";
import ModsEditor, { type ModsForm } from "./ModsEditor.vue";
import {
  VILLAGER_PROFESSIONS, VILLAGER_TYPES, buildVillagerCommand, isKnownItem,
  type VillagerTrade,
} from "../../utils/nbt";
import {
  GIVE_ITEMS, ENCHANTMENTS, GIVE_FORMAT_OPTIONS, type ItemMods, type GiveFormat,
} from "../../utils/give";
import { copyText } from "../../utils/clipboard";

const format = ref<GiveFormat>("modern");
const formatOptions: SelOption[] = GIVE_FORMAT_OPTIONS.map((o) => ({
  value: o.value,
  label: o.label,
}));

const professionOptions: SelOption[] = VILLAGER_PROFESSIONS.map((p) => ({
  value: p.id,
  label: p.name,
}));
const typeOptions: SelOption[] = VILLAGER_TYPES.map((t) => ({
  value: t.id,
  label: t.name,
}));

const profession = ref("librarian");
const type = ref("plains");
const level = ref(2);

// 交易列表
const trades = ref<VillagerTrade[]>([]);
const buyId = ref("emerald");
const buyCount = ref(1);
const buyBId = ref("");
const buyBCount = ref(1);
const sellId = ref("book");
const sellCount = ref(1);
const maxUses = ref(12);
const rewardExp = ref(true);
const addError = ref("");

/** 交易物品修饰表单（购买/第二购买/出售）。 */
const modsForms = reactive<Record<TradeItemKey, ModsForm>>({
  buy: { name: "", colorCode: "", enchants: [] },
  buyB: { name: "", colorCode: "", enchants: [] },
  sell: { name: "", colorCode: "", enchants: [] },
});
type TradeItemKey = "buy" | "buyB" | "sell";
/** 当前展开的修饰编辑区（同一时间只展开一个）。 */
const editingKey = ref<TradeItemKey | null>(null);

function toggleEditing(key: TradeItemKey) {
  editingKey.value = editingKey.value === key ? null : key;
}

/** 表单 → ItemMods（全空返回 undefined = 不自定义）。 */
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
  };
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
  trades.value.push({
    buyId: buyId.value.trim(),
    buyCount: Math.max(1, Math.min(buyCount.value, 64)),
    buyMods: modsToItemMods(modsForms.buy),
    buyBId: buyBId.value.trim(),
    buyBCount: Math.max(1, Math.min(buyBCount.value, 64)),
    buyBMods: modsToItemMods(modsForms.buyB),
    sellId: sellId.value.trim(),
    sellCount: Math.max(1, Math.min(sellCount.value, 64)),
    sellMods: modsToItemMods(modsForms.sell),
    maxUses: Math.max(1, Math.min(maxUses.value, 9999)),
    rewardExp: rewardExp.value,
  });
}

function removeTrade(idx: number) {
  trades.value.splice(idx, 1);
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

const command = computed(() =>
  buildVillagerCommand(
    {
      profession: profession.value,
      type: type.value,
      level: level.value,
      trades: trades.value,
    },
    format.value,
  ),
);

async function copyCommand() {
  await copyText(command.value, "命令已复制");
}
</script>

<template>
  <div class="tool-card">
    <div class="tool-card-hdr">
      <h3>村民交易生成器</h3>
      <div class="tool-card-sub">职业/等级 + 自定义交易（买/卖物品）——输出 /summon 命令</div>
    </div>

    <div class="nb-row">
      <span class="nb-label">命令格式</span>
      <DropSelect
        :model-value="format"
        :options="formatOptions"
        width="200px"
        @update:model-value="(v: string) => (format = v as GiveFormat)"
      />
    </div>

    <div class="nb-sec">
      <div class="nb-row">
        <span class="nb-label">职业</span>
        <DropSelect
          :model-value="profession"
          :options="professionOptions"
          width="140px"
          @update:model-value="(v: string) => (profession = v)"
        />
        <span class="nb-label nb-label-short">类型</span>
        <DropSelect
          :model-value="type"
          :options="typeOptions"
          width="110px"
          @update:model-value="(v: string) => (type = v)"
        />
        <span class="nb-label nb-label-short">等级</span>
        <input v-model.number="level" type="number" class="nb-num" min="1" max="5" autocomplete="off" />
      </div>
    </div>

    <div class="nb-sec">
      <div class="nb-sec-title">交易（可添加多条；物品填 ID，如 emerald / book）</div>
      <div class="nb-row">
        <span class="nb-label">购买</span>
        <ItemSearchInput v-model="buyId" :extra-ids="['emerald']" placeholder="绿宝石 / emerald" :max-version="format === 'legacy' ? 20 : 21" />
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
      <ModsEditor v-if="editingKey === 'buy'" :form="modsForms.buy" :format="format" />
      <div class="nb-row">
        <span class="nb-label">第二购买</span>
        <ItemSearchInput v-model="buyBId" :extra-ids="['emerald']" placeholder="可选，搜索物品" :max-version="format === 'legacy' ? 20 : 21" />
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
      <ModsEditor v-if="editingKey === 'buyB'" :form="modsForms.buyB" :format="format" />
      <div class="nb-row">
        <span class="nb-label">出售</span>
        <ItemSearchInput v-model="sellId" placeholder="书 / book" :max-version="format === 'legacy' ? 20 : 21" />
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
      <ModsEditor v-if="editingKey === 'sell'" :form="modsForms.sell" :format="format" />
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

    <div class="nb-output">
      <div class="nb-output-head">
        <span>命令 /SUMMON（{{ format === "modern" ? "1.20.5+" : "1.13–1.20.4" }}）</span>
        <button class="tool-btn nb-copy-btn" @click="copyCommand">
          <Copy :size="14" />
          复制
        </button>
      </div>
      <code class="gen-output gen-output-cmd">{{ command }}</code>
    </div>
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

.nb-label-short {
  min-width: 0;
  margin-left: 8px;
}

.nb-input {
  flex: 1;
  min-width: 80px;
  height: 30px;
  padding: 0 10px;
  font-size: 13px;
  color: var(--text-primary);
  background-color: var(--surface-page);
  border: 1px solid var(--border-default);
  border-radius: var(--radius-sm);
  outline: none;
}

.nb-input-sm {
  max-width: 180px;
  flex: 0 0 auto;
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
  background-color: var(--surface-page);
  border: 1px solid var(--border-default);
  border-radius: var(--radius-sm);
  outline: none;
}

.nb-num:focus {
  border-color: var(--color-brand);
}

.nb-sec {
  padding: 10px 0;
  border-top: 1px solid var(--border-default);
}

.nb-sec-title {
  font-size: 13px;
  font-weight: 600;
  color: var(--text-primary);
  margin-bottom: 8px;
}

.nb-add-btn {
  font-size: 12px;
  padding: 4px 10px;
  margin-left: auto;
}

.nb-mini-btn {
  font-size: 12px;
  padding: 4px 10px;
  display: inline-flex;
  align-items: center;
  gap: 4px;
}

.nb-mini-btn.active {
  color: var(--color-brand);
  border-color: var(--color-brand);
}

.nb-error {
  margin-top: 4px;
  font-size: 12px;
  color: var(--color-danger);
}

.nb-tags {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  margin-top: 8px;
}

.nb-tag {
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

.nb-output-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  font-size: 11px;
  color: var(--text-tertiary);
  margin-bottom: 6px;
}

.nb-copy-btn {
  font-size: 12px;
  padding: 3px 10px;
}

</style>
