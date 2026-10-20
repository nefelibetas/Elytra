// 原版指令生成器（精选指令）：schema 数据 + 纯构建函数，UI 由 CommandGeneratorTool 按字段渲染。
// 仅收录 Java 版原版指令；语法基线 1.13+（命令改名后的形态），NBT/组件附加段原样透传、由调用方校验。

import { POTION_EFFECTS } from "./nbt";
import { hasTopLevelEquals, normalizeNbtFragment } from "./nbtText";
import { ENCHANTMENTS } from "./give";
import { GAMERULES } from "./mcData";
import { currentLocale } from "../i18n";

// ── 字段模型 ──

/** 下拉选项（结构兼容 DropSelect 的 SelOption）。 */
export interface FieldOption {
  value: string;
  label: string;
  /** 英文界面标签（缺失时回退 label）。 */
  labelEn?: string;
}

/** 按当前界面语言取选项标签。 */
export function optionLabel(option: FieldOption): string {
  return currentLocale.value === "en-US" ? option.labelEn ?? option.label : option.label;
}

export type ParamField =
  | { kind: "item"; key: string; labelKey: string; placeholderKey?: string }
  | { kind: "target"; key: string; labelKey: string; default: string }
  | {
      kind: "search";
      key: string;
      labelKey: string;
      options: FieldOption[];
      default: string;
      /** 允许自由输入（实体/效果等：列表未收录的 id 可直接写）。 */
      allowCustom?: boolean;
    }
  | { kind: "text"; key: string; labelKey: string; placeholderKey?: string; default: string }
  | { kind: "textarea"; key: string; labelKey: string; placeholderKey?: string; rows?: number }
  | { kind: "num"; key: string; labelKey: string; min: number; max: number; default: number }
  | { kind: "check"; key: string; labelKey: string; default: boolean };

export type CommandValues = Record<string, string | number | boolean>;

export interface CommandDef {
  id: string;
  /** 显示名（含斜杠）。 */
  name: string;
  descKey: string;
  fields: ParamField[];
  build: (v: CommandValues) => string;
}

// 取值助手（命令构建内部使用）
function s(v: CommandValues, key: string): string {
  return String(v[key] ?? "").trim();
}
function n(v: CommandValues, key: string, fallback: number): number {
  const x = Number(v[key]);
  return Number.isFinite(x) ? x : fallback;
}
function clampNum(x: number, min: number, max: number): number {
  return Math.max(min, Math.min(Math.round(x), max));
}
/** 命名空间化（无前缀时补 minecraft:）。 */
function ns(id: string): string {
  return id.includes(":") ? id : `minecraft:${id}`;
}

// ── 数据表 ──

