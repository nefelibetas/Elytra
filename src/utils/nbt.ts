// NBT 构造器：药水/烟花/村民交易，按版本区间输出（1.7.10–1.20.4 为 NBT；1.20.5 起组件）。

import {
  GIVE_ITEMS, buildTradeItem, EMPTY_MODS, legacyItemRef, maxVersionOf, syntaxOfVersion,
  type GiveVersion, type ItemMods,
} from "./give";
import { currentLocale } from "../i18n";

/** 按当前界面语言取数据表名称；英文缺失时回退注册 id。 */
export function localizedName(entry: { id: string; name: string; nameEn?: string }): string {
  return currentLocale.value === "en-US" ? entry.nameEn ?? entry.id : entry.name;
}

// ── 公共助手 ──

export function rgbToInt(hex: string): number {
  const m = /^#?([0-9a-fA-F]{6})$/.exec(hex.trim());
  if (!m) return 0;
  return parseInt(m[1], 16);
}

export function isKnownItem(id: string): boolean {
  return GIVE_ITEMS.some((i) => i.id === id) || id === "emerald";
}

// ── 药水 ──

/** 状态效果（numId 用于 legacy NBT，命名空间用于 modern 组件）。 */
export interface PotionEffect {
  id: string;
  name: string;
  /** 官方英文名（缺失时回退 id）。 */
  nameEn?: string;
  numId: number;
  /** 引入主版本，缺省 7（1.7.10 可用的经典效果）。 */
  since?: number;
}

/** 按目标区间过滤状态效果（查表取 since，兼容选项表与已选输入）。 */
export function filterEffectsByVersion<T extends { id: string }>(effects: T[], version: GiveVersion): T[] {
  const max = maxVersionOf(version);
  return effects.filter((e) => (POTION_EFFECTS.find((x) => x.id === e.id)?.since ?? 7) <= max);
}

export const POTION_EFFECTS: PotionEffect[] = [
  { id: "speed", name: "速度", nameEn: "Speed", numId: 0 },
  { id: "slowness", name: "缓慢", nameEn: "Slowness", numId: 1 },
  { id: "haste", name: "急迫", nameEn: "Haste", numId: 2 },
  { id: "mining_fatigue", name: "挖掘疲劳", nameEn: "Mining Fatigue", numId: 3 },
  { id: "strength", name: "力量", nameEn: "Strength", numId: 4 },
  { id: "instant_health", name: "瞬间治疗", nameEn: "Instant Health", numId: 5 },
  { id: "instant_damage", name: "瞬间伤害", nameEn: "Instant Damage", numId: 6 },
  { id: "jump_boost", name: "跳跃提升", nameEn: "Jump Boost", numId: 7 },
  { id: "nausea", name: "反胃", nameEn: "Nausea", numId: 8 },
  { id: "regeneration", name: "再生", nameEn: "Regeneration", numId: 9 },
  { id: "resistance", name: "抗性提升", nameEn: "Resistance", numId: 10 },
  { id: "fire_resistance", name: "火焰抗性", nameEn: "Fire Resistance", numId: 11 },
  { id: "water_breathing", name: "水下呼吸", nameEn: "Water Breathing", numId: 12 },
  { id: "invisibility", name: "隐身", nameEn: "Invisibility", numId: 13 },
  { id: "blindness", name: "失明", nameEn: "Blindness", numId: 14 },
  { id: "night_vision", name: "夜视", nameEn: "Night Vision", numId: 15 },
  { id: "hunger", name: "饥饿", nameEn: "Hunger", numId: 16 },
  { id: "weakness", name: "虚弱", nameEn: "Weakness", numId: 17 },
  { id: "poison", name: "中毒", nameEn: "Poison", numId: 18 },
  { id: "wither", name: "凋零", nameEn: "Wither", numId: 19 },
  { id: "health_boost", name: "生命提升", nameEn: "Health Boost", numId: 20 },
  { id: "absorption", name: "伤害吸收", nameEn: "Absorption", numId: 21 },
  { id: "saturation", name: "饱和", nameEn: "Saturation", numId: 22 },
  { id: "glowing", name: "发光", nameEn: "Glowing", numId: 23, since: 9 },
  { id: "levitation", name: "漂浮", nameEn: "Levitation", numId: 24, since: 9 },
  { id: "luck", name: "幸运", nameEn: "Luck", numId: 25, since: 9 },
  { id: "unluck", name: "霉运", nameEn: "Bad Luck", numId: 26, since: 9 },
  { id: "slow_falling", name: "缓降", nameEn: "Slow Falling", numId: 27, since: 13 },
  { id: "conduit_power", name: "潮涌能量", nameEn: "Conduit Power", numId: 28, since: 13 },
  { id: "dolphins_grace", name: "海豚的恩惠", nameEn: "Dolphin's Grace", numId: 29, since: 13 },
  { id: "bad_omen", name: "不祥之兆", nameEn: "Bad Omen", numId: 30, since: 14 },
  { id: "hero_of_the_village", name: "村庄英雄", nameEn: "Hero of the Village", numId: 31, since: 14 },
  { id: "darkness", name: "黑暗", nameEn: "Darkness", numId: 32 , since: 19},
  { id: "trial_omen", name: "试炼之兆", nameEn: "Trial Omen", numId: 33 , since: 21},
  { id: "raid_omen", name: "袭击之兆", nameEn: "Raid Omen", numId: 34 , since: 21},
  { id: "wind_charged", name: "风击", nameEn: "Wind Charged", numId: 35 , since: 21},
  { id: "weaving", name: "盘丝", nameEn: "Weaving", numId: 36 , since: 21},
  { id: "oozing", name: "渗浆", nameEn: "Oozing", numId: 37 , since: 21},
  { id: "infested", name: "寄生", nameEn: "Infested", numId: 38 , since: 21},
];

