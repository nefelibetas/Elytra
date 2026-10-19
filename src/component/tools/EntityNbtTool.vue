<script setup lang="ts">
// 生物 NBT 生成：选实体 → 通用 NBT 快捷项（无 AI/无敌/静音/名字/血量…）+ 村民特化（职业/交易）
// + 附加 NBT → 输出实体 NBT 片段（供 /summon 等场景粘贴）与完整 /summon 命令。
import { computed, ref, watch } from "vue";
import { Copy } from "@lucide/vue";
import CheckToggle from "../common/CheckToggle.vue";
import DropSelect, { type SelOption } from "../common/DropSelect.vue";
import SearchSelect from "./SearchSelect.vue";
import VillagerTradeEditor from "./VillagerTradeEditor.vue";
import { buildVillagerNbt, VILLAGER_PROFESSIONS, VILLAGER_TYPES, type VillagerTrade } from "../../utils/nbt";
import { normalizeNbtFragment, checkNbtFragment } from "../../utils/nbtText";
import { GIVE_VERSION_OPTIONS, DEFAULT_GIVE_VERSION, maxVersionOf, type GiveVersion } from "../../utils/give";
import { SUMMON_ENTITIES } from "../../utils/commands";
import { copyText } from "../../utils/clipboard";

const version = ref<GiveVersion>(DEFAULT_GIVE_VERSION);
const maxVersion = computed(() => maxVersionOf(version.value));
/** NoAI/Silent/Invulnerable/PersistenceRequired 自 1.8 起。 */
const entityFlagsSupported = computed(() => maxVersion.value >= 8);
/** Glowing 自 1.9 起。 */
const glowingSupported = computed(() => maxVersion.value >= 9);
/** NoGravity 在 1.10 起对所有实体生效。 */
const noGravitySupported = computed(() => maxVersion.value >= 10);
const versionOptions: SelOption[] = GIVE_VERSION_OPTIONS.map((o) => ({
  value: o.value,
  label: o.label,
}));

/** 实体（可搜索可选列表，也允许直接输入 id）。 */
const entityOptions = SUMMON_ENTITIES.map((o) => ({ id: o.value, name: o.label }));
const entityId = ref("zombie");

// 通用 NBT 快捷项
const noAI = ref(false);
const invulnerable = ref(false);
const silent = ref(false);
const persist = ref(false);
const glowing = ref(false);
const noGravity = ref(false);
const customName = ref("");
const health = ref(0);

// 村民特化（实体 == villager 时显示）
const era = ref<"1.14+" | "pre1.14">("1.14+");
const eraOptions = computed<SelOption[]>(() => {
  const pre = { value: "pre1.14", label: "1.13 及以下（纯 Offers 交易）" };
  return maxVersionOf(version.value) < 14
    ? [pre]
    : [{ value: "1.14+", label: "1.14+（职业/等级体系）" }, pre];
});
watch(version, (v) => {
  if (maxVersionOf(v) < 14) era.value = "pre1.14";
});
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
const trades = ref<VillagerTrade[]>([]);

const extraNbt = ref("");
const extraError = computed(() => (extraNbt.value.trim() ? checkNbtFragment(extraNbt.value) : null));

