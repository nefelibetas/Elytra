<script setup lang="ts">
// 药水构造块（嵌入「物品 NBT 生成」：物品为四种药水类时出现）。
// 维护基础药水/效果列表/液体颜色并输出附加内文（modern potion_contents / legacy 标签）；
// 物品名与颜色沿用主表单通用区。
import { computed, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import { Plus, Trash2 } from "@lucide/vue";
import DropSelect, { type SelOption } from "../common/DropSelect.vue";
import CheckToggle from "../common/CheckToggle.vue";
import {
  POTION_EFFECTS, POTION_BASES, potionRawInner, rgbToInt, localizedName,
  filterEffectsByVersion, filterPotionBasesByVersion,
  type PotionEffectInput,
} from "../../utils/nbt";
import type { GiveVersion } from "../../utils/give";

const { t } = useI18n();

const props = defineProps<{
  /** 由外部物品 id 决定：potion / splash_potion / lingering_potion / tipped_arrow。 */
  itemKind: string;
  version: GiveVersion;
}>();

const emit = defineEmits<{ (e: "update:inner", v: string): void }>();

const baseOptions = computed<SelOption[]>(() => [
  { value: "", label: t("tools.potion.noBase") },
  ...filterPotionBasesByVersion(POTION_BASES, props.version).map((b) => ({
    value: b.id,
    label: localizedName(b),
  })),
]);

const potionId = ref("");
const effects = ref<PotionEffectInput[]>([]);
const newEffect = ref("");
const newAmp = ref(1);
const newDurSec = ref(60);

const effectOptions = computed<SelOption[]>(() =>
  filterEffectsByVersion(POTION_EFFECTS, props.version).map((e) => ({
    value: e.id,
    label: localizedName(e),
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
  const effect = POTION_EFFECTS.find((e) => e.id === id);
  return effect ? localizedName(effect) : id;
}

// 自定义液体颜色（RGB packed int；null = 默认）
const customColor = ref<number | null>(null);
const customColorHex = ref("#ff5500");

function toggleCustomColor() {
  customColor.value = customColor.value === null ? rgbToInt(customColorHex.value) : null;
}

function onCustomColor(e: Event) {
  customColorHex.value = (e.target as HTMLInputElement).value;
  customColor.value = rgbToInt(customColorHex.value);
}

/** 输出内文（随物品/版本/药水配置变化）。 */
watch(
  [() => props.itemKind, () => props.version, potionId, effects, customColor],
  () => {
    emit(
      "update:inner",
      potionRawInner(
        {
          itemKind: props.itemKind,
          potionId: potionId.value,
          effects: effects.value,
          customColor: customColor.value,
          customName: "",
          colorCode: "",
          count: 1,
        },
        props.version,
      ),
    );
  },
  { deep: true, immediate: true },
);

/** 版本切换：清掉该区间不存在的基础药水与效果。 */
watch(
  () => props.version,
  () => {
    const bases = new Set(baseOptions.value.map((b) => b.value));
    if (potionId.value && !bases.has(potionId.value)) potionId.value = "";
    const ids = new Set(effectOptions.value.map((o) => o.value));
    effects.value = effects.value.filter((e) => ids.has(e.id));
    if (newEffect.value && !ids.has(newEffect.value)) newEffect.value = "";
  },
);
</script>

<template>
  <div class="nb-sec">
    <div class="nb-sec-title">{{ t("tools.potion.title") }}</div>
    <div class="nb-row">
      <span class="nb-label">{{ t("tools.potion.base") }}</span>
      <DropSelect
        :model-value="potionId"
        :options="baseOptions"
        width="150px"
        :placeholder="t('tools.potion.noBase')"
        @update:model-value="(v: string) => (potionId = v)"
      />
    </div>
    <div class="nb-row">
      <DropSelect
        :model-value="newEffect"
        :options="effectOptions"
        width="150px"
        :placeholder="t('tools.potion.chooseEffect')"
        @update:model-value="(v: string) => (newEffect = v)"
      />
      <span class="nb-label nb-label-short">{{ t("tools.potion.level") }}</span>
      <input v-model.number="newAmp" type="number" class="nb-num" min="0" max="255" autocomplete="off" />
      <span class="nb-label nb-label-short">{{ t("tools.potion.duration") }}</span>
      <input v-model.number="newDurSec" type="number" class="nb-num" min="1" autocomplete="off" />
      <button class="tool-btn nb-add-btn" :disabled="!newEffect" @click="addEffect">
        <Plus :size="14" />
        {{ t("common.action.add") }}
      </button>
    </div>
    <div v-if="effects.length > 0" class="nb-tags">
      <span v-for="(e, i) in effects" :key="i" class="nb-tag">
        {{ effectName(e.id) }} {{ e.amplifier }} · {{ t("tools.potion.seconds", { count: Math.round(e.duration / 20) }) }}
        <button class="nb-tag-x" @click="removeEffect(i)">
          <Trash2 :size="12" />
        </button>
      </span>
    </div>
    <div class="nb-row">
      <span class="nb-label">{{ t("tools.potion.liquidColor") }}</span>
      <CheckToggle :model-value="customColor !== null" :size="14" @change="toggleCustomColor">{{ t("tools.potion.custom") }}</CheckToggle>
      <input
        v-if="customColor !== null"
        type="color"
        class="nb-color-pick"
        :value="customColorHex"
        @input="onCustomColor"
        autocomplete="off"
      />
      <span v-if="customColor === null" class="nb-hint">{{ t("tools.potion.defaultLiquidColor") }}</span>
    </div>
  </div>
</template>

<style>
@import "../../styles/tools-shared.css";
</style>
