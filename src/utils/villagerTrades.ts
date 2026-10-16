// 村民交易预设 —— Java Edition 当前版本（源：minecraft.wiki/w/Trading，已用 zh wiki / 游戏数据文件交叉校验）
// want = "Villager wants"（玩家付出）；get = "Player receives"（玩家得到）；uses = "Trades in stock"（失效前可交易次数）
// 价格约定：附魔书/随机附魔装备的绿宝石价在游戏内浮动，此处取 wiki 区间下限；随机颜色交易用白色变体作基础 id。

export interface TradePresetItem {
  id: string;
  n: number;
}

export interface TradePreset {
  /** 职业 id（游戏内 id）。 */
  p: string;
  /** 等级 1–5。 */
  lv: number;
  want: TradePresetItem[];
  get: TradePresetItem[];
  uses?: number;
}

export const VILLAGER_TRADE_PRESETS: TradePreset[] = [
  // ── armorer 盔甲匠 ──
  { p: "armorer", lv: 1, want: [{ id: "coal", n: 15 }], get: [{ id: "emerald", n: 1 }], uses: 16 },
  { p: "armorer", lv: 1, want: [{ id: "emerald", n: 5 }], get: [{ id: "iron_helmet", n: 1 }], uses: 12 },
  { p: "armorer", lv: 1, want: [{ id: "emerald", n: 9 }], get: [{ id: "iron_chestplate", n: 1 }], uses: 12 },
  { p: "armorer", lv: 1, want: [{ id: "emerald", n: 7 }], get: [{ id: "iron_leggings", n: 1 }], uses: 12 },
  { p: "armorer", lv: 1, want: [{ id: "emerald", n: 4 }], get: [{ id: "iron_boots", n: 1 }], uses: 12 },
  { p: "armorer", lv: 2, want: [{ id: "iron_ingot", n: 4 }], get: [{ id: "emerald", n: 1 }], uses: 12 },
  { p: "armorer", lv: 2, want: [{ id: "emerald", n: 36 }], get: [{ id: "bell", n: 1 }], uses: 12 },
  { p: "armorer", lv: 2, want: [{ id: "emerald", n: 3 }], get: [{ id: "chainmail_leggings", n: 1 }], uses: 12 },
  { p: "armorer", lv: 2, want: [{ id: "emerald", n: 1 }], get: [{ id: "chainmail_boots", n: 1 }], uses: 12 },
  { p: "armorer", lv: 3, want: [{ id: "lava_bucket", n: 1 }], get: [{ id: "emerald", n: 1 }], uses: 12 },
  { p: "armorer", lv: 3, want: [{ id: "diamond", n: 1 }], get: [{ id: "emerald", n: 1 }], uses: 12 },
  { p: "armorer", lv: 3, want: [{ id: "emerald", n: 1 }], get: [{ id: "chainmail_helmet", n: 1 }], uses: 12 },
  { p: "armorer", lv: 3, want: [{ id: "emerald", n: 4 }], get: [{ id: "chainmail_chestplate", n: 1 }], uses: 12 },
  { p: "armorer", lv: 3, want: [{ id: "emerald", n: 5 }], get: [{ id: "shield", n: 1 }], uses: 12 },
  { p: "armorer", lv: 4, want: [{ id: "emerald", n: 19 }], get: [{ id: "diamond_leggings", n: 1 }], uses: 3 },
  { p: "armorer", lv: 4, want: [{ id: "emerald", n: 13 }], get: [{ id: "diamond_boots", n: 1 }], uses: 3 },
  { p: "armorer", lv: 5, want: [{ id: "emerald", n: 13 }], get: [{ id: "diamond_helmet", n: 1 }], uses: 3 },
  { p: "armorer", lv: 5, want: [{ id: "emerald", n: 21 }], get: [{ id: "diamond_chestplate", n: 1 }], uses: 3 },

  // ── butcher 屠夫 ──
  { p: "butcher", lv: 1, want: [{ id: "chicken", n: 14 }], get: [{ id: "emerald", n: 1 }], uses: 16 },
  { p: "butcher", lv: 1, want: [{ id: "rabbit", n: 4 }], get: [{ id: "emerald", n: 1 }], uses: 16 },
  { p: "butcher", lv: 1, want: [{ id: "porkchop", n: 7 }], get: [{ id: "emerald", n: 1 }], uses: 16 },
  { p: "butcher", lv: 1, want: [{ id: "emerald", n: 1 }], get: [{ id: "rabbit_stew", n: 1 }], uses: 12 },
  { p: "butcher", lv: 2, want: [{ id: "coal", n: 15 }], get: [{ id: "emerald", n: 1 }], uses: 16 },
  { p: "butcher", lv: 2, want: [{ id: "emerald", n: 1 }], get: [{ id: "cooked_chicken", n: 8 }], uses: 16 },
  { p: "butcher", lv: 2, want: [{ id: "emerald", n: 1 }], get: [{ id: "cooked_porkchop", n: 5 }], uses: 16 },
  { p: "butcher", lv: 3, want: [{ id: "beef", n: 10 }], get: [{ id: "emerald", n: 1 }], uses: 16 },
  { p: "butcher", lv: 3, want: [{ id: "mutton", n: 7 }], get: [{ id: "emerald", n: 1 }], uses: 16 },
  { p: "butcher", lv: 4, want: [{ id: "dried_kelp_block", n: 10 }], get: [{ id: "emerald", n: 1 }], uses: 12 },
  { p: "butcher", lv: 5, want: [{ id: "sweet_berries", n: 10 }], get: [{ id: "emerald", n: 1 }], uses: 12 },

  // ── cartographer 制图师 ──
  { p: "cartographer", lv: 1, want: [{ id: "paper", n: 24 }], get: [{ id: "emerald", n: 1 }], uses: 12 },
  { p: "cartographer", lv: 1, want: [{ id: "emerald", n: 7 }], get: [{ id: "map", n: 1 }], uses: 12 },
  { p: "cartographer", lv: 2, want: [{ id: "glass_pane", n: 11 }], get: [{ id: "emerald", n: 1 }], uses: 12 },
  { p: "cartographer", lv: 2, want: [{ id: "emerald", n: 8 }, { id: "compass", n: 1 }], get: [{ id: "filled_map", n: 1 }], uses: 12 },
  { p: "cartographer", lv: 3, want: [{ id: "compass", n: 1 }], get: [{ id: "emerald", n: 1 }], uses: 12 },
  { p: "cartographer", lv: 3, want: [{ id: "emerald", n: 13 }, { id: "compass", n: 1 }], get: [{ id: "filled_map", n: 1 }], uses: 12 },
  { p: "cartographer", lv: 3, want: [{ id: "emerald", n: 12 }, { id: "compass", n: 1 }], get: [{ id: "filled_map", n: 1 }], uses: 12 },
  { p: "cartographer", lv: 4, want: [{ id: "emerald", n: 7 }], get: [{ id: "item_frame", n: 1 }], uses: 12 },
  { p: "cartographer", lv: 4, want: [{ id: "emerald", n: 2 }], get: [{ id: "white_banner", n: 1 }], uses: 12 },
  { p: "cartographer", lv: 5, want: [{ id: "emerald", n: 8 }], get: [{ id: "globe_banner_pattern", n: 1 }], uses: 12 },
  { p: "cartographer", lv: 5, want: [{ id: "emerald", n: 14 }, { id: "compass", n: 1 }], get: [{ id: "filled_map", n: 1 }], uses: 12 },

  // ── cleric 牧师 ──
  { p: "cleric", lv: 1, want: [{ id: "rotten_flesh", n: 32 }], get: [{ id: "emerald", n: 1 }], uses: 16 },
  { p: "cleric", lv: 1, want: [{ id: "emerald", n: 1 }], get: [{ id: "redstone", n: 2 }], uses: 12 },
  { p: "cleric", lv: 2, want: [{ id: "gold_ingot", n: 3 }], get: [{ id: "emerald", n: 1 }], uses: 12 },
  { p: "cleric", lv: 2, want: [{ id: "emerald", n: 1 }], get: [{ id: "lapis_lazuli", n: 1 }], uses: 12 },
  { p: "cleric", lv: 3, want: [{ id: "rabbit_foot", n: 2 }], get: [{ id: "emerald", n: 1 }], uses: 12 },
  { p: "cleric", lv: 3, want: [{ id: "emerald", n: 4 }], get: [{ id: "glowstone", n: 1 }], uses: 12 },
  { p: "cleric", lv: 4, want: [{ id: "turtle_scute", n: 4 }], get: [{ id: "emerald", n: 1 }], uses: 12 },
  { p: "cleric", lv: 4, want: [{ id: "glass_bottle", n: 9 }], get: [{ id: "emerald", n: 1 }], uses: 12 },
  { p: "cleric", lv: 4, want: [{ id: "emerald", n: 5 }], get: [{ id: "ender_pearl", n: 1 }], uses: 12 },
  { p: "cleric", lv: 5, want: [{ id: "nether_wart", n: 22 }], get: [{ id: "emerald", n: 1 }], uses: 12 },
  { p: "cleric", lv: 5, want: [{ id: "emerald", n: 3 }], get: [{ id: "experience_bottle", n: 1 }], uses: 12 },

  // ── farmer 农民 ──
  { p: "farmer", lv: 1, want: [{ id: "wheat", n: 20 }], get: [{ id: "emerald", n: 1 }], uses: 16 },
  { p: "farmer", lv: 1, want: [{ id: "potato", n: 26 }], get: [{ id: "emerald", n: 1 }], uses: 16 },
  { p: "farmer", lv: 1, want: [{ id: "carrot", n: 22 }], get: [{ id: "emerald", n: 1 }], uses: 16 },
  { p: "farmer", lv: 1, want: [{ id: "beetroot", n: 15 }], get: [{ id: "emerald", n: 1 }], uses: 16 },
  { p: "farmer", lv: 1, want: [{ id: "emerald", n: 1 }], get: [{ id: "bread", n: 6 }], uses: 16 },
  { p: "farmer", lv: 2, want: [{ id: "pumpkin", n: 6 }], get: [{ id: "emerald", n: 1 }], uses: 12 },
  { p: "farmer", lv: 2, want: [{ id: "emerald", n: 1 }], get: [{ id: "pumpkin_pie", n: 4 }], uses: 12 },
  { p: "farmer", lv: 2, want: [{ id: "emerald", n: 1 }], get: [{ id: "apple", n: 4 }], uses: 16 },
  { p: "farmer", lv: 3, want: [{ id: "melon", n: 4 }], get: [{ id: "emerald", n: 1 }], uses: 12 },
  { p: "farmer", lv: 3, want: [{ id: "emerald", n: 3 }], get: [{ id: "cookie", n: 18 }], uses: 12 },
  { p: "farmer", lv: 4, want: [{ id: "emerald", n: 1 }], get: [{ id: "suspicious_stew", n: 1 }], uses: 12 },
  { p: "farmer", lv: 4, want: [{ id: "emerald", n: 1 }], get: [{ id: "cake", n: 1 }], uses: 12 },
  { p: "farmer", lv: 5, want: [{ id: "emerald", n: 3 }], get: [{ id: "golden_carrot", n: 3 }], uses: 12 },
  { p: "farmer", lv: 5, want: [{ id: "emerald", n: 4 }], get: [{ id: "glistering_melon_slice", n: 3 }], uses: 12 },

  // ── fisherman 渔夫 ──
  { p: "fisherman", lv: 1, want: [{ id: "string", n: 20 }], get: [{ id: "emerald", n: 1 }], uses: 16 },
  { p: "fisherman", lv: 1, want: [{ id: "coal", n: 10 }], get: [{ id: "emerald", n: 1 }], uses: 16 },
  { p: "fisherman", lv: 1, want: [{ id: "emerald", n: 3 }], get: [{ id: "cod_bucket", n: 1 }], uses: 16 },
  { p: "fisherman", lv: 1, want: [{ id: "cod", n: 6 }, { id: "emerald", n: 1 }], get: [{ id: "cooked_cod", n: 6 }], uses: 16 },
  { p: "fisherman", lv: 2, want: [{ id: "cod", n: 15 }], get: [{ id: "emerald", n: 1 }], uses: 16 },
  { p: "fisherman", lv: 2, want: [{ id: "emerald", n: 2 }], get: [{ id: "campfire", n: 1 }], uses: 12 },
  { p: "fisherman", lv: 2, want: [{ id: "salmon", n: 6 }, { id: "emerald", n: 1 }], get: [{ id: "cooked_salmon", n: 6 }], uses: 16 },
  { p: "fisherman", lv: 3, want: [{ id: "salmon", n: 13 }], get: [{ id: "emerald", n: 1 }], uses: 16 },
  { p: "fisherman", lv: 3, want: [{ id: "emerald", n: 8 }], get: [{ id: "fishing_rod", n: 1 }], uses: 3 },
  { p: "fisherman", lv: 4, want: [{ id: "tropical_fish", n: 6 }], get: [{ id: "emerald", n: 1 }], uses: 12 },
  { p: "fisherman", lv: 5, want: [{ id: "pufferfish", n: 4 }], get: [{ id: "emerald", n: 1 }], uses: 12 },
  { p: "fisherman", lv: 5, want: [{ id: "oak_boat", n: 1 }], get: [{ id: "emerald", n: 1 }], uses: 12 },

  // ── fletcher 制箭师 ──
  { p: "fletcher", lv: 1, want: [{ id: "stick", n: 32 }], get: [{ id: "emerald", n: 1 }], uses: 16 },
  { p: "fletcher", lv: 1, want: [{ id: "emerald", n: 1 }], get: [{ id: "arrow", n: 16 }], uses: 12 },
  { p: "fletcher", lv: 1, want: [{ id: "gravel", n: 10 }, { id: "emerald", n: 1 }], get: [{ id: "flint", n: 10 }], uses: 12 },
  { p: "fletcher", lv: 2, want: [{ id: "flint", n: 26 }], get: [{ id: "emerald", n: 1 }], uses: 12 },
  { p: "fletcher", lv: 2, want: [{ id: "emerald", n: 2 }], get: [{ id: "bow", n: 1 }], uses: 12 },
  { p: "fletcher", lv: 3, want: [{ id: "string", n: 14 }], get: [{ id: "emerald", n: 1 }], uses: 16 },
  { p: "fletcher", lv: 3, want: [{ id: "emerald", n: 3 }], get: [{ id: "crossbow", n: 1 }], uses: 12 },
  { p: "fletcher", lv: 4, want: [{ id: "feather", n: 24 }], get: [{ id: "emerald", n: 1 }], uses: 16 },
  { p: "fletcher", lv: 4, want: [{ id: "emerald", n: 7 }], get: [{ id: "bow", n: 1 }], uses: 3 },
  { p: "fletcher", lv: 5, want: [{ id: "tripwire_hook", n: 8 }], get: [{ id: "emerald", n: 1 }], uses: 12 },
  { p: "fletcher", lv: 5, want: [{ id: "emerald", n: 8 }], get: [{ id: "crossbow", n: 1 }], uses: 3 },
  { p: "fletcher", lv: 5, want: [{ id: "emerald", n: 2 }, { id: "arrow", n: 5 }], get: [{ id: "tipped_arrow", n: 5 }], uses: 12 },

  // ── leatherworker 皮匠 ──
  { p: "leatherworker", lv: 1, want: [{ id: "leather", n: 6 }], get: [{ id: "emerald", n: 1 }], uses: 16 },
  { p: "leatherworker", lv: 1, want: [{ id: "emerald", n: 3 }], get: [{ id: "leather_leggings", n: 1 }], uses: 12 },
  { p: "leatherworker", lv: 1, want: [{ id: "emerald", n: 7 }], get: [{ id: "leather_chestplate", n: 1 }], uses: 12 },
  { p: "leatherworker", lv: 2, want: [{ id: "flint", n: 26 }], get: [{ id: "emerald", n: 1 }], uses: 12 },
  { p: "leatherworker", lv: 2, want: [{ id: "emerald", n: 5 }], get: [{ id: "leather_helmet", n: 1 }], uses: 12 },
  { p: "leatherworker", lv: 2, want: [{ id: "emerald", n: 4 }], get: [{ id: "leather_boots", n: 1 }], uses: 12 },
  { p: "leatherworker", lv: 3, want: [{ id: "rabbit_hide", n: 9 }], get: [{ id: "emerald", n: 1 }], uses: 12 },
  { p: "leatherworker", lv: 3, want: [{ id: "emerald", n: 7 }], get: [{ id: "leather_chestplate", n: 1 }], uses: 12 },
  { p: "leatherworker", lv: 4, want: [{ id: "turtle_scute", n: 4 }], get: [{ id: "emerald", n: 1 }], uses: 12 },
  { p: "leatherworker", lv: 4, want: [{ id: "emerald", n: 6 }], get: [{ id: "leather_horse_armor", n: 1 }], uses: 12 },
  { p: "leatherworker", lv: 5, want: [{ id: "emerald", n: 5 }], get: [{ id: "leather_helmet", n: 1 }], uses: 12 },
  { p: "leatherworker", lv: 5, want: [{ id: "emerald", n: 6 }], get: [{ id: "saddle", n: 1 }], uses: 12 },

  // ── librarian 图书管理员 ──
  { p: "librarian", lv: 1, want: [{ id: "paper", n: 24 }], get: [{ id: "emerald", n: 1 }], uses: 16 },
  { p: "librarian", lv: 1, want: [{ id: "emerald", n: 9 }], get: [{ id: "bookshelf", n: 1 }], uses: 12 },
  { p: "librarian", lv: 1, want: [{ id: "emerald", n: 5 }, { id: "book", n: 1 }], get: [{ id: "enchanted_book", n: 1 }], uses: 12 },
  { p: "librarian", lv: 2, want: [{ id: "book", n: 4 }], get: [{ id: "emerald", n: 1 }], uses: 12 },
  { p: "librarian", lv: 2, want: [{ id: "emerald", n: 1 }], get: [{ id: "lantern", n: 1 }], uses: 12 },
  { p: "librarian", lv: 2, want: [{ id: "emerald", n: 5 }, { id: "book", n: 1 }], get: [{ id: "enchanted_book", n: 1 }], uses: 12 },
  { p: "librarian", lv: 3, want: [{ id: "ink_sac", n: 5 }], get: [{ id: "emerald", n: 1 }], uses: 12 },
  { p: "librarian", lv: 3, want: [{ id: "emerald", n: 1 }], get: [{ id: "glass", n: 4 }], uses: 12 },
  { p: "librarian", lv: 3, want: [{ id: "emerald", n: 5 }, { id: "book", n: 1 }], get: [{ id: "enchanted_book", n: 1 }], uses: 12 },
  { p: "librarian", lv: 4, want: [{ id: "writable_book", n: 1 }], get: [{ id: "emerald", n: 1 }], uses: 12 },
  { p: "librarian", lv: 4, want: [{ id: "emerald", n: 4 }], get: [{ id: "compass", n: 1 }], uses: 12 },
  { p: "librarian", lv: 4, want: [{ id: "emerald", n: 5 }], get: [{ id: "clock", n: 1 }], uses: 12 },
  { p: "librarian", lv: 4, want: [{ id: "emerald", n: 5 }, { id: "book", n: 1 }], get: [{ id: "enchanted_book", n: 1 }], uses: 12 },
  { p: "librarian", lv: 5, want: [{ id: "emerald", n: 3 }], get: [{ id: "red_candle", n: 1 }], uses: 12 },
  { p: "librarian", lv: 5, want: [{ id: "emerald", n: 3 }], get: [{ id: "yellow_candle", n: 1 }], uses: 12 },

  // ── mason 石匠 ──
  { p: "mason", lv: 1, want: [{ id: "clay_ball", n: 10 }], get: [{ id: "emerald", n: 1 }], uses: 16 },
  { p: "mason", lv: 1, want: [{ id: "emerald", n: 1 }], get: [{ id: "brick", n: 10 }], uses: 16 },
  { p: "mason", lv: 2, want: [{ id: "stone", n: 20 }], get: [{ id: "emerald", n: 1 }], uses: 16 },
  { p: "mason", lv: 2, want: [{ id: "emerald", n: 1 }], get: [{ id: "chiseled_stone_bricks", n: 4 }], uses: 16 },
  { p: "mason", lv: 3, want: [{ id: "granite", n: 16 }], get: [{ id: "emerald", n: 1 }], uses: 16 },
  { p: "mason", lv: 3, want: [{ id: "andesite", n: 16 }], get: [{ id: "emerald", n: 1 }], uses: 16 },
  { p: "mason", lv: 3, want: [{ id: "diorite", n: 16 }], get: [{ id: "emerald", n: 1 }], uses: 16 },
  { p: "mason", lv: 3, want: [{ id: "emerald", n: 1 }], get: [{ id: "dripstone_block", n: 4 }], uses: 16 },
  { p: "mason", lv: 3, want: [{ id: "emerald", n: 1 }], get: [{ id: "polished_andesite", n: 4 }], uses: 16 },
  { p: "mason", lv: 3, want: [{ id: "emerald", n: 1 }], get: [{ id: "polished_diorite", n: 4 }], uses: 16 },
  { p: "mason", lv: 3, want: [{ id: "emerald", n: 1 }], get: [{ id: "polished_granite", n: 4 }], uses: 16 },
  { p: "mason", lv: 4, want: [{ id: "quartz", n: 12 }], get: [{ id: "emerald", n: 1 }], uses: 12 },
  { p: "mason", lv: 4, want: [{ id: "emerald", n: 1 }], get: [{ id: "white_terracotta", n: 1 }], uses: 12 },
  { p: "mason", lv: 4, want: [{ id: "emerald", n: 1 }], get: [{ id: "white_glazed_terracotta", n: 1 }], uses: 12 },
  { p: "mason", lv: 5, want: [{ id: "emerald", n: 1 }], get: [{ id: "quartz_pillar", n: 1 }], uses: 12 },
  { p: "mason", lv: 5, want: [{ id: "emerald", n: 1 }], get: [{ id: "quartz_block", n: 1 }], uses: 12 },

  // ── shepherd 牧羊人 ──
  { p: "shepherd", lv: 1, want: [{ id: "white_wool", n: 18 }], get: [{ id: "emerald", n: 1 }], uses: 16 },
  { p: "shepherd", lv: 1, want: [{ id: "brown_wool", n: 18 }], get: [{ id: "emerald", n: 1 }], uses: 16 },
  { p: "shepherd", lv: 1, want: [{ id: "black_wool", n: 18 }], get: [{ id: "emerald", n: 1 }], uses: 16 },
  { p: "shepherd", lv: 1, want: [{ id: "gray_wool", n: 18 }], get: [{ id: "emerald", n: 1 }], uses: 16 },
  { p: "shepherd", lv: 1, want: [{ id: "emerald", n: 2 }], get: [{ id: "shears", n: 1 }], uses: 12 },
  { p: "shepherd", lv: 2, want: [{ id: "white_dye", n: 12 }], get: [{ id: "emerald", n: 1 }], uses: 16 },
  { p: "shepherd", lv: 2, want: [{ id: "gray_dye", n: 12 }], get: [{ id: "emerald", n: 1 }], uses: 16 },
  { p: "shepherd", lv: 2, want: [{ id: "black_dye", n: 12 }], get: [{ id: "emerald", n: 1 }], uses: 16 },
  { p: "shepherd", lv: 2, want: [{ id: "light_blue_dye", n: 12 }], get: [{ id: "emerald", n: 1 }], uses: 16 },
  { p: "shepherd", lv: 2, want: [{ id: "lime_dye", n: 12 }], get: [{ id: "emerald", n: 1 }], uses: 16 },
  { p: "shepherd", lv: 2, want: [{ id: "emerald", n: 1 }], get: [{ id: "white_wool", n: 1 }], uses: 16 },
  { p: "shepherd", lv: 2, want: [{ id: "emerald", n: 1 }], get: [{ id: "white_carpet", n: 4 }], uses: 16 },
  { p: "shepherd", lv: 3, want: [{ id: "yellow_dye", n: 12 }], get: [{ id: "emerald", n: 1 }], uses: 16 },
  { p: "shepherd", lv: 3, want: [{ id: "light_gray_dye", n: 12 }], get: [{ id: "emerald", n: 1 }], uses: 16 },
  { p: "shepherd", lv: 3, want: [{ id: "orange_dye", n: 12 }], get: [{ id: "emerald", n: 1 }], uses: 16 },
  { p: "shepherd", lv: 3, want: [{ id: "red_dye", n: 12 }], get: [{ id: "emerald", n: 1 }], uses: 16 },
  { p: "shepherd", lv: 3, want: [{ id: "pink_dye", n: 12 }], get: [{ id: "emerald", n: 1 }], uses: 16 },
  { p: "shepherd", lv: 3, want: [{ id: "emerald", n: 3 }], get: [{ id: "white_bed", n: 1 }], uses: 12 },
  { p: "shepherd", lv: 4, want: [{ id: "brown_dye", n: 12 }], get: [{ id: "emerald", n: 1 }], uses: 16 },
  { p: "shepherd", lv: 4, want: [{ id: "purple_dye", n: 12 }], get: [{ id: "emerald", n: 1 }], uses: 16 },
  { p: "shepherd", lv: 4, want: [{ id: "blue_dye", n: 12 }], get: [{ id: "emerald", n: 1 }], uses: 16 },
  { p: "shepherd", lv: 4, want: [{ id: "green_dye", n: 12 }], get: [{ id: "emerald", n: 1 }], uses: 16 },
  { p: "shepherd", lv: 4, want: [{ id: "magenta_dye", n: 12 }], get: [{ id: "emerald", n: 1 }], uses: 16 },
  { p: "shepherd", lv: 4, want: [{ id: "cyan_dye", n: 12 }], get: [{ id: "emerald", n: 1 }], uses: 16 },
  { p: "shepherd", lv: 4, want: [{ id: "emerald", n: 3 }], get: [{ id: "white_banner", n: 1 }], uses: 12 },
  { p: "shepherd", lv: 5, want: [{ id: "emerald", n: 2 }], get: [{ id: "painting", n: 3 }], uses: 12 },

  // ── toolsmith 工具匠 ──
  { p: "toolsmith", lv: 1, want: [{ id: "coal", n: 15 }], get: [{ id: "emerald", n: 1 }], uses: 16 },
  { p: "toolsmith", lv: 1, want: [{ id: "emerald", n: 1 }], get: [{ id: "stone_axe", n: 1 }], uses: 12 },
  { p: "toolsmith", lv: 1, want: [{ id: "emerald", n: 1 }], get: [{ id: "stone_shovel", n: 1 }], uses: 12 },
  { p: "toolsmith", lv: 1, want: [{ id: "emerald", n: 1 }], get: [{ id: "stone_pickaxe", n: 1 }], uses: 12 },
  { p: "toolsmith", lv: 1, want: [{ id: "emerald", n: 1 }], get: [{ id: "stone_hoe", n: 1 }], uses: 12 },
  { p: "toolsmith", lv: 2, want: [{ id: "iron_ingot", n: 4 }], get: [{ id: "emerald", n: 1 }], uses: 12 },
  { p: "toolsmith", lv: 2, want: [{ id: "emerald", n: 36 }], get: [{ id: "bell", n: 1 }], uses: 12 },
  { p: "toolsmith", lv: 3, want: [{ id: "flint", n: 30 }], get: [{ id: "emerald", n: 1 }], uses: 12 },
  { p: "toolsmith", lv: 3, want: [{ id: "emerald", n: 6 }], get: [{ id: "iron_axe", n: 1 }], uses: 3 },
  { p: "toolsmith", lv: 3, want: [{ id: "emerald", n: 7 }], get: [{ id: "iron_shovel", n: 1 }], uses: 3 },
  { p: "toolsmith", lv: 3, want: [{ id: "emerald", n: 8 }], get: [{ id: "iron_pickaxe", n: 1 }], uses: 3 },
  { p: "toolsmith", lv: 3, want: [{ id: "emerald", n: 4 }], get: [{ id: "diamond_hoe", n: 1 }], uses: 3 },
  { p: "toolsmith", lv: 4, want: [{ id: "diamond", n: 1 }], get: [{ id: "emerald", n: 1 }], uses: 12 },
  { p: "toolsmith", lv: 4, want: [{ id: "emerald", n: 17 }], get: [{ id: "diamond_axe", n: 1 }], uses: 3 },
  { p: "toolsmith", lv: 4, want: [{ id: "emerald", n: 10 }], get: [{ id: "diamond_shovel", n: 1 }], uses: 3 },
  { p: "toolsmith", lv: 5, want: [{ id: "emerald", n: 18 }], get: [{ id: "diamond_pickaxe", n: 1 }], uses: 3 },

  // ── weaponsmith 武器匠 ──
  { p: "weaponsmith", lv: 1, want: [{ id: "coal", n: 15 }], get: [{ id: "emerald", n: 1 }], uses: 16 },
  { p: "weaponsmith", lv: 1, want: [{ id: "emerald", n: 3 }], get: [{ id: "iron_axe", n: 1 }], uses: 12 },
  { p: "weaponsmith", lv: 1, want: [{ id: "emerald", n: 7 }], get: [{ id: "iron_sword", n: 1 }], uses: 12 },
  { p: "weaponsmith", lv: 2, want: [{ id: "iron_ingot", n: 4 }], get: [{ id: "emerald", n: 1 }], uses: 12 },
  { p: "weaponsmith", lv: 2, want: [{ id: "emerald", n: 36 }], get: [{ id: "bell", n: 1 }], uses: 12 },
  { p: "weaponsmith", lv: 3, want: [{ id: "flint", n: 24 }], get: [{ id: "emerald", n: 1 }], uses: 12 },
  { p: "weaponsmith", lv: 4, want: [{ id: "diamond", n: 1 }], get: [{ id: "emerald", n: 1 }], uses: 12 },
  { p: "weaponsmith", lv: 4, want: [{ id: "emerald", n: 17 }], get: [{ id: "diamond_axe", n: 1 }], uses: 3 },
  { p: "weaponsmith", lv: 5, want: [{ id: "emerald", n: 13 }], get: [{ id: "diamond_sword", n: 1 }], uses: 3 },
];