/** 常见可召唤实体（按用途排序）。 */
export const SUMMON_ENTITIES: FieldOption[] = [
  { value: "zombie", label: "僵尸", labelEn: "Zombie" },
  { value: "skeleton", label: "骷髅", labelEn: "Skeleton" },
  { value: "creeper", label: "苦力怕", labelEn: "Creeper" },
  { value: "spider", label: "蜘蛛", labelEn: "Spider" },
  { value: "cave_spider", label: "洞穴蜘蛛", labelEn: "Cave Spider" },
  { value: "enderman", label: "末影人", labelEn: "Enderman" },
  { value: "witch", label: "女巫", labelEn: "Witch" },
  { value: "slime", label: "史莱姆", labelEn: "Slime" },
  { value: "magma_cube", label: "岩浆怪", labelEn: "Magma Cube" },
  { value: "blaze", label: "烈焰人", labelEn: "Blaze" },
  { value: "ghast", label: "恶魂", labelEn: "Ghast" },
  { value: "wither_skeleton", label: "凋灵骷髅", labelEn: "Wither Skeleton" },
  { value: "guardian", label: "守卫者", labelEn: "Guardian" },
  { value: "elder_guardian", label: "远古守卫者", labelEn: "Elder Guardian" },
  { value: "drowned", label: "溺尸", labelEn: "Drowned" },
  { value: "phantom", label: "幻翼", labelEn: "Phantom" },
  { value: "shulker", label: "潜影贝", labelEn: "Shulker" },
  { value: "vex", label: "恼鬼", labelEn: "Vex" },
  { value: "vindicator", label: "卫道士", labelEn: "Vindicator" },
  { value: "evoker", label: "唤魔者", labelEn: "Evoker" },
  { value: "pillager", label: "掠夺者", labelEn: "Pillager" },
  { value: "ravager", label: "劫掠兽", labelEn: "Ravager" },
  { value: "piglin", label: "猪灵", labelEn: "Piglin" },
  { value: "piglin_brute", label: "猪灵蛮兵", labelEn: "Piglin Brute" },
  { value: "hoglin", label: "疣猪兽", labelEn: "Hoglin" },
  { value: "zoglin", label: "僵尸疣猪兽", labelEn: "Zoglin" },
  { value: "warden", label: "监守者", labelEn: "Warden" },
  { value: "breeze", label: "旋风人", labelEn: "Breeze" },
  { value: "allay", label: "悦灵", labelEn: "Allay" },
  { value: "villager", label: "村民", labelEn: "Villager" },
  { value: "wandering_trader", label: "流浪商人", labelEn: "Wandering Trader" },
  { value: "iron_golem", label: "铁傀儡", labelEn: "Iron Golem" },
  { value: "snow_golem", label: "雪傀儡", labelEn: "Snow Golem" },
  { value: "armor_stand", label: "盔甲架", labelEn: "Armor Stand" },
  { value: "cow", label: "牛", labelEn: "Cow" },
  { value: "pig", label: "猪", labelEn: "Pig" },
  { value: "sheep", label: "绵羊", labelEn: "Sheep" },
  { value: "chicken", label: "鸡", labelEn: "Chicken" },
  { value: "rabbit", label: "兔子", labelEn: "Rabbit" },
  { value: "horse", label: "马", labelEn: "Horse" },
  { value: "donkey", label: "驴", labelEn: "Donkey" },
  { value: "llama", label: "羊驼", labelEn: "Llama" },
  { value: "cat", label: "猫", labelEn: "Cat" },
  { value: "wolf", label: "狼", labelEn: "Wolf" },
  { value: "fox", label: "狐狸", labelEn: "Fox" },
  { value: "bee", label: "蜜蜂", labelEn: "Bee" },
  { value: "panda", label: "熊猫", labelEn: "Panda" },
  { value: "polar_bear", label: "北极熊", labelEn: "Polar Bear" },
  { value: "turtle", label: "海龟", labelEn: "Turtle" },
  { value: "dolphin", label: "海豚", labelEn: "Dolphin" },
  { value: "axolotl", label: "美西螈", labelEn: "Axolotl" },
  { value: "frog", label: "青蛙", labelEn: "Frog" },
  { value: "goat", label: "山羊", labelEn: "Goat" },
  { value: "camel", label: "骆驼", labelEn: "Camel" },
  { value: "sniffer", label: "嗅探兽", labelEn: "Sniffer" },
  { value: "armadillo", label: "犰狳", labelEn: "Armadillo" },
  { value: "item", label: "掉落物（物品展示用）", labelEn: "Item (display)" },
];

const EFFECT_OPTIONS: FieldOption[] = POTION_EFFECTS.map((e) => ({
  value: e.id,
  label: e.name,
  labelEn: e.nameEn ?? e.id,
}));

const ENCHANT_OPTIONS: FieldOption[] = ENCHANTMENTS.map((e) => ({
  value: e.id,
  label: e.name,
  labelEn: e.nameEn ?? e.id,
}));

const GAMERULE_OPTIONS: FieldOption[] = GAMERULES.map((g) => ({
  value: g.name,
  label: `${g.label}（${g.name}）`,
  labelEn: `${g.labelEn} (${g.name})`,
}));

const giveDef: CommandDef = {
  id: "give",
  name: "/give",
  descKey: "tools.command.defs.give.desc",
  fields: [
    {
      kind: "item",
      key: "item",
      labelKey: "tools.command.defs.give.fields.item",
      placeholderKey: "tools.command.defs.give.placeholders.item",
    },
    { kind: "num", key: "count", labelKey: "tools.command.defs.give.fields.count", min: 1, max: 99, default: 1 },
    {
      kind: "textarea",
      key: "nbt",
      labelKey: "tools.command.defs.give.fields.nbt",
      placeholderKey: "tools.command.defs.give.placeholders.nbt",
      rows: 2,
    },
  ],
  build: (v) => {
    const item = s(v, "item");
    if (!item) return "";
    const count = clampNum(n(v, "count", 1), 1, 99);
    const extra = normalizeNbtFragment(s(v, "nbt"));
    const head = `/give @p ${ns(item)}`;
    if (!extra) return `${head} ${count}`;
    // 组件形态（以引号开头的组件内文，或顶层 `k=v`）用 [ ]，否则按 NBT 标签用 { }
    const asComponents = extra.startsWith('"') || hasTopLevelEquals(extra);
    const wrapped = asComponents ? `[${extra}]` : `{${extra}}`;
    return `${head}${wrapped} ${count}`;
  },
};

