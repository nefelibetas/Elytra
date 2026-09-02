<script setup lang="ts">
// 烟花生成器：爆炸形状/颜色/渐隐/拖尾/闪烁组合，双版本语法输出。
// legacy：Fireworks NBT（Flight/Explosions）；modern：fireworks 组件（has_twinkle）。
import { computed, ref } from "vue";
import { Copy, Plus, Trash2 } from "@lucide/vue";
import DropSelect, { type SelOption } from "../common/DropSelect.vue";
import CheckToggle from "../common/CheckToggle.vue";
import {
  FIREWORK_SHAPES, buildFireworkCommand, rgbToInt,
  type FireworkExplosion,
} from "../../utils/nbt";
import { MC_COLORS, MC_COLOR_KEYS, MC_COLOR_NAMES } from "../../utils/motd";
import { showToast } from "../../utils/toast";
import type { GiveFormat } from "../../utils/give";

const format = ref<GiveFormat>("modern");
const formatOptions: SelOption[] = [
  { value: "modern", label: "1.20.5+（组件格式）" },
  { value: "legacy", label: "1.13–1.20.4（NBT 格式）" },
];

const itemKind = ref("firework_rocket");
const kindOptions: SelOption[] = [
  { value: "firework_rocket", label: "烟花火箭" },
  { value: "firework_star", label: "烟花之星" },
];

const flight = ref(2);
const count = ref(1);

// 爆炸列表（火箭可多个；星只取第一个）
const explosions = ref<FireworkExplosion[]>([]);
const newShape = ref("small_ball");
const newColors = ref<number[]>([]);
const newFades = ref<number[]>([]);
const newTrail = ref(false);
const newTwinkle = ref(false);

const shapeOptions: SelOption[] = FIREWORK_SHAPES.map((s) => ({
  value: s.id,
  label: s.name,
}));

function toggleColor(list: number[], hex: string): number[] {
  const v = rgbToInt(hex);
  return list.includes(v) ? list.filter((c) => c !== v) : [...list, v];
}

function addExplosion() {
  if (newColors.value.length === 0) return;
  explosions.value.push({
    shape: newShape.value,
    colors: [...newColors.value],
    fadeColors: [...newFades.value],
    trail: newTrail.value,
    twinkle: newTwinkle.value,
  });
  newColors.value = [];
  newFades.value = [];
  newTrail.value = false;
  newTwinkle.value = false;
}

function removeExplosion(idx: number) {
  explosions.value.splice(idx, 1);
}

function shapeName(id: string): string {
  return FIREWORK_SHAPES.find((s) => s.id === id)?.name ?? id;
}

/** 颜色 int → 色板 key（显示用）。 */
function colorKeys(colors: number[]): string[] {
  return MC_COLOR_KEYS.filter((k) => colors.includes(rgbToInt(MC_COLORS[k])));
}

const command = computed(() =>
  buildFireworkCommand(
    {
      itemKind: itemKind.value,
      flight: flight.value,
      explosions: explosions.value,
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
      <h3>烟花生成器</h3>
      <div class="tool-card-sub">爆炸形状、主色与渐隐色、拖尾与闪烁——输出 /give 命令</div>
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
        <template v-if="itemKind === 'firework_rocket'">
          <span class="nb-label nb-label-short">飞行时长</span>
          <input v-model.number="flight" type="number" class="nb-num" min="1" max="3" />
        </template>
        <span class="nb-label nb-label-short">数量</span>
        <input v-model.number="count" type="number" class="nb-num" min="1" max="64" />
      </div>
    </div>

    <!-- 爆炸配置 -->
    <div class="nb-sec">
      <div class="nb-sec-title">
        {{ itemKind === "firework_rocket" ? "爆炸效果（可添加多个）" : "爆炸效果" }}
      </div>
      <div class="nb-row">
        <span class="nb-label">形状</span>
        <DropSelect
          :model-value="newShape"
          :options="shapeOptions"
          width="130px"
          @update:model-value="(v: string) => (newShape = v)"
        />
      </div>
      <div class="nb-row">
        <span class="nb-label">主色</span>
        <button
          v-for="key in MC_COLOR_KEYS"
          :key="key"
          class="nb-color"
          :class="{ active: newColors.includes(rgbToInt(MC_COLORS[key])) }"
          :style="{ backgroundColor: MC_COLORS[key] }"
          :title="`§${key} ${MC_COLOR_NAMES[key]}`"
          @click="newColors = toggleColor(newColors, MC_COLORS[key])"
        />
        <span v-if="newColors.length === 0" class="nb-hint">至少选一个主色</span>
      </div>
      <div class="nb-row">
        <span class="nb-label">渐隐色</span>
        <button
          v-for="key in MC_COLOR_KEYS"
          :key="key"
          class="nb-color"
          :class="{ active: newFades.includes(rgbToInt(MC_COLORS[key])) }"
          :style="{ backgroundColor: MC_COLORS[key] }"
          :title="`§${key} ${MC_COLOR_NAMES[key]}`"
          @click="newFades = toggleColor(newFades, MC_COLORS[key])"
        />
        <span v-if="newFades.length === 0" class="nb-hint">可选</span>
      </div>
      <div class="nb-row">
        <CheckToggle v-model="newTrail" :size="14">拖尾（钻石轨迹）</CheckToggle>
        <CheckToggle v-model="newTwinkle" :size="14">闪烁（萤石粉效果）</CheckToggle>
        <button class="tool-btn nb-add-btn" :disabled="newColors.length === 0" @click="addExplosion">
          <Plus :size="14" />
          添加爆炸
        </button>
      </div>
      <div v-if="explosions.length > 0" class="nb-tags">
        <span v-for="(e, i) in explosions" :key="i" class="nb-tag">
          {{ shapeName(e.shape) }} ·
          <span
            v-for="key in colorKeys(e.colors)"
            :key="key"
            class="nb-mini-color"
            :style="{ backgroundColor: MC_COLORS[key] }"
          />
          {{ e.trail ? "拖尾" : "" }}{{ e.twinkle ? "闪烁" : "" }}
          <button class="nb-tag-x" @click="removeExplosion(i)">
            <Trash2 :size="12" />
          </button>
        </span>
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

.nb-hint {
  font-size: 11px;
  color: var(--text-tertiary);
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

.nb-mini-color {
  width: 10px;
  height: 10px;
  border: 1px solid var(--border-strong);
  border-radius: 2px;
  display: inline-block;
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
