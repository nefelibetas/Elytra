// NBT 构造器：自定义药水 / 烟花 / 村民交易，双版本语法命令构建。
//
// 与 give.ts 同构：legacy = 1.13–1.20.4 NBT；modern = 1.20.5+ 组件格式。
// 数据来源：
// - 效果表：minecraft-data 1.21.4 effects.json（数字 ID 0–38，1.13–1.20.4 NBT 用）
// - 烟花/村民结构：Minecraft Wiki 当前版本 Data values（实体 NBT 未组件化，
//   1.20.5 组件化仅影响物品槽格式：{id,count} 无 b 后缀）

import { GIVE_ITEMS, buildTradeItem, EMPTY_MODS, type ItemMods, type GiveFormat } from "./give";

// ── 公共助手 ──

/** 十六进制色（#RRGGBB）→ RGB packed int（NBT Colors / custom_color 用）。 */
export function rgbToInt(hex: string): number {
  const m = /^#?([0-9a-fA-F]{6})$/.exec(hex.trim());
  if (!m) return 0;
  return parseInt(m[1], 16);
}

/** 物品 ID 是否存在（交易物品输入校验）。 */
export function isKnownItem(id: string): boolean {
  return GIVE_ITEMS.some((i) => i.id === id) || id === "emerald";
}

// ── 药水 ──

/** 状态效果（数字 ID = 1.13–1.20.4 NBT 用；命名空间 = 1.20.5+ 组件用）。 */
export interface PotionEffect {
  id: string;
  name: string;
  numId: number;
  /** 引入主版本（如 1.21 风击；缺省 13）。 */
  since?: number;
}

/** 按命令格式过滤效果版本可用性：legacy = ≤1.20.4；modern = ≤1.21。 */
export function filterEffectsByVersion(effects: PotionEffect[], format: GiveFormat): PotionEffect[] {
  const max = format === "legacy" ? 20 : 21;
  return effects.filter((e) => (e.since ?? 13) <= max);
}

export const POTION_EFFECTS: PotionEffect[] = [
  { id: "speed", name: "速度", numId: 0 },
  { id: "slowness", name: "缓慢", numId: 1 },
  { id: "haste", name: "急迫", numId: 2 },
  { id: "mining_fatigue", name: "挖掘疲劳", numId: 3 },
  { id: "strength", name: "力量", numId: 4 },
  { id: "instant_health", name: "瞬间治疗", numId: 5 },
  { id: "instant_damage", name: "瞬间伤害", numId: 6 },
  { id: "jump_boost", name: "跳跃提升", numId: 7 },
  { id: "nausea", name: "反胃", numId: 8 },
  { id: "regeneration", name: "再生", numId: 9 },
  { id: "resistance", name: "抗性提升", numId: 10 },
  { id: "fire_resistance", name: "火焰抗性", numId: 11 },
  { id: "water_breathing", name: "水下呼吸", numId: 12 },
  { id: "invisibility", name: "隐身", numId: 13 },
  { id: "blindness", name: "失明", numId: 14 },
  { id: "night_vision", name: "夜视", numId: 15 },
  { id: "hunger", name: "饥饿", numId: 16 },
  { id: "weakness", name: "虚弱", numId: 17 },
  { id: "poison", name: "中毒", numId: 18 },
  { id: "wither", name: "凋零", numId: 19 },
  { id: "health_boost", name: "生命提升", numId: 20 },
  { id: "absorption", name: "伤害吸收", numId: 21 },
  { id: "saturation", name: "饱和", numId: 22 },
  { id: "glowing", name: "发光", numId: 23 },
  { id: "levitation", name: "漂浮", numId: 24 },
  { id: "luck", name: "幸运", numId: 25 },
  { id: "unluck", name: "霉运", numId: 26 },
  { id: "slow_falling", name: "缓降", numId: 27 },
  { id: "conduit_power", name: "潮涌能量", numId: 28 },
  { id: "dolphins_grace", name: "海豚的恩惠", numId: 29 },
  { id: "bad_omen", name: "不祥之兆", numId: 30 },
  { id: "hero_of_the_village", name: "村庄英雄", numId: 31 },
  { id: "darkness", name: "黑暗", numId: 32 , since: 19},
  { id: "trial_omen", name: "试炼之兆", numId: 33 , since: 21},
  { id: "raid_omen", name: "袭击之兆", numId: 34 , since: 21},
  { id: "wind_charged", name: "风击", numId: 35 , since: 21},
  { id: "weaving", name: "盘丝", numId: 36 , since: 21},
  { id: "oozing", name: "渗浆", numId: 37 , since: 21},
  { id: "infested", name: "寄生", numId: 38 , since: 21},
];