/** 基础药水（现代组件中的 potion 值需 minecraft: 前缀）。 */
export interface PotionBase {
  id: string;
  name: string;
  /** 官方英文名（缺失时回退 id）。 */
  nameEn?: string;
  /** 引入主版本，缺省 7（1.7.10 可用的经典药水）。 */
  since?: number;
}

export const POTION_BASES: PotionBase[] = [
  { id: "swiftness", name: "迅捷", nameEn: "Swiftness" },
  { id: "slowness", name: "缓慢", nameEn: "Slowness" },
  { id: "strength", name: "力量", nameEn: "Strength" },
  { id: "healing", name: "治疗", nameEn: "Healing" },
  { id: "harming", name: "伤害", nameEn: "Harming" },
  { id: "poison", name: "剧毒", nameEn: "Poison" },
  { id: "regeneration", name: "再生", nameEn: "Regeneration" },
  { id: "weakness", name: "虚弱", nameEn: "Weakness" },
  { id: "leaping", name: "跳跃", nameEn: "Leaping" },
  { id: "fire_resistance", name: "火焰抗性", nameEn: "Fire Resistance" },
  { id: "water_breathing", name: "水下呼吸", nameEn: "Water Breathing" },
  { id: "invisibility", name: "隐身", nameEn: "Invisibility" },
  { id: "night_vision", name: "夜视", nameEn: "Night Vision" },
  { id: "turtle_master", name: "神龟", nameEn: "Turtle Master", since: 13 },
  { id: "slow_falling", name: "缓降", nameEn: "Slow Falling", since: 13 },
  { id: "luck", name: "幸运", nameEn: "Luck", since: 9 },
  { id: "wind_charged", name: "风击", nameEn: "Wind Charged", since: 21 },
  { id: "weaving", name: "盘丝", nameEn: "Weaving", since: 21 },
  { id: "oozing", name: "渗浆", nameEn: "Oozing", since: 21 },
  { id: "infestation", name: "寄生", nameEn: "Infestation", since: 21 },
];

/** 按目标区间过滤基础药水（查表取 since）。 */
export function filterPotionBasesByVersion<T extends { id: string }>(bases: T[], version: GiveVersion): T[] {
  const max = maxVersionOf(version);
  return bases.filter((b) => (POTION_BASES.find((x) => x.id === b.id)?.since ?? 7) <= max);
}

export const POTION_ITEM_KINDS: { id: string; name: string; nameEn?: string }[] = [
  { id: "potion", name: "药水", nameEn: "Potion" },
  { id: "splash_potion", name: "喷溅药水", nameEn: "Splash Potion" },
  { id: "lingering_potion", name: "滞留药水", nameEn: "Lingering Potion" },
  { id: "tipped_arrow", name: "药水箭", nameEn: "Tipped Arrow" },
];

export interface PotionEffectInput {
  id: string;
  amplifier: number;
  /** 时长（tick；0 = 瞬时效果按 1 tick）。 */
  duration: number;
}

