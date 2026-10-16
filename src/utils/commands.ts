// 原版指令生成器（一期：热门指令）：schema 数据 + 纯构建函数，UI 由 CommandGeneratorTool 按字段渲染。
// 仅收录 Java 版原版指令；语法基线 1.13+（命令改名后的形态），NBT/组件附加段原样透传、由调用方校验。

import { POTION_EFFECTS } from "./nbt";
import { hasTopLevelEquals, normalizeNbtFragment } from "./nbtText";
import { ENCHANTMENTS } from "./give";
import { PARTICLES, PARTICLE_MODES, SOUND_EVENTS, SOUND_SOURCES } from "./mcData";

// ── 字段模型 ──

/** 下拉选项（结构兼容 DropSelect 的 SelOption）。 */
export interface FieldOption {
  value: string;
  label: string;
}

export type ParamField =
  | { kind: "item"; key: string; label: string; placeholder?: string }
  | { kind: "target"; key: string; label: string; default: string }
  | { kind: "enum"; key: string; label: string; options: FieldOption[]; default: string }
  | {
      kind: "search";
      key: string;
      label: string;
      options: FieldOption[];
      default: string;
      /** 允许自由输入（实体/效果等：列表未收录的 id 可直接写）。 */
      allowCustom?: boolean;
    }
  | { kind: "text"; key: string; label: string; placeholder?: string; default: string }
  | { kind: "textarea"; key: string; label: string; placeholder?: string; rows?: number }
  | { kind: "num"; key: string; label: string; min: number; max: number; default: number }
  | { kind: "check"; key: string; label: string; default: boolean };

export type CommandValues = Record<string, string | number | boolean>;

