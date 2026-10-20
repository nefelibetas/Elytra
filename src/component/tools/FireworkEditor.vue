<script setup lang="ts">
// 烟花构造块（嵌入「物品 NBT 生成」：物品为 firework_rocket/firework_star 时出现）。
// 维护爆炸列表并输出该物品的附加内文（modern 组件对 / legacy 标签），由父组件并入输出。
import { computed, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import { Plus, Trash2 } from "@lucide/vue";
import DropSelect, { type SelOption } from "../common/DropSelect.vue";
import CheckToggle from "../common/CheckToggle.vue";
import { FIREWORK_SHAPES, fireworkRawInner, rgbToInt, localizedName, type FireworkExplosion } from "../../utils/nbt";
import { MC_COLORS, MC_COLOR_KEYS, colorName } from "../../utils/motd";
import type { GiveVersion } from "../../utils/give";

const { t } = useI18n();

const props = defineProps<{
  /** 由外部物品 id 决定：firework_rocket / firework_star。 */
  itemKind: string;
  version: GiveVersion;
}>();

const emit = defineEmits<{ (e: "update:inner", v: string): void }>();

const flight = ref(2);
const explosions = ref<FireworkExplosion[]>([]);
const newShape = ref("small_ball");
const newColors = ref<number[]>([]);
const newFades = ref<number[]>([]);
const newTrail = ref(false);
const newTwinkle = ref(false);

const shapeOptions = computed<SelOption[]>(() =>
  FIREWORK_SHAPES.map((s) => ({ value: s.id, label: localizedName(s) })),
);

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
  const shape = FIREWORK_SHAPES.find((s) => s.id === id);
  return shape ? localizedName(shape) : id;
}

/** 颜色 int → 色板 key（显示用）。 */
function colorKeys(colors: number[]): string[] {
  return MC_COLOR_KEYS.filter((k) => colors.includes(rgbToInt(MC_COLORS[k])));
}

/** 输出内文（随物品类型/版本/爆炸列表变化）。 */
watch(
  [() => props.itemKind, () => props.version, flight, explosions],
  () => {
    emit(
      "update:inner",
      fireworkRawInner(
        {
          itemKind: props.itemKind,
          flight: flight.value,
          explosions: explosions.value,
          count: 1,
        },
        props.version,
      ),
    );
  },
  { deep: true, immediate: true },
);
</script>

<template>
  <div class="nb-sec">
    <div class="nb-sec-title">{{ t("tools.firework.title") }}</div>
    <div v-if="itemKind === 'firework_rocket'" class="nb-row">
      <span class="nb-label">{{ t("tools.firework.flight") }}</span>
      <input v-model.number="flight" type="number" class="nb-num" min="1" max="3" autocomplete="off" />
      <span class="nb-hint">1–3</span>
    </div>
    <div class="nb-row">
      <span class="nb-label">{{ t("tools.firework.shape") }}</span>
      <DropSelect
        :model-value="newShape"
        :options="shapeOptions"
        width="130px"
        @update:model-value="(v: string) => (newShape = v)"
      />
    </div>
    <div class="nb-row">
      <span class="nb-label">{{ t("tools.firework.primaryColors") }}</span>
      <button
        v-for="key in MC_COLOR_KEYS"
        :key="key"
        class="nb-color"
        :class="{ active: newColors.includes(rgbToInt(MC_COLORS[key])) }"
        :style="{ backgroundColor: MC_COLORS[key] }"
        :title="`§${key} ${colorName(key)}`"
        @click="newColors = toggleColor(newColors, MC_COLORS[key])"
      />
      <span v-if="newColors.length === 0" class="nb-hint">{{ t("tools.firework.atLeastOne") }}</span>
    </div>
    <div class="nb-row">
      <span class="nb-label">{{ t("tools.firework.fadeColors") }}</span>
      <button
        v-for="key in MC_COLOR_KEYS"
        :key="key"
        class="nb-color"
        :class="{ active: newFades.includes(rgbToInt(MC_COLORS[key])) }"
        :style="{ backgroundColor: MC_COLORS[key] }"
        :title="`§${key} ${colorName(key)}`"
        @click="newFades = toggleColor(newFades, MC_COLORS[key])"
      />
      <span v-if="newFades.length === 0" class="nb-hint">{{ t("tools.firework.optional") }}</span>
    </div>
    <div class="nb-row">
      <CheckToggle v-model="newTrail" :size="14">{{ t("tools.firework.trail") }}</CheckToggle>
      <CheckToggle v-model="newTwinkle" :size="14">{{ t("tools.firework.twinkle") }}</CheckToggle>
      <button class="tool-btn nb-add-btn" :disabled="newColors.length === 0" @click="addExplosion">
        <Plus :size="14" />
        {{ t("tools.firework.addExplosion") }}
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
        {{ e.trail ? t("tools.firework.trail") : "" }}{{ e.twinkle ? t("tools.firework.twinkle") : "" }}
        <button class="nb-tag-x" @click="removeExplosion(i)">
          <Trash2 :size="12" />
        </button>
      </span>
    </div>
  </div>
</template>

<style>
@import "../../styles/tools-shared.css";
</style>