export interface PotionOptions {
  itemKind: string;
  potionId: string;
  effects: PotionEffectInput[];
  /** null = 不设置。 */
  customColor: number | null;
  customName: string;
  colorCode: string;
  count: number;
}

export function buildPotionCommand(opts: PotionOptions, version: GiveVersion): string {
  const kind = POTION_ITEM_KINDS.some((k) => k.id === opts.itemKind)
    ? opts.itemKind
    : "potion";
  const id = `minecraft:${kind}`;
  const count = Math.max(1, Math.min(64, Math.floor(opts.count) || 1));
  const suffix = syntaxOfVersion(version) === "legacy"
    ? buildPotionLegacy(id, opts, version)
    : buildPotionModern(id, opts, version);
  return `/give @p ${suffix} ${count}`;
}

/** 药水 legacy 标签（含显示名）：/give 内文与物品栈共用。 */
function potionLegacyTags(opts: PotionOptions, version: GiveVersion): string[] {
  const tags: string[] = [];
  const max = maxVersionOf(version);
  const potionId = opts.potionId && (POTION_BASES.find((b) => b.id === opts.potionId)?.since ?? 7) <= max
    ? opts.potionId
    : "";
  // 1.9 起药水类型改用 Potion 标签；更早版本由物品 Damage 决定
  if (potionId && max >= 9) {
    tags.push(`Potion:"minecraft:${potionId}"`);
  }
  const effects = filterEffectsByVersion(opts.effects, version);
  if (effects.length > 0) {
    const list = effects
      .map((e) => {
        const fx = POTION_EFFECTS.find((f) => f.id === e.id);
        const amp = Math.max(0, Math.min(e.amplifier, 255));
        const dur = Math.max(1, Math.min(Math.floor(e.duration) || 1, 1000000));
        return `{Id:${fx?.numId ?? 0}b,Amplifier:${amp}b,Duration:${dur}}`;
      })
      .join(",");
    tags.push(`CustomPotionEffects:[${list}]`);
  }
  // 1.9 起自定义液体颜色进入 NBT；1.20.5 前标签名是 CustomPotionColor
  if (opts.customColor !== null && max >= 9) {
    tags.push(`CustomPotionColor:${opts.customColor}`);
  }
  const name = `${opts.colorCode}${opts.customName}`.trim();
  if (name) {
    tags.push(`display:{Name:'{"text":"${name}"}'}`);
  }
  return tags;
}

/** 药水现代组件键值对（含 custom_name）：/give 内文 `k=v` 与物品栈 `"minecraft:k":v` 共用。 */
function potionModernPairs(opts: PotionOptions, version: GiveVersion): { key: string; value: string }[] {
  const pairs: { key: string; value: string }[] = [];
  const pc: string[] = [];
  const max = maxVersionOf(version);
  const potionId = opts.potionId && (POTION_BASES.find((b) => b.id === opts.potionId)?.since ?? 7) <= max
    ? opts.potionId
    : "";
  if (potionId) {
    pc.push(`potion:"minecraft:${potionId}"`);
  }
  if (opts.customColor !== null) {
    pc.push(`custom_color:${opts.customColor}`);
  }
  const effects = filterEffectsByVersion(opts.effects, version);
  if (effects.length > 0) {
    const list = effects
      .map((e) => {
        const amp = Math.max(0, Math.min(e.amplifier, 255));
        const dur = Math.max(1, Math.min(Math.floor(e.duration) || 1, 1000000));
        return `{id:"minecraft:${e.id}",amplifier:${amp},duration:${dur}}`;
      })
      .join(",");
    pc.push(`custom_effects:[${list}]`);
  }
  if (pc.length > 0) {
    pairs.push({ key: "potion_contents", value: `{${pc.join(",")}}` });
  }
  const name = `${opts.colorCode}${opts.customName}`.trim();
  if (name) {
    pairs.push({ key: "custom_name", value: `'"${name.replace(/"/g, '\\"')}"'` });
  }
  return pairs;
}

function buildPotionLegacy(id: string, opts: PotionOptions, version: GiveVersion): string {
  const tags = potionLegacyTags(opts, version);
  return tags.length > 0 ? `${id}{${tags.join(",")}}` : id;
}

function buildPotionModern(id: string, opts: PotionOptions, version: GiveVersion): string {
  const pairs = potionModernPairs(opts, version);
  return pairs.length > 0 ? `${id}[${pairs.map((p) => `${p.key}=${p.value}`).join(",")}]` : id;
}