/** 基础药水（potion 标签值；1.20.5+ 需 minecraft: 前缀）。 */
export const POTION_BASES: { id: string; name: string }[] = [
  { id: "swiftness", name: "迅捷" },
  { id: "slowness", name: "缓慢" },
  { id: "strength", name: "力量" },
  { id: "healing", name: "治疗" },
  { id: "harming", name: "伤害" },
  { id: "poison", name: "剧毒" },
  { id: "regeneration", name: "再生" },
  { id: "weakness", name: "虚弱" },
  { id: "leaping", name: "跳跃" },
  { id: "fire_resistance", name: "火焰抗性" },
  { id: "water_breathing", name: "水下呼吸" },
  { id: "invisibility", name: "隐身" },
  { id: "night_vision", name: "夜视" },
  { id: "turtle_master", name: "神龟" },
  { id: "slow_falling", name: "缓降" },
  { id: "luck", name: "幸运" },
  { id: "wind_charged", name: "风击" },
  { id: "weaving", name: "盘丝" },
  { id: "oozing", name: "渗浆" },
  { id: "infestation", name: "寄生" },
];

/** 药水物品变体。 */
export const POTION_ITEM_KINDS: { id: string; name: string }[] = [
  { id: "potion", name: "药水" },
  { id: "splash_potion", name: "喷溅药水" },
  { id: "lingering_potion", name: "滞留药水" },
  { id: "tipped_arrow", name: "药水箭" },
];

/** 自定义效果条目。 */
export interface PotionEffectInput {
  id: string;
  amplifier: number;
  /** 时长（tick；0 = 瞬时效果自动 1 tick）。 */
  duration: number;
}

export interface PotionOptions {
  itemKind: string;
  /** 基础药水 id（'' = 无）。 */
  potionId: string;
  effects: PotionEffectInput[];
  /** 自定义颜色（RGB packed int；null = 不设置）。 */
  customColor: number | null;
  customName: string;
  colorCode: string;
  count: number;
}

export function buildPotionCommand(opts: PotionOptions, format: "legacy" | "modern"): string {
  const kind = POTION_ITEM_KINDS.some((k) => k.id === opts.itemKind)
    ? opts.itemKind
    : "potion";
  const id = `minecraft:${kind}`;
  const count = Math.max(1, Math.min(64, Math.floor(opts.count) || 1));
  const suffix = format === "legacy" ? buildPotionLegacy(id, opts) : buildPotionModern(id, opts);
  return `/give @p ${suffix} ${count}`;
}

function buildPotionLegacy(id: string, opts: PotionOptions): string {
  const tags: string[] = [];
  if (opts.potionId) {
    tags.push(`Potion:"minecraft:${opts.potionId}"`);
  }
  if (opts.effects.length > 0) {
    const list = opts.effects
      .map((e) => {
        const fx = POTION_EFFECTS.find((f) => f.id === e.id);
        const amp = Math.max(0, Math.min(e.amplifier, 255));
        const dur = Math.max(1, Math.min(Math.floor(e.duration) || 1, 1000000));
        return `{Id:${fx?.numId ?? 0}b,Amplifier:${amp}b,Duration:${dur}}`;
      })
      .join(",");
    tags.push(`CustomPotionEffects:[${list}]`);
  }
  if (opts.customColor !== null) {
    tags.push(`CustomColor:${opts.customColor}`);
  }
  const name = `${opts.colorCode}${opts.customName}`.trim();
  if (name) {
    tags.push(`display:{Name:'{"text":"${name}"}'}`);
  }
  return tags.length > 0 ? `${id}{${tags.join(",")}}` : id;
}

