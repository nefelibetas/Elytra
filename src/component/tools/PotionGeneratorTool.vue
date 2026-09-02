<script setup lang="ts">
// 自定义药水：效果组合（39 种）+ 基础药水 + 自定义颜色 + 名称，双版本语法输出。
// legacy：Potion/CustomPotionEffects/CustomColor NBT；modern：potion_contents 组件。
import { computed, ref } from "vue";
import { Copy, Plus, Trash2 } from "@lucide/vue";
import DropSelect, { type SelOption } from "../common/DropSelect.vue";
import {
  POTION_EFFECTS, POTION_BASES, POTION_ITEM_KINDS,
  buildPotionCommand, rgbToInt, filterEffectsByVersion,
  type PotionEffectInput,
} from "../../utils/nbt";
import { MC_COLORS, MC_COLOR_KEYS, MC_COLOR_NAMES, hexToSectionX } from "../../utils/motd";
import { showToast } from "../../utils/toast";
import type { GiveFormat } from "../../utils/give";

const format = ref<GiveFormat>("modern");
const formatOptions: SelOption[] = [
  { value: "modern", label: "1.20.5+（组件格式）" },
  { value: "legacy", label: "1.13–1.20.4（NBT 格式）" },
];

const kindOptions: SelOption[] = POTION_ITEM_KINDS.map((k) => ({ value: k.id, label: k.name }));
const baseOptions: SelOption[] = [
  { value: "", label: "无（基础药水）" },
  ...POTION_BASES.map((b) => ({ value: b.id, label: b.name })),
];

const itemKind = ref("potion");
const potionId = ref("");
const count = ref(1);

// 效果列表
const effects = ref<PotionEffectInput[]>([]);
const newEffect = ref("");
const newAmp = ref(1);
const newDurSec = ref(60);

const effectOptions = computed<SelOption[]>(() =>
  filterEffectsByVersion(POTION_EFFECTS, format.value).map((e) => ({
    value: e.id,
    label: e.name,
  })),
);

function addEffect() {
  if (!newEffect.value) return;
  effects.value.push({
    id: newEffect.value,
    amplifier: Math.max(0, Math.min(newAmp.value, 255)),
    duration: Math.max(1, Math.min(Math.floor(newDurSec.value * 20) || 1, 1000000)),
  });
  newEffect.value = "";
  newAmp.value = 1;
  newDurSec.value = 60;
}

function removeEffect(idx: number) {
  effects.value.splice(idx, 1);
}

function effectName(id: string): string {
  return POTION_EFFECTS.find((e) => e.id === id)?.name ?? id;
}

// 自定义颜色（RGB packed int；null = 液体默认色）
const customColor = ref<number | null>(null);
const customColorHex = ref("#ff5500");

function toggleCustomColor() {
  customColor.value = customColor.value === null ? rgbToInt(customColorHex.value) : null;
}

function onCustomColor(e: Event) {
  customColorHex.value = (e.target as HTMLInputElement).value;
  customColor.value = rgbToInt(customColorHex.value);
}

// 名称（16 色/自定义 §x）
const customName = ref("");
const colorCode = ref("");

function onCustomNameColor(e: Event) {
  const v = (e.target as HTMLInputElement).value;
  const sx = hexToSectionX(v);
  if (sx) colorCode.value = sx;
}

const command = computed(() =>
  buildPotionCommand(
    {
      itemKind: itemKind.value,
      potionId: potionId.value,
      effects: effects.value,
      customColor: customColor.value,
      customName: customName.value,
      colorCode: colorCode.value,
      count: count.value,
    },
    format.value,
  ),
);

async function copyCommand() {
  try {
    await navigator.clipboard.writeText(command.value);
    showToast("命令已复制");
  } catch {
    showToast("复制失败，请手动复制");
  }
}
</script>