/** SNBT 字符串转义（双引号/反斜杠）。 */
function jsonStr(s: string): string {
  return s.replace(/\\/g, "\\\\").replace(/"/g, '\\"');
}

/** 实体 NBT 内文（各段拼接；为空时输出无 NBT）。 */
const nbtBody = computed(() => {
  const parts: string[] = [];
  if (noAI.value && entityFlagsSupported.value) parts.push("NoAI:1b");
  if (invulnerable.value && entityFlagsSupported.value) parts.push("Invulnerable:1b");
  if (silent.value && entityFlagsSupported.value) parts.push("Silent:1b");
  if (persist.value && entityFlagsSupported.value) parts.push("PersistenceRequired:1b");
  if (glowing.value && glowingSupported.value) parts.push("Glowing:1b");
  if (noGravity.value && noGravitySupported.value) parts.push("NoGravity:1b");
  const name = customName.value.trim();
  if (name) parts.push(`CustomName:'"${jsonStr(name)}"'`);
  if (health.value > 0) parts.push(`Health:${health.value}f`);
  if (entityId.value === "villager") {
    const villager = buildVillagerNbt(
      { era: era.value, profession: profession.value, type: type.value, level: level.value, trades: trades.value },
      version.value,
    );
    if (villager) parts.push(villager);
  }
  const extra = normalizeNbtFragment(extraNbt.value);
  if (extra) parts.push(extra);
  return parts.join(",");
});

const nbtFragment = computed(() => (nbtBody.value.length > 0 ? `{${nbtBody.value}}` : ""));

/** 完整 /summon 指令（无 NBT 时省略 NBT 段）。 */
const summonCommand = computed(() => {
  const id = entityId.value.trim();
  if (!id) return "";
  return nbtFragment.value ? `/summon ${id} ~ ~ ~ ${nbtFragment.value}` : `/summon ${id} ~ ~ ~`;
});

async function copy(text: string, label: string) {
  if (!text) return;
  await copyText(text, label);
}
</script>

<template>
  <div class="tool-card">
    <div class="tool-card-hdr">
      <h3>生物 NBT 生成</h3>
      <div class="tool-card-sub">构造实体 NBT 片段（含村民职业/交易），供指令生成器等场景引用</div>
    </div>

    <div class="nb-row">
      <span class="nb-label">版本区间</span>
      <DropSelect
        :model-value="version"
        :options="versionOptions"
        width="230px"
        @update:model-value="(v: string) => (version = v as GiveVersion)"
      />
    </div>

    <div class="nb-row">
      <span class="nb-label">实体</span>
      <SearchSelect
        v-model="entityId"
        :options="entityOptions"
        :allow-custom="true"
        placeholder="搜索实体或直接输入 id…"
        max-width="360px"
      />
    </div>

    <div class="nb-sec">
      <div class="nb-sec-title">通用 NBT</div>
      <div class="nb-checks">
        <CheckToggle v-if="entityFlagsSupported" v-model="noAI" :size="14">无 AI</CheckToggle>
        <CheckToggle v-if="entityFlagsSupported" v-model="invulnerable" :size="14">无敌</CheckToggle>
        <CheckToggle v-if="entityFlagsSupported" v-model="silent" :size="14">静音</CheckToggle>
        <CheckToggle v-if="entityFlagsSupported" v-model="persist" :size="14">不消失</CheckToggle>
        <CheckToggle v-if="glowingSupported" v-model="glowing" :size="14">发光</CheckToggle>
        <CheckToggle v-if="noGravitySupported" v-model="noGravity" :size="14">无重力</CheckToggle>
        <span v-if="!entityFlagsSupported" class="nb-hint">1.7.10 仅支持自定义名/血量</span>
      </div>
      <div class="nb-row">
        <span class="nb-label">自定义名</span>
        <input v-model="customName" class="nb-input" placeholder="留空使用默认名" spellcheck="false" autocomplete="off" />
      </div>
      <div class="nb-row">
        <span class="nb-label">血量</span>
        <input v-model.number="health" type="number" class="nb-num" min="0" max="2048" autocomplete="off" />
        <span class="nb-hint">0 = 不设置</span>
      </div>
    </div>

    <template v-if="entityId.trim() === 'villager'">
      <div class="nb-sec">
        <div class="nb-sec-title">村民</div>
        <div class="nb-row">
          <span class="nb-label">村民世代</span>
          <DropSelect
            :model-value="era"
            :options="eraOptions"
            width="220px"
            @update:model-value="(v: string) => (era = v as '1.14+' | 'pre1.14')"
          />
        </div>
        <div v-if="era === '1.14+'" class="nb-row">
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
      <!-- 交易区显示条件：低世代（纯 Offers）恒显示；1.14+ 仅职业村民可交易——
           失业村民不交易（wiki），且其预置交易会在认领工作站点时被游戏清除（MC-150464）；
           傻子不就业不交易。两者隐藏交易区并给出原因。 -->
      <VillagerTradeEditor
        v-if="era === 'pre1.14' || (profession !== 'none' && profession !== 'nitwit')"
        :trades="trades"
        :version="version"
        :profession="profession"
        :level="level"
      />
      <p v-else class="nb-note">
        {{ profession === "nitwit" ? "傻子不就业、不交易，无法配置交易" : "失业村民不交易；预置交易会在其获得职业时被游戏清除" }}
      </p>
    </template>

    <div class="nb-sec">
      <div class="nb-row nb-row-top">
        <span class="nb-label">附加 NBT</span>
        <textarea
          v-model="extraNbt"
          class="nb-textarea"
          rows="2"
          spellcheck="false"
          autocomplete="off"
          placeholder="自由 NBT（如 {Tags:[&quot;boss&quot;]}，外层括号可省略）"
        ></textarea>
      </div>
      <p v-if="extraError" class="nb-error">{{ extraError }}</p>
    </div>

    <div class="nb-output-head">
      <span>实体 NBT 片段</span>
      <button class="tool-btn nb-copy-btn" @click="copy(nbtFragment, 'NBT 片段已复制')">
        <Copy :size="14" />
        复制
      </button>
    </div>
    <code class="gen-output gen-output-cmd">{{ nbtFragment || "（配置后生成）" }}</code>

    <div class="nb-output-head nb-output-gap">
      <span>完整指令</span>
      <button class="tool-btn nb-copy-btn" @click="copy(summonCommand, '指令已复制')">
        <Copy :size="14" />
        复制
      </button>
    </div>
    <code class="gen-output gen-output-cmd">{{ summonCommand }}</code>


  </div>
</template>

<style>
@import "../../styles/tools-shared.css";
</style>

<style scoped>
.nb-checks {
  display: flex;
  flex-wrap: wrap;
  gap: 4px 16px;
  margin-bottom: 8px;
}

.nb-hint {
  font-size: 11px;
  color: var(--text-tertiary);
}

.nb-copy-btn {
  font-size: 12px;
  padding: 3px 10px;
}

.nb-output-gap {
  margin-top: 10px;
}

</style>