function buildPotionModern(id: string, opts: PotionOptions): string {
  const parts: string[] = [];
  const pc: string[] = [];
  if (opts.potionId) {
    pc.push(`potion:"minecraft:${opts.potionId}"`);
  }
  if (opts.customColor !== null) {
    pc.push(`custom_color:${opts.customColor}`);
  }
  if (opts.effects.length > 0) {
    const list = opts.effects
      .map((e) => {
        const amp = Math.max(0, Math.min(e.amplifier, 255));
        const dur = Math.max(1, Math.min(Math.floor(e.duration) || 1, 1000000));
        return `{id:"minecraft:${e.id}",amplifier:${amp},duration:${dur}}`;
      })
      .join(",");
    pc.push(`custom_effects:[${list}]`);
  }
  if (pc.length > 0) {
    parts.push(`potion_contents={${pc.join(",")}}`);
  }
  const name = `${opts.colorCode}${opts.customName}`.trim();
  if (name) {
    parts.push(`custom_name='"${name.replace(/"/g, '\\"')}"'`);
  }
  return parts.length > 0 ? `${id}[${parts.join(",")}]` : id;
}

// ── 烟花 ──

/** 爆炸形状（legacy Type 数字 / modern shape 名）。 */
export interface FireworkShape {
  id: string;
  name: string;
  numId: number;
}

export const FIREWORK_SHAPES: FireworkShape[] = [
  { id: "small_ball", name: "小型球体", numId: 0 },
  { id: "large_ball", name: "大型球体", numId: 1 },
  { id: "star", name: "星形", numId: 2 },
  { id: "creeper", name: "苦力怕形", numId: 3 },
  { id: "burst", name: "爆裂", numId: 4 },
];

/** 单次爆炸。 */
export interface FireworkExplosion {
  shape: string;
  /** 初始颜色（RGB packed int 数组）。 */
  colors: number[];
  /** 渐隐颜色。 */
  fadeColors: number[];
  trail: boolean;
  /** 1.20.5+ 组件名 has_twinkle；legacy NBT 为 Flicker。 */
  twinkle: boolean;
}

export interface FireworkOptions {
  /** firework_rocket / firework_star。 */
  itemKind: string;
  /** 飞行时长 1–3（仅火箭）。 */
  flight: number;
  explosions: FireworkExplosion[];
  count: number;
}