export interface CommandDef {
  id: string;
  /** 显示名（含斜杠）。 */
  name: string;
  desc: string;
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

/** 常见可召唤实体（一期收录热门，按用途排序）。 */
export const SUMMON_ENTITIES: FieldOption[] = [
  { value: "zombie", label: "僵尸" },
  { value: "skeleton", label: "骷髅" },
  { value: "creeper", label: "苦力怕" },
  { value: "spider", label: "蜘蛛" },
  { value: "cave_spider", label: "洞穴蜘蛛" },
  { value: "enderman", label: "末影人" },
  { value: "witch", label: "女巫" },
  { value: "slime", label: "史莱姆" },
  { value: "magma_cube", label: "岩浆怪" },
  { value: "blaze", label: "烈焰人" },
  { value: "ghast", label: "恶魂" },
  { value: "wither_skeleton", label: "凋灵骷髅" },
  { value: "guardian", label: "守卫者" },
  { value: "elder_guardian", label: "远古守卫者" },
  { value: "drowned", label: "溺尸" },
  { value: "phantom", label: "幻翼" },
  { value: "shulker", label: "潜影贝" },
  { value: "vex", label: "恼鬼" },
  { value: "vindicator", label: "卫道士" },
  { value: "evoker", label: "唤魔者" },
  { value: "pillager", label: "掠夺者" },
  { value: "ravager", label: "劫掠兽" },
  { value: "piglin", label: "猪灵" },
  { value: "piglin_brute", label: "猪灵蛮兵" },
  { value: "hoglin", label: "疣猪兽" },
  { value: "zoglin", label: "僵尸疣猪兽" },
  { value: "warden", label: "监守者" },
  { value: "breeze", label: "旋风人" },
  { value: "allay", label: "悦灵" },
  { value: "villager", label: "村民" },
  { value: "wandering_trader", label: "流浪商人" },
  { value: "iron_golem", label: "铁傀儡" },
  { value: "snow_golem", label: "雪傀儡" },
  { value: "armor_stand", label: "盔甲架" },
  { value: "cow", label: "牛" },
  { value: "pig", label: "猪" },
  { value: "sheep", label: "绵羊" },
  { value: "chicken", label: "鸡" },
  { value: "rabbit", label: "兔子" },
  { value: "horse", label: "马" },
  { value: "donkey", label: "驴" },
  { value: "llama", label: "羊驼" },
  { value: "cat", label: "猫" },
  { value: "wolf", label: "狼" },
  { value: "fox", label: "狐狸" },
  { value: "bee", label: "蜜蜂" },
  { value: "panda", label: "熊猫" },
  { value: "polar_bear", label: "北极熊" },
  { value: "turtle", label: "海龟" },
  { value: "dolphin", label: "海豚" },
  { value: "axolotl", label: "美西螈" },
  { value: "frog", label: "青蛙" },
  { value: "goat", label: "山羊" },
  { value: "camel", label: "骆驼" },
  { value: "sniffer", label: "嗅探兽" },
  { value: "armadillo", label: "犰狳" },
  { value: "item", label: "掉落物（物品展示用）" },
];

/** 经验单位。 */
export const XP_UNITS: FieldOption[] = [
  { value: "points", label: "点数" },
  { value: "levels", label: "等级" },
];

const EFFECT_OPTIONS: FieldOption[] = POTION_EFFECTS.map((e) => ({
  value: e.id,
  label: e.name,
}));

const ENCHANT_OPTIONS: FieldOption[] = ENCHANTMENTS.map((e) => ({
  value: e.id,
  label: e.name,
}));

/** 效果 id → 展示名（输出预览用）。 */
export function effectName(id: string): string {
  return POTION_EFFECTS.find((e) => e.id === id)?.name ?? id;
}

// ── 指令定义（一期六条） ──

const giveDef: CommandDef = {
  id: "give",
  name: "/give",
  desc: "给予玩家物品（附加段可贴「物品 NBT 生成」的修饰内容）",
  fields: [
    { kind: "item", key: "item", label: "物品", placeholder: "搜索物品…" },
    { kind: "num", key: "count", label: "数量", min: 1, max: 99, default: 1 },
    {
      kind: "textarea",
      key: "nbt",
      label: "附加 NBT",
      placeholder: "按组件/NBT 语法书写（如 custom_model_data=7 或 CustomModelData:7）；完整修饰用「物品 NBT 生成」",
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
  desc: "生成实体（位置默认跟随执行者，NBT 可贴村民交易等场景的构造）",
  fields: [
    { kind: "search", key: "entity", label: "实体", options: SUMMON_ENTITIES, default: "zombie", allowCustom: true },
    { kind: "text", key: "pos", label: "位置", placeholder: "~ ~ ~", default: "~ ~ ~" },
    {
      kind: "textarea",
      key: "nbt",
      label: "NBT",
      placeholder: "实体 NBT（如 {NoAI:1b}；村民整套 NBT 可贴村民交易工具的产物）",
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
  desc: "给予状态效果",
  fields: [
    { kind: "target", key: "target", label: "目标", default: "@p" },
    { kind: "search", key: "effect", label: "效果", options: EFFECT_OPTIONS, default: "speed", allowCustom: true },
    { kind: "num", key: "duration", label: "时长（秒）", min: 1, max: 1000000, default: 30 },
    { kind: "num", key: "amplifier", label: "等级", min: 0, max: 255, default: 0 },
    { kind: "check", key: "hide", label: "隐藏粒子", default: false },
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

const xpDef: CommandDef = {
  id: "xp",
  name: "/xp",
  desc: "给予经验",
  fields: [
    { kind: "target", key: "target", label: "目标", default: "@p" },
    { kind: "num", key: "amount", label: "数量", min: 0, max: 2147483647, default: 100 },
    { kind: "enum", key: "unit", label: "单位", options: XP_UNITS, default: "points" },
  ],
  build: (v) => {
    const target = s(v, "target") || "@p";
    const amount = clampNum(n(v, "amount", 100), 0, 2147483647);
    const unit = s(v, "unit") || "points";
    return `/xp add ${target} ${amount} ${unit}`;
  },
};

const enchantDef: CommandDef = {
  id: "enchant",
  name: "/enchant",
  desc: "给手持物品附加魔咒",
  fields: [
    { kind: "target", key: "target", label: "目标", default: "@p" },
    { kind: "search", key: "enchantment", label: "附魔", options: ENCHANT_OPTIONS, default: "sharpness", allowCustom: true },
    { kind: "num", key: "level", label: "等级", min: 1, max: 255, default: 1 },
  ],
  build: (v) => {
    const target = s(v, "target") || "@p";
    const ench = s(v, "enchantment");
    if (!ench) return "";
    const level = clampNum(n(v, "level", 1), 1, 255);
    return `/enchant ${target} ${ns(ench)} ${level}`;
  },
};

const playsoundDef: CommandDef = {
  id: "playsound",
  name: "/playsound",
  desc: "向玩家播放音效",
  fields: [
    { kind: "search", key: "sound", label: "音效", options: SOUND_EVENTS, default: "ui.button.click", allowCustom: true },
    { kind: "enum", key: "source", label: "声道", options: SOUND_SOURCES, default: "master" },
    { kind: "target", key: "targets", label: "听众", default: "@a" },
    { kind: "text", key: "pos", label: "位置", placeholder: "~ ~ ~ 或坐标", default: "~ ~ ~" },
    { kind: "text", key: "volume", label: "音量", placeholder: "1", default: "1" },
    { kind: "text", key: "pitch", label: "音调", placeholder: "1（0.5–2.0）", default: "1" },
  ],
  build: (v) => {
    const sound = s(v, "sound");
    if (!sound) return "";
    const source = s(v, "source") || "master";
    const targets = s(v, "targets") || "@a";
    const pos = s(v, "pos") || "~ ~ ~";
    const volume = s(v, "volume") || "1";
    const pitch = s(v, "pitch") || "1";
    return `/playsound ${ns(sound)} ${source} ${targets} ${pos} ${volume} ${pitch}`;
  },
};

const particleDef: CommandDef = {
  id: "particle",
  name: "/particle",
  desc: "在指定位置生成粒子",
  fields: [
    { kind: "search", key: "name", label: "粒子", options: PARTICLES, default: "flame", allowCustom: true },
    { kind: "text", key: "pos", label: "位置", placeholder: "~ ~ ~ 或坐标", default: "~ ~ ~" },
    { kind: "text", key: "delta", label: "散布", placeholder: "dx dy dz", default: "0 0 0" },
    { kind: "text", key: "speed", label: "速度", placeholder: "0", default: "0" },
    { kind: "num", key: "count", label: "数量", min: 1, max: 100000, default: 10 },
    { kind: "enum", key: "mode", label: "模式", options: PARTICLE_MODES, default: "normal" },
  ],
  build: (v) => {
    const name = s(v, "name");
    if (!name) return "";
    const pos = s(v, "pos") || "~ ~ ~";
    const delta = s(v, "delta") || "0 0 0";
    const speed = s(v, "speed") || "0";
    const count = clampNum(n(v, "count", 10), 1, 100000);
    const mode = s(v, "mode") || "normal";
    return `/particle ${ns(name)} ${pos} ${delta} ${speed} ${count} ${mode}`;
  },
};

/** 指令注册表（顺序即 UI 顺序）。 */
export const COMMANDS: CommandDef[] = [
  giveDef,
  summonDef,
  effectDef,
  xpDef,
  enchantDef,
  playsoundDef,
  particleDef,
];

/** 取指令的默认值表。 */
export function defaultValues(def: CommandDef): CommandValues {
  const out: CommandValues = {};
  for (const f of def.fields) {
    if (f.kind === "enum" || f.kind === "search" || f.kind === "target") out[f.key] = f.default;
    else if (f.kind === "text") out[f.key] = f.default;
    else if (f.kind === "num") out[f.key] = f.default;
    else if (f.kind === "check") out[f.key] = f.default;
    else if (f.kind === "item") out[f.key] = "";
    else out[f.key] = "";
  }
  return out;
}