/** 药水物品栈片段（村民交易等场景引用）：modern `{id,count,components:{...}}` / legacy `{id,Count:Nb,标签...}`。 */
export function buildPotionStack(opts: PotionOptions, version: GiveVersion): string {
  const kind = POTION_ITEM_KINDS.some((k) => k.id === opts.itemKind) ? opts.itemKind : "potion";
  const id = `minecraft:${kind}`;
  const count = Math.max(1, Math.min(64, Math.floor(opts.count) || 1));
  if (syntaxOfVersion(version) === "modern") {
    const pairs = potionModernPairs(opts, version);
    const comps = pairs.length > 0
      ? `,components:{${pairs.map((p) => `"minecraft:${p.key}":${p.value}`).join(",")}}`
      : "";
    return `{id:"${id}",count:${count}${comps}}`;
  }
  const tags = potionLegacyTags(opts, version);
  const ref = legacyItemRef(kind, version);
  const damageStr = ref.damage !== 0 ? `,Damage:${ref.damage}s` : "";
  const tagStr = tags.length > 0 ? `,tag:{${tags.join(",")}}` : "";
  return `{id:"minecraft:${ref.id}",Count:${count}b${damageStr}${tagStr}}`;
}

// ── 烟花 ──

/** legacy Type 数字 / modern shape 名。 */
export interface FireworkShape {
  id: string;
  name: string;
  /** 官方英文名（缺失时回退 id）。 */
  nameEn?: string;
  numId: number;
}

export const FIREWORK_SHAPES: FireworkShape[] = [
  { id: "small_ball", name: "小型球体", nameEn: "Small Ball", numId: 0 },
  { id: "large_ball", name: "大型球体", nameEn: "Large Ball", numId: 1 },
  { id: "star", name: "星形", nameEn: "Star", numId: 2 },
  { id: "creeper", name: "苦力怕形", nameEn: "Creeper", numId: 3 },
  { id: "burst", name: "爆裂", nameEn: "Burst", numId: 4 },
];

export interface FireworkExplosion {
  shape: string;
  colors: number[];
  fadeColors: number[];
  trail: boolean;
  /** modern 组件名 has_twinkle；legacy NBT 为 Flicker。 */
  twinkle: boolean;
}

export interface FireworkOptions {
  itemKind: string;
  /** 1–3（仅火箭）。 */
  flight: number;
  explosions: FireworkExplosion[];
  count: number;
}

export function buildFireworkCommand(opts: FireworkOptions, version: GiveVersion): string {
  const isStar = opts.itemKind === "firework_star";
  const id = `minecraft:${isStar ? "firework_star" : "firework_rocket"}`;
  const count = Math.max(1, Math.min(64, Math.floor(opts.count) || 1));
  const suffix = syntaxOfVersion(version) === "legacy"
    ? buildFireworkLegacy(id, isStar, opts)
    : buildFireworkModern(id, isStar, opts);
  return `/give @p ${suffix} ${count}`;
}

function explosionLegacy(e: FireworkExplosion): string {
  const shape = FIREWORK_SHAPES.find((s) => s.id === e.shape);
  const colors = e.colors.length > 0 ? `Colors:[I;${e.colors.join(",")}]` : "Colors:[I;]";
  const fades = e.fadeColors.length > 0 ? `FadeColors:[I;${e.fadeColors.join(",")}]` : "FadeColors:[I;]";
  return `{${colors},${fades},Type:${shape?.numId ?? 0}b,Trail:${e.trail ? "1b" : "0b"},Flicker:${e.twinkle ? "1b" : "0b"}}`;
}

/** 烟花 legacy 内文（`Fireworks:{...}` / `Explosion:{...}`）：命令与物品栈共用；无内容返回 null。 */
function fireworkLegacyInner(isStar: boolean, opts: FireworkOptions): string | null {
  if (isStar) {
    const e = opts.explosions[0];
    return e ? `Explosion:${explosionLegacy(e)}` : null;
  }
  const flight = Math.max(1, Math.min(opts.flight, 3));
  const list = opts.explosions.map(explosionLegacy).join(",");
  return `Fireworks:{Flight:${flight}b,Explosions:[${list}]}`;
}

function buildFireworkLegacy(id: string, isStar: boolean, opts: FireworkOptions): string {
  const inner = fireworkLegacyInner(isStar, opts);
  return inner ? `${id}{${inner}}` : id;
}