const summonDef: CommandDef = {
  id: "summon",
  name: "/summon",
  descKey: "tools.command.defs.summon.desc",
  fields: [
    {
      kind: "search",
      key: "entity",
      labelKey: "tools.command.defs.summon.fields.entity",
      options: SUMMON_ENTITIES,
      default: "zombie",
      allowCustom: true,
    },
    { kind: "text", key: "pos", labelKey: "tools.command.defs.summon.fields.pos", default: "~ ~ ~" },
    {
      kind: "textarea",
      key: "nbt",
      labelKey: "tools.command.defs.summon.fields.nbt",
      placeholderKey: "tools.command.defs.summon.placeholders.nbt",
      rows: 2,
    },
  ],
  build: (v) => {
    const entity = s(v, "entity");
    if (!entity) return "";
    const pos = s(v, "pos") || "~ ~ ~";
    const nbt = normalizeNbtFragment(s(v, "nbt"));
    return nbt ? `/summon ${ns(entity)} ${pos} {${nbt}}` : `/summon ${ns(entity)} ${pos}`;
  },
};

const effectDef: CommandDef = {
  id: "effect",
  name: "/effect",
  descKey: "tools.command.defs.effect.desc",
  fields: [
    { kind: "target", key: "target", labelKey: "tools.command.defs.effect.fields.target", default: "@p" },
    {
      kind: "search",
      key: "effect",
      labelKey: "tools.command.defs.effect.fields.effect",
      options: EFFECT_OPTIONS,
      default: "speed",
      allowCustom: true,
    },
    { kind: "num", key: "duration", labelKey: "tools.command.defs.effect.fields.duration", min: 1, max: 1000000, default: 30 },
    { kind: "num", key: "amplifier", labelKey: "tools.command.defs.effect.fields.amplifier", min: 0, max: 255, default: 0 },
    { kind: "check", key: "hide", labelKey: "tools.command.defs.effect.fields.hide", default: false },
  ],
  build: (v) => {
    const target = s(v, "target") || "@p";
    const effect = s(v, "effect");
    if (!effect) return "";
    const duration = clampNum(n(v, "duration", 30), 1, 1000000);
    const amp = clampNum(n(v, "amplifier", 0), 0, 255);
    const tail = Boolean(v["hide"]) ? " true" : "";
    return `/effect give ${target} ${ns(effect)} ${duration} ${amp}${tail}`;
  },
};

const enchantDef: CommandDef = {
  id: "enchant",
  name: "/enchant",
  descKey: "tools.command.defs.enchant.desc",
  fields: [
    { kind: "target", key: "target", labelKey: "tools.command.defs.enchant.fields.target", default: "@p" },
    {
      kind: "search",
      key: "enchantment",
      labelKey: "tools.command.defs.enchant.fields.enchantment",
      options: ENCHANT_OPTIONS,
      default: "sharpness",
      allowCustom: true,
    },
    { kind: "num", key: "level", labelKey: "tools.command.defs.enchant.fields.level", min: 1, max: 255, default: 1 },
  ],
  build: (v) => {
    const target = s(v, "target") || "@p";
    const ench = s(v, "enchantment");
    if (!ench) return "";
    const level = clampNum(n(v, "level", 1), 1, 255);
    return `/enchant ${target} ${ns(ench)} ${level}`;
  },
};

const gameruleDef: CommandDef = {
  id: "gamerule",
  name: "/gamerule",
  descKey: "tools.command.defs.gamerule.desc",
  fields: [
    {
      kind: "search",
      key: "rule",
      labelKey: "tools.command.defs.gamerule.fields.rule",
      options: GAMERULE_OPTIONS,
      default: "keepInventory",
      allowCustom: true,
    },
    {
      kind: "text",
      key: "value",
      labelKey: "tools.command.defs.gamerule.fields.value",
      placeholderKey: "tools.command.defs.gamerule.placeholders.value",
      default: "",
    },
  ],
  build: (v) => {
    const rule = s(v, "rule");
    if (!rule) return "";
    const value = s(v, "value");
    return value ? `/gamerule ${rule} ${value}` : `/gamerule ${rule}`;
  },
};

/** 指令注册表（顺序即 UI 顺序）。 */
export const COMMANDS: CommandDef[] = [
  giveDef,
  summonDef,
  effectDef,
  enchantDef,
  gameruleDef,
];

/** 取指令的默认值表。 */
export function defaultValues(def: CommandDef): CommandValues {
  const out: CommandValues = {};
  for (const f of def.fields) {
    if (f.kind === "search" || f.kind === "target") out[f.key] = f.default;
    else if (f.kind === "text") out[f.key] = f.default;
    else if (f.kind === "num") out[f.key] = f.default;
    else if (f.kind === "check") out[f.key] = f.default;
    else if (f.kind === "item") out[f.key] = "";
    else out[f.key] = "";
  }
  return out;
}