<template>
  <div class="tool-card">
    <div class="tool-card-hdr">
      <h3>药水生成器</h3>
      <div class="tool-card-sub">组合 39 种状态效果、基础药水与自定义颜色——输出 /give 命令</div>
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
        <span class="nb-label">物品</span>
        <DropSelect
          :model-value="itemKind"
          :options="kindOptions"
          width="140px"
          @update:model-value="(v: string) => (itemKind = v)"
        />
        <span class="nb-label nb-label-short">基础药水</span>
        <DropSelect
          :model-value="potionId"
          :options="baseOptions"
          width="150px"
          placeholder="无（基础药水）"
          @update:model-value="(v: string) => (potionId = v)"
        />
        <span class="nb-label nb-label-short">数量</span>
        <input v-model.number="count" type="number" class="nb-num" min="1" max="64" />
      </div>
    </div>

    <!-- 效果 -->
    <div class="nb-sec">
      <div class="nb-sec-title">状态效果（可叠加多个）</div>
      <div class="nb-row">
        <DropSelect
          :model-value="newEffect"
          :options="effectOptions"
          width="150px"
          placeholder="选择效果"
          @update:model-value="(v: string) => (newEffect = v)"
        />
        <span class="nb-label nb-label-short">等级</span>
        <input v-model.number="newAmp" type="number" class="nb-num" min="0" max="255" />
        <span class="nb-label nb-label-short">时长（秒）</span>
        <input v-model.number="newDurSec" type="number" class="nb-num" min="1" />
        <button class="tool-btn nb-add-btn" :disabled="!newEffect" @click="addEffect">
          <Plus :size="14" />
          添加
        </button>
      </div>
      <div v-if="effects.length > 0" class="nb-tags">
        <span v-for="(e, i) in effects" :key="i" class="nb-tag">
          {{ effectName(e.id) }} {{ e.amplifier }} · {{ Math.round(e.duration / 20) }} 秒
          <button class="nb-tag-x" @click="removeEffect(i)">
            <Trash2 :size="12" />
          </button>
        </span>
      </div>
    </div>

    <!-- 颜色与名称 -->
    <div class="nb-sec">
      <div class="nb-row">
        <span class="nb-label">液体颜色</span>
        <label class="nb-check">
          <input
            type="checkbox"
            :checked="customColor !== null"
            @change="toggleCustomColor"
          />
          自定义
        </label>
        <input
          v-if="customColor !== null"
          type="color"
          class="nb-color-pick"
          :value="customColorHex"
          @input="onCustomColor"
        />
        <span v-if="customColor === null" class="nb-hint">默认液体颜色</span>
      </div>
      <div class="nb-row">
        <span class="nb-label">名称</span>
        <input
          v-model="customName"
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
          class="nb-color"
          :class="{ active: colorCode === `§${key}` }"
          :style="{ backgroundColor: MC_COLORS[key] }"
          :title="`§${key} ${MC_COLOR_NAMES[key]}`"
          @click="colorCode = `§${key}`"
        />
        <input
          type="color"
          class="nb-color-pick"
          :class="{ active: colorCode.startsWith('§x') }"
          title="自定义颜色（§x，需 1.16+）"
          @input="onCustomNameColor"
        />
      </div>
    </div>

    <!-- 输出 -->
    <div class="nb-output">
      <div class="nb-output-head">
        <span>命令 /GIVE（{{ format === "modern" ? "1.20.5+" : "1.13–1.20.4" }}）</span>
        <button class="tool-btn nb-copy-btn" @click="copyCommand">
          <Copy :size="14" />
          复制
        </button>
      </div>
      <code class="nb-cmd">{{ command }}</code>
    </div>
  </div>
</template>

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

.nb-check {
  display: flex;
  align-items: center;
  gap: 5px;
  font-size: 12.5px;
  color: var(--text-secondary);
  cursor: pointer;
}

.nb-check input {
  accent-color: var(--color-brand);
}

.nb-hint {
  font-size: 11px;
  color: var(--text-tertiary);
}

.nb-color {
  width: 18px;
  height: 18px;
  border: 1px solid var(--border-strong);
  border-radius: 3px;
  padding: 0;
  cursor: pointer;
  transition: transform 0.1s;
}

.nb-color:hover {
  transform: scale(1.15);
}

.nb-color.active {
  outline: 2px solid var(--color-brand);
  outline-offset: 1px;
}

.nb-color-pick {
  width: 20px;
  height: 20px;
  padding: 0;
  border: 1px solid var(--border-strong);
  border-radius: 3px;
  background: none;
  cursor: pointer;
}

.nb-color-pick.active {
  outline: 2px solid var(--color-brand);
  outline-offset: 1px;
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

.nb-cmd {
  display: block;
  padding: 10px 12px;
  font-family: "Cascadia Mono", Consolas, "Courier New", monospace;
  font-size: 12px;
  line-height: 1.6;
  color: var(--text-primary);
  background-color: rgba(0, 0, 0, 0.35);
  border-radius: var(--radius-sm);
  word-break: break-all;
  white-space: pre-wrap;
}
</style>