function explosionModern(e: FireworkExplosion): string {
  const shape = FIREWORK_SHAPES.find((s) => s.id === e.shape);
  const colors = e.colors.length > 0 ? `[${e.colors.join(",")}]` : "[]";
  const fades = e.fadeColors.length > 0 ? `[${e.fadeColors.join(",")}]` : "[]";
  return `{shape:"${shape?.id ?? "small_ball"}",colors:${colors},fade_colors:${fades},has_trail:${e.trail},has_twinkle:${e.twinkle}}`;
}

/** 烟花现代组件键值对：/give 内文 `k=v` 与物品栈 `"minecraft:k":v` 共用。 */
function fireworkModernPairs(isStar: boolean, opts: FireworkOptions): { key: string; value: string }[] {
  if (isStar) {
    const e = opts.explosions[0];
    // explosionModern 已含花括号，不能重复包裹
    return e ? [{ key: "firework_explosion", value: explosionModern(e) }] : [];
  }
  const flight = Math.max(1, Math.min(opts.flight, 3));
  const list = opts.explosions.map(explosionModern).join(",");
  return [{ key: "fireworks", value: `{flight_duration:${flight},explosions:[${list}]}` }];
}

function buildFireworkModern(id: string, isStar: boolean, opts: FireworkOptions): string {
  const pairs = fireworkModernPairs(isStar, opts);
  return pairs.length > 0 ? `${id}[${pairs.map((p) => `${p.key}=${p.value}`).join(",")}]` : id;
}

/** 烟花物品栈片段（村民交易等场景引用）：modern `{id,count,components:{...}}` / legacy `{id,Count:Nb,内文}`。 */
export function buildFireworkStack(opts: FireworkOptions, version: GiveVersion): string {
  const isStar = opts.itemKind === "firework_star";
  const id = `minecraft:${isStar ? "firework_star" : "firework_rocket"}`;
  const count = Math.max(1, Math.min(64, Math.floor(opts.count) || 1));
  if (syntaxOfVersion(version) === "modern") {
    const pairs = fireworkModernPairs(isStar, opts);
    const comps = pairs.length > 0
      ? `,components:{${pairs.map((p) => `"minecraft:${p.key}":${p.value}`).join(",")}}`
      : "";
    return `{id:"${id}",count:${count}${comps}}`;
  }
  const inner = fireworkLegacyInner(isStar, opts);
  const ref = legacyItemRef(isStar ? "firework_star" : "firework_rocket", version);
  const tagStr = inner ? `,tag:{${inner}}` : "";
  return `{id:"minecraft:${ref.id}",Count:${count}b${tagStr}}`;
}

/** 烟花附加内文（并入物品 NBT 生成的附加段）：modern 组件对（`"minecraft:k":v`）/ legacy 标签。 */
export function fireworkRawInner(opts: FireworkOptions, version: GiveVersion): string {
  const isStar = opts.itemKind === "firework_star";
  if (syntaxOfVersion(version) === "modern") {
    return fireworkModernPairs(isStar, opts)
      .map((p) => `"minecraft:${p.key}":${p.value}`)
      .join(",");
  }
  return fireworkLegacyInner(isStar, opts) ?? "";
}

/** 药水附加内文（并入物品 NBT 生成的附加段）：modern 组件对 / legacy 标签。 */
export function potionRawInner(opts: PotionOptions, version: GiveVersion): string {
  // 1.9 前药水类型由物品 Damage 决定，当前 UI 不生成该组合
  if (maxVersionOf(version) < 9) return "";
  if (syntaxOfVersion(version) === "modern") {
    return potionModernPairs(opts, version)
      .map((p) => `"minecraft:${p.key}":${p.value}`)
      .join(",");
  }
  return potionLegacyTags(opts, version).join(",");
}

// ── 村民交易 ──

export const VILLAGER_PROFESSIONS: { id: string; name: string; nameEn?: string }[] = [
  { id: "none", name: "无职业（失业）", nameEn: "Unemployed" },
  { id: "armorer", name: "盔甲匠", nameEn: "Armorer" },
  { id: "butcher", name: "屠夫", nameEn: "Butcher" },
  { id: "cartographer", name: "制图师", nameEn: "Cartographer" },
  { id: "cleric", name: "牧师", nameEn: "Cleric" },
  { id: "farmer", name: "农民", nameEn: "Farmer" },
  { id: "fisherman", name: "渔夫", nameEn: "Fisherman" },
  { id: "fletcher", name: "制箭师", nameEn: "Fletcher" },
  { id: "leatherworker", name: "皮匠", nameEn: "Leatherworker" },
  { id: "librarian", name: "图书管理员", nameEn: "Librarian" },
  { id: "mason", name: "石匠", nameEn: "Mason" },
  { id: "shepherd", name: "牧羊人", nameEn: "Shepherd" },
  { id: "toolsmith", name: "工具匠", nameEn: "Toolsmith" },
  { id: "weaponsmith", name: "武器匠", nameEn: "Weaponsmith" },
  { id: "nitwit", name: "傻子（不就业）", nameEn: "Nitwit" },
];