export function buildFireworkCommand(opts: FireworkOptions, format: "legacy" | "modern"): string {
  const isStar = opts.itemKind === "firework_star";
  const id = `minecraft:${isStar ? "firework_star" : "firework_rocket"}`;
  const count = Math.max(1, Math.min(64, Math.floor(opts.count) || 1));
  const suffix = format === "legacy"
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

function buildFireworkLegacy(id: string, isStar: boolean, opts: FireworkOptions): string {
  if (isStar) {
    const e = opts.explosions[0];
    return e ? `${id}{Explosion:${explosionLegacy(e)}}` : id;
  }
  const flight = Math.max(1, Math.min(opts.flight, 3));
  const list = opts.explosions.map(explosionLegacy).join(",");
  return `${id}{Fireworks:{Flight:${flight}b,Explosions:[${list}]}}`;
}

function explosionModern(e: FireworkExplosion): string {
  const shape = FIREWORK_SHAPES.find((s) => s.id === e.shape);
  const colors = e.colors.length > 0 ? `[${e.colors.join(",")}]` : "[]";
  const fades = e.fadeColors.length > 0 ? `[${e.fadeColors.join(",")}]` : "[]";
  return `{shape:"${shape?.id ?? "small_ball"}",colors:${colors},fade_colors:${fades},has_trail:${e.trail},has_twinkle:${e.twinkle}}`;
}

function buildFireworkModern(id: string, isStar: boolean, opts: FireworkOptions): string {
  if (isStar) {
    const e = opts.explosions[0];
    // explosionModern 已含花括号，不能重复包裹
    return e ? `${id}[firework_explosion=${explosionModern(e)}]` : id;
  }
  const flight = Math.max(1, Math.min(opts.flight, 3));
  const list = opts.explosions.map(explosionModern).join(",");
  return `${id}[fireworks={flight_duration:${flight},explosions:[${list}]}]`;
}

// ── 村民交易 ──

export const VILLAGER_PROFESSIONS: { id: string; name: string }[] = [
  { id: "none", name: "无职业" },
  { id: "armorer", name: "盔甲匠" },
  { id: "butcher", name: "屠夫" },
  { id: "cartographer", name: "制图师" },
  { id: "cleric", name: "牧师" },
  { id: "farmer", name: "农民" },
  { id: "fisherman", name: "渔夫" },
  { id: "fletcher", name: "制箭师" },
  { id: "leatherworker", name: "皮匠" },
  { id: "librarian", name: "图书管理员" },
  { id: "mason", name: "石匠" },
  { id: "shepherd", name: "牧羊人" },
  { id: "toolsmith", name: "工具匠" },
  { id: "weaponsmith", name: "武器匠" },
  { id: "nitwit", name: "傻子" },
];

export const VILLAGER_TYPES: { id: string; name: string }[] = [
  { id: "plains", name: "平原" },
  { id: "desert", name: "沙漠" },
  { id: "savanna", name: "热带草原" },
  { id: "taiga", name: "针叶林" },
  { id: "snowy", name: "雪原" },
  { id: "jungle", name: "丛林" },
  { id: "swamp", name: "沼泽" },
];

/** 单笔交易。 */
export interface VillagerTrade {
  buyId: string;
  buyCount: number;
  /** 购买物品修饰（名称/附魔等；空 = 不自定义）。 */
  buyMods?: ItemMods;
  /** 第二购买物品（'' = 无）。 */
  buyBId: string;
  buyBCount: number;
  buyBMods?: ItemMods;
  sellId: string;
  sellCount: number;
  sellMods?: ItemMods;
  maxUses: number;
  rewardExp: boolean;
}

export interface VillagerOptions {
  profession: string;
  type: string;
  /** 1–5。 */
  level: number;
  trades: VillagerTrade[];
}

export function buildVillagerCommand(opts: VillagerOptions, format: "legacy" | "modern"): string {
  const profession = VILLAGER_PROFESSIONS.some((p) => p.id === opts.profession)
    ? opts.profession
    : "none";
  const type = VILLAGER_TYPES.some((t) => t.id === opts.type) ? opts.type : "plains";
  const level = Math.max(1, Math.min(opts.level, 5));
  const tags: string[] = [];
  tags.push(
    `VillagerData:{profession:"minecraft:${profession}",level:${level},type:"minecraft:${type}"}`,
  );
  if (opts.trades.length > 0) {
    const recipes = opts.trades
      .map((t) => {
        const parts: string[] = [];
        parts.push(`buy:${buildTradeItem(t.buyId, t.buyCount, t.buyMods ?? EMPTY_MODS, format)}`);
        if (t.buyBId) {
          parts.push(`buyB:${buildTradeItem(t.buyBId, t.buyBCount, t.buyBMods ?? EMPTY_MODS, format)}`);
        }
        parts.push(`sell:${buildTradeItem(t.sellId, t.sellCount, t.sellMods ?? EMPTY_MODS, format)}`);
        parts.push(`maxUses:${Math.max(1, Math.min(t.maxUses, 9999))}`);
        parts.push(`rewardExp:${t.rewardExp ? "1b" : "0b"}`);
        return `{${parts.join(",")}}`;
      })
      .join(",");
    tags.push(`Offers:{Recipes:[${recipes}]}`);
  }
  return `/summon villager ~ ~ ~ {${tags.join(",")}}`;
}