export const VILLAGER_TYPES: { id: string; name: string; nameEn?: string }[] = [
  { id: "plains", name: "平原", nameEn: "Plains" },
  { id: "desert", name: "沙漠", nameEn: "Desert" },
  { id: "savanna", name: "热带草原", nameEn: "Savanna" },
  { id: "taiga", name: "针叶林", nameEn: "Taiga" },
  { id: "snowy", name: "雪原", nameEn: "Snowy" },
  { id: "jungle", name: "丛林", nameEn: "Jungle" },
  { id: "swamp", name: "沼泽", nameEn: "Swamp" },
];

/** 物品槽自由内容（两档分开保存，切档不丢数据）。 */
export interface SlotRaw {
  legacy: string;
  modern: string;
}

export interface VillagerTrade {
  buyId: string;
  buyCount: number;
  /** 空 = 不自定义。 */
  buyMods?: ItemMods;
  /** 槽级自定义 NBT：以 `{` 开头按完整物品栈覆盖，否则并入当前档附加内容。 */
  buyRaw?: SlotRaw;
  buyBId: string;
  buyBCount: number;
  buyBMods?: ItemMods;
  buyBRaw?: SlotRaw;
  sellId: string;
  sellCount: number;
  sellMods?: ItemMods;
  sellRaw?: SlotRaw;
  maxUses: number;
  rewardExp: boolean;
}

export interface VillagerOptions {
  /** 村民世代：1.14+ 有职业/等级体系（VillagerData）；1.13 及以下无职业，仅 Offers 生效。 */
  era: "1.14+" | "pre1.14";
  profession: string;
  type: string;
  /** 1–5。 */
  level: number;
  trades: VillagerTrade[];
}

/** 村民实体 NBT 内文（不含外层花括号）：`VillagerData:...,Offers:...`；不可用世代仅 Offers，无交易时为空串。 */
export function buildVillagerNbt(opts: VillagerOptions, version: GiveVersion): string {
  const format = syntaxOfVersion(version);
  const tags: string[] = [];
  if (opts.era === "1.14+") {
    const profession = VILLAGER_PROFESSIONS.some((p) => p.id === opts.profession)
      ? opts.profession
      : "none";
    const type = VILLAGER_TYPES.some((t) => t.id === opts.type) ? opts.type : "plains";
    const level = Math.max(1, Math.min(opts.level, 5));
    tags.push(
      `VillagerData:{profession:"minecraft:${profession}",level:${level},type:"minecraft:${type}"}`,
    );
  }
  if (opts.trades.length > 0) {
    const recipes = opts.trades
      .map((t) => {
        const parts: string[] = [];
        parts.push(
          `buy:${buildTradeItem(t.buyId, t.buyCount, t.buyMods ?? EMPTY_MODS, version, t.buyRaw?.[format])}`,
        );
        if (t.buyBId) {
          parts.push(
            `buyB:${buildTradeItem(t.buyBId, t.buyBCount, t.buyBMods ?? EMPTY_MODS, version, t.buyBRaw?.[format])}`,
          );
        }
        parts.push(
          `sell:${buildTradeItem(t.sellId, t.sellCount, t.sellMods ?? EMPTY_MODS, version, t.sellRaw?.[format])}`,
        );
        parts.push(`maxUses:${Math.max(1, Math.min(t.maxUses, 9999))}`);
        parts.push(`rewardExp:${t.rewardExp ? "1b" : "0b"}`);
        return `{${parts.join(",")}}`;
      })
      .join(",");
    tags.push(`Offers:{Recipes:[${recipes}]}`);
  }
  return tags.join(",");
}

export function buildVillagerCommand(opts: VillagerOptions, version: GiveVersion): string {
  const body = buildVillagerNbt(opts, version);
  return body.length > 0 ? `/summon villager ~ ~ ~ {${body}}` : "/summon villager ~ ~ ~";
}
