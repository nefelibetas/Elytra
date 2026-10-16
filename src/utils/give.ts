// /give 命令生成器：物品数据表 + 双版本命令构建（modern = 1.20.5+ 组件；legacy = 1.13–1.20.4 NBT）；物品 ID 用 1.13+ 命名空间，不支持 1.12 及以下数字 ID。

import { normalizeNbtFragment } from "./nbtText";

export interface GiveItem {
  /** 无 minecraft: 前缀。 */
  id: string;
  name: string;
  cat: string;
  /** 引入主版本，缺省 13。 */
  since?: number;
}

export const GIVE_ITEMS: GiveItem[] = [
  // ── 剑 ──
  { id: "wooden_sword", name: "木剑", cat: "sword" },
  { id: "stone_sword", name: "石剑", cat: "sword" },
  { id: "iron_sword", name: "铁剑", cat: "sword" },
  { id: "golden_sword", name: "金剑", cat: "sword" },
  { id: "diamond_sword", name: "钻石剑", cat: "sword" },
  { id: "netherite_sword", name: "下界合金剑", cat: "sword" , since: 16},
  // ── 工具 ──
  { id: "wooden_pickaxe", name: "木镐", cat: "tool" },
  { id: "stone_pickaxe", name: "石镐", cat: "tool" },
  { id: "iron_pickaxe", name: "铁镐", cat: "tool" },
  { id: "golden_pickaxe", name: "金镐", cat: "tool" },
  { id: "diamond_pickaxe", name: "钻石镐", cat: "tool" },
  { id: "netherite_pickaxe", name: "下界合金镐", cat: "tool" , since: 16},
  { id: "wooden_axe", name: "木斧", cat: "tool" },
  { id: "stone_axe", name: "石斧", cat: "tool" },
  { id: "iron_axe", name: "铁斧", cat: "tool" },
  { id: "golden_axe", name: "金斧", cat: "tool" },
  { id: "diamond_axe", name: "钻石斧", cat: "tool" },
  { id: "netherite_axe", name: "下界合金斧", cat: "tool" , since: 16},
  { id: "wooden_shovel", name: "木锹", cat: "tool" },
  { id: "stone_shovel", name: "石锹", cat: "tool" },
  { id: "iron_shovel", name: "铁锹", cat: "tool" },
  { id: "golden_shovel", name: "金锹", cat: "tool" },
  { id: "diamond_shovel", name: "钻石锹", cat: "tool" },
  { id: "netherite_shovel", name: "下界合金锹", cat: "tool" , since: 16},
  { id: "wooden_hoe", name: "木锄", cat: "tool" },
  { id: "stone_hoe", name: "石锄", cat: "tool" },
  { id: "iron_hoe", name: "铁锄", cat: "tool" },
  { id: "golden_hoe", name: "金锄", cat: "tool" },
  { id: "diamond_hoe", name: "钻石锄", cat: "tool" },
  { id: "netherite_hoe", name: "下界合金锄", cat: "tool" , since: 16},
  { id: "bow", name: "弓", cat: "tool" },
  { id: "crossbow", name: "弩", cat: "tool" },
  { id: "trident", name: "三叉戟", cat: "tool" },
  { id: "shield", name: "盾牌", cat: "tool" },
  { id: "shears", name: "剪刀", cat: "tool" },
  { id: "fishing_rod", name: "钓鱼竿", cat: "tool" },
  { id: "flint_and_steel", name: "打火石", cat: "tool" },
  { id: "brush", name: "刷子", cat: "tool" , since: 20},
  { id: "spyglass", name: "望远镜", cat: "tool" , since: 17},
  { id: "compass", name: "指南针", cat: "tool" },
  { id: "recovery_compass", name: "回溯指南针", cat: "tool" , since: 19},
  { id: "clock", name: "时钟", cat: "tool" },
  { id: "bucket", name: "桶", cat: "tool" },
  { id: "water_bucket", name: "水桶", cat: "tool" },
  { id: "lava_bucket", name: "熔岩桶", cat: "tool" },
  { id: "milk_bucket", name: "奶桶", cat: "tool" },
  { id: "map", name: "地图", cat: "tool" },
  { id: "name_tag", name: "命名牌", cat: "tool" },
  { id: "lead", name: "拴绳", cat: "tool" },
  { id: "bone_meal", name: "骨粉", cat: "tool" },
  { id: "ender_eye", name: "末影之眼", cat: "tool" },
  { id: "totem_of_undying", name: "不死图腾", cat: "tool" },
  { id: "elytra", name: "鞘翅", cat: "tool" },
  { id: "firework_rocket", name: "烟花火箭", cat: "tool" },
  { id: "goat_horn", name: "山羊角", cat: "tool" , since: 19},
  // ── 盔甲 ──
  { id: "leather_helmet", name: "皮革帽子", cat: "armor" },
  { id: "leather_chestplate", name: "皮革外套", cat: "armor" },
  { id: "leather_leggings", name: "皮革裤子", cat: "armor" },
  { id: "leather_boots", name: "皮革靴子", cat: "armor" },
  { id: "chainmail_helmet", name: "锁链头盔", cat: "armor" },
  { id: "chainmail_chestplate", name: "锁链胸甲", cat: "armor" },
  { id: "chainmail_leggings", name: "锁链护腿", cat: "armor" },
  { id: "chainmail_boots", name: "锁链靴子", cat: "armor" },
  { id: "iron_helmet", name: "铁头盔", cat: "armor" },
  { id: "iron_chestplate", name: "铁胸甲", cat: "armor" },
  { id: "iron_leggings", name: "铁护腿", cat: "armor" },
  { id: "iron_boots", name: "铁靴子", cat: "armor" },
  { id: "golden_helmet", name: "金头盔", cat: "armor" },
  { id: "golden_chestplate", name: "金胸甲", cat: "armor" },
  { id: "golden_leggings", name: "金护腿", cat: "armor" },
  { id: "golden_boots", name: "金靴子", cat: "armor" },
  { id: "diamond_helmet", name: "钻石头盔", cat: "armor" },
  { id: "diamond_chestplate", name: "钻石胸甲", cat: "armor" },
  { id: "diamond_leggings", name: "钻石护腿", cat: "armor" },
  { id: "diamond_boots", name: "钻石靴子", cat: "armor" },
  { id: "netherite_helmet", name: "下界合金头盔", cat: "armor" , since: 16},
  { id: "netherite_chestplate", name: "下界合金胸甲", cat: "armor" , since: 16},
  { id: "netherite_leggings", name: "下界合金护腿", cat: "armor" , since: 16},
  { id: "netherite_boots", name: "下界合金靴子", cat: "armor" , since: 16},
  { id: "turtle_helmet", name: "海龟壳", cat: "armor" },
  // ── 食物 ──
  { id: "apple", name: "苹果", cat: "food" },
  { id: "golden_apple", name: "金苹果", cat: "food" },
  { id: "enchanted_golden_apple", name: "附魔金苹果", cat: "food" },
  { id: "bread", name: "面包", cat: "food" },
  { id: "cooked_beef", name: "牛排", cat: "food" },
  { id: "cooked_porkchop", name: "熟猪排", cat: "food" },
  { id: "cooked_chicken", name: "熟鸡肉", cat: "food" },
  { id: "cooked_mutton", name: "熟羊肉", cat: "food" },
  { id: "cooked_rabbit", name: "熟兔肉", cat: "food" },
  { id: "cooked_cod", name: "熟鳕鱼", cat: "food" },
  { id: "cooked_salmon", name: "熟鲑鱼", cat: "food" },
  { id: "baked_potato", name: "烤马铃薯", cat: "food" },
  { id: "carrot", name: "胡萝卜", cat: "food" },
  { id: "golden_carrot", name: "金胡萝卜", cat: "food" },
  { id: "sweet_berries", name: "甜浆果", cat: "food" },
  { id: "glow_berries", name: "发光浆果", cat: "food" },
  { id: "melon_slice", name: "西瓜片", cat: "food" },
  { id: "pumpkin_pie", name: "南瓜派", cat: "food" },
  { id: "cookie", name: "曲奇", cat: "food" },
  { id: "cake", name: "蛋糕", cat: "food" },
  { id: "honey_bottle", name: "蜂蜜瓶", cat: "food" },
  { id: "mushroom_stew", name: "蘑菇煲", cat: "food" },
  { id: "beetroot_soup", name: "甜菜汤", cat: "food" },
  { id: "chorus_fruit", name: "紫颂果", cat: "food" },
  { id: "dried_kelp", name: "干海带", cat: "food" },
  { id: "rabbit_stew", name: "兔肉煲", cat: "food" },
  // ── 材料 ──
  { id: "diamond", name: "钻石", cat: "material" },
  { id: "iron_ingot", name: "铁锭", cat: "material" },
  { id: "gold_ingot", name: "金锭", cat: "material" },
  { id: "copper_ingot", name: "铜锭", cat: "material" , since: 17},
  { id: "netherite_ingot", name: "下界合金锭", cat: "material" , since: 16},
  { id: "emerald", name: "绿宝石", cat: "material" },
  { id: "lapis_lazuli", name: "青金石", cat: "material" },
  { id: "redstone", name: "红石粉", cat: "material" },
  { id: "coal", name: "煤炭", cat: "material" },
  { id: "charcoal", name: "木炭", cat: "material" },
  { id: "quartz", name: "下界石英", cat: "material" },
  { id: "amethyst_shard", name: "紫水晶碎片", cat: "material" , since: 17},
  { id: "echo_shard", name: "回响碎片", cat: "material" , since: 19},
  { id: "blaze_rod", name: "烈焰棒", cat: "material" },
  { id: "blaze_powder", name: "烈焰粉", cat: "material" },
  { id: "ghast_tear", name: "恶魂之泪", cat: "material" },
  { id: "spider_eye", name: "蜘蛛眼", cat: "material" },
  { id: "fermented_spider_eye", name: "发酵蛛眼", cat: "material" },
  { id: "gunpowder", name: "火药", cat: "material" },
  { id: "string", name: "线", cat: "material" },
  { id: "feather", name: "羽毛", cat: "material" },
  { id: "bone", name: "骨头", cat: "material" },
  { id: "leather", name: "皮革", cat: "material" },
  { id: "rabbit_hide", name: "兔子皮", cat: "material" },
  { id: "rabbit_foot", name: "兔子脚", cat: "material" },
  { id: "ink_sac", name: "墨囊", cat: "material" },
  { id: "glow_ink_sac", name: "发光墨囊", cat: "material" , since: 17},
  { id: "slime_ball", name: "粘液球", cat: "material" },
  { id: "magma_cream", name: "岩浆膏", cat: "material" },
  { id: "phantom_membrane", name: "幻翼膜", cat: "material" },
  { id: "prismarine_shard", name: "海晶碎片", cat: "material" },
  { id: "prismarine_crystals", name: "海晶砂粒", cat: "material" },
  { id: "nautilus_shell", name: "鹦鹉螺壳", cat: "material" },
  { id: "heart_of_the_sea", name: "海洋之心", cat: "material" },
  { id: "nether_star", name: "下界之星", cat: "material" },
  { id: "shulker_shell", name: "潜影壳", cat: "material" },
  { id: "dragon_breath", name: "龙息", cat: "material" },
  { id: "ender_pearl", name: "末影珍珠", cat: "material" },
  { id: "paper", name: "纸", cat: "material" },
  { id: "book", name: "书", cat: "material" },
  { id: "writable_book", name: "书与笔", cat: "material" },
  { id: "honeycomb", name: "蜜脾", cat: "material" },
  { id: "scute", name: "鳞甲", cat: "material" },
  // ── 矿物块 ──
  { id: "diamond_block", name: "钻石块", cat: "block" },
  { id: "iron_block", name: "铁块", cat: "block" },
  { id: "gold_block", name: "金块", cat: "block" },
  { id: "copper_block", name: "铜块", cat: "block" , since: 17},
  { id: "netherite_block", name: "下界合金块", cat: "block" , since: 16},
  { id: "emerald_block", name: "绿宝石块", cat: "block" },
  { id: "lapis_block", name: "青金石块", cat: "block" },
  { id: "redstone_block", name: "红石块", cat: "block" },
  { id: "coal_block", name: "煤炭块", cat: "block" },
  { id: "quartz_block", name: "石英块", cat: "block" },
  { id: "amethyst_block", name: "紫水晶块", cat: "block" , since: 17},
  { id: "raw_iron_block", name: "粗铁块", cat: "block" , since: 17},
  { id: "raw_gold_block", name: "粗金块", cat: "block" , since: 17},
  { id: "raw_copper_block", name: "粗铜块", cat: "block" , since: 17},
  // ── 红石 ──
  { id: "redstone_repeater", name: "红石中继器", cat: "redstone" },
  { id: "redstone_comparator", name: "红石比较器", cat: "redstone" },
  { id: "piston", name: "活塞", cat: "redstone" },
  { id: "sticky_piston", name: "粘性活塞", cat: "redstone" },
  { id: "observer", name: "侦测器", cat: "redstone" },
  { id: "hopper", name: "漏斗", cat: "redstone" },
  { id: "dispenser", name: "发射器", cat: "redstone" },
  { id: "dropper", name: "投掷器", cat: "redstone" },
  { id: "note_block", name: "音符盒", cat: "redstone" },
  { id: "lever", name: "拉杆", cat: "redstone" },
  { id: "redstone_lamp", name: "红石灯", cat: "redstone" },
  { id: "target", name: "标靶", cat: "redstone" , since: 16},
  { id: "slime_block", name: "粘液块", cat: "redstone" },
  { id: "honey_block", name: "蜜块", cat: "redstone" },
  { id: "daylight_detector", name: "阳光探测器", cat: "redstone" },
  { id: "tripwire_hook", name: "绊线钩", cat: "redstone" },
  { id: "trapped_chest", name: "陷阱箱", cat: "redstone" },
  { id: "jukebox", name: "唱片机", cat: "redstone" },
  { id: "sculk_sensor", name: "潜声传感器", cat: "redstone" , since: 19},
  { id: "calibrated_sculk_sensor", name: "校定潜声传感器", cat: "redstone" , since: 19},
  // ── 建筑 ──
  { id: "oak_planks", name: "橡木木板", cat: "building" },
  { id: "spruce_planks", name: "云杉木板", cat: "building" },
  { id: "birch_planks", name: "白桦木板", cat: "building" },
  { id: "jungle_planks", name: "丛林木板", cat: "building" },
  { id: "acacia_planks", name: "金合欢木板", cat: "building" },
  { id: "dark_oak_planks", name: "深色橡木木板", cat: "building" },
  { id: "mangrove_planks", name: "红树木板", cat: "building" , since: 19},
  { id: "cherry_planks", name: "樱花木板", cat: "building" , since: 20},
  { id: "bamboo_planks", name: "竹木板", cat: "building" , since: 20},
  { id: "crimson_planks", name: "绯红木板", cat: "building" , since: 16},
  { id: "warped_planks", name: "诡异木板", cat: "building" , since: 16},
  { id: "stone", name: "石头", cat: "building" },
  { id: "cobblestone", name: "圆石", cat: "building" },
  { id: "stone_bricks", name: "石砖", cat: "building" },
  { id: "mossy_stone_bricks", name: "苔石砖", cat: "building" },
  { id: "deepslate_bricks", name: "深板岩砖", cat: "building" , since: 17},
  { id: "polished_andesite", name: "磨制安山岩", cat: "building" },
  { id: "granite", name: "花岗岩", cat: "building" },
  { id: "diorite", name: "闪长岩", cat: "building" },
  { id: "sandstone", name: "砂岩", cat: "building" },
  { id: "red_sandstone", name: "红砂岩", cat: "building" },
  { id: "bricks", name: "红砖块", cat: "building" },
  { id: "glass", name: "玻璃", cat: "building" },
  { id: "glass_pane", name: "玻璃板", cat: "building" },
  { id: "white_wool", name: "白色羊毛", cat: "building" },
  { id: "black_wool", name: "黑色羊毛", cat: "building" },
  { id: "white_concrete", name: "白色混凝土", cat: "building" },
  { id: "black_concrete", name: "黑色混凝土", cat: "building" },
  { id: "white_terracotta", name: "白色陶瓦", cat: "building" },
  { id: "oak_log", name: "橡木原木", cat: "building" },
  { id: "oak_fence", name: "橡木栅栏", cat: "building" },
  { id: "oak_door", name: "橡木门", cat: "building" },
  { id: "oak_stairs", name: "橡木楼梯", cat: "building" },
  { id: "oak_slab", name: "橡木台阶", cat: "building" },
  { id: "bookshelf", name: "书架", cat: "building" },
  { id: "enchanting_table", name: "附魔台", cat: "building" },
  { id: "anvil", name: "铁砧", cat: "building" },
  { id: "grindstone", name: "砂轮", cat: "building" },
  { id: "crafting_table", name: "工作台", cat: "building" },
  { id: "furnace", name: "熔炉", cat: "building" },
  { id: "blast_furnace", name: "高炉", cat: "building" },
  { id: "smoker", name: "烟熏炉", cat: "building" },
  { id: "beacon", name: "信标", cat: "building" },
  { id: "chest", name: "箱子", cat: "building" },
  { id: "ender_chest", name: "末影箱", cat: "building" },
  { id: "barrel", name: "木桶", cat: "building" },
  { id: "shulker_box", name: "潜影盒", cat: "building" },
  { id: "ladder", name: "梯子", cat: "building" },
  { id: "torch", name: "火把", cat: "building" },
  { id: "lantern", name: "灯笼", cat: "building" },
  { id: "soul_lantern", name: "灵魂灯笼", cat: "building" , since: 16},
  { id: "campfire", name: "营火", cat: "building" },
  { id: "bed", name: "床", cat: "building" },
  { id: "respawn_anchor", name: "重生锚", cat: "building" , since: 16},
  // ── 杂项 ──
  { id: "experience_bottle", name: "附魔之瓶", cat: "misc" },
  { id: "potion", name: "药水", cat: "misc" },
  { id: "splash_potion", name: "喷溅药水", cat: "misc" },
  { id: "lingering_potion", name: "滞留药水", cat: "misc" },
  { id: "tipped_arrow", name: "药水箭", cat: "misc" },
  { id: "arrow", name: "箭", cat: "misc" },
  { id: "spectral_arrow", name: "光谱之箭", cat: "misc" },
  { id: "snowball", name: "雪球", cat: "misc" },
  { id: "player_head", name: "玩家头颅", cat: "misc" },
  { id: "zombie_head", name: "僵尸头颅", cat: "misc" },
  { id: "creeper_head", name: "苦力怕头颅", cat: "misc" },
  { id: "skeleton_skull", name: "骷髅头颅", cat: "misc" },
  { id: "wither_skeleton_skull", name: "凋灵骷髅头颅", cat: "misc" },
  { id: "dragon_head", name: "龙首", cat: "misc" },
  { id: "music_disc_13", name: "音乐唱片 13", cat: "misc" },
  { id: "music_disc_cat", name: "音乐唱片 Cat", cat: "misc" },
  { id: "music_disc_otherside", name: "音乐唱片 Otherside", cat: "misc" , since: 18},
  { id: "debug_stick", name: "调试棒", cat: "misc" },
  { id: "knowledge_book", name: "知识之书", cat: "misc" },
  { id: "stick", name: "木棍", cat: "misc" },
  { id: "bowl", name: "碗", cat: "misc" },
  { id: "flint", name: "燧石", cat: "misc" },
  // ── 村民交易常用 ──
  { id: "wheat", name: "小麦", cat: "material" },
  { id: "potato", name: "马铃薯", cat: "food" },
  { id: "beetroot", name: "甜菜根", cat: "food" },
  { id: "pumpkin", name: "南瓜", cat: "block" },
  { id: "melon", name: "西瓜", cat: "block" },
  { id: "rotten_flesh", name: "腐肉", cat: "food" },
  { id: "beef", name: "生牛肉", cat: "food" },
  { id: "porkchop", name: "生猪排", cat: "food" },
  { id: "chicken", name: "生鸡肉", cat: "food" },
  { id: "mutton", name: "生羊肉", cat: "food" },
  { id: "rabbit", name: "生兔肉", cat: "food" },
  { id: "cod", name: "生鳕鱼", cat: "food" },
  { id: "salmon", name: "生鲑鱼", cat: "food" },
  { id: "tropical_fish", name: "热带鱼", cat: "food" },
  { id: "pufferfish", name: "河豚", cat: "food" },
  { id: "suspicious_stew", name: "迷之炖菜", cat: "food" },
  { id: "glistering_melon_slice", name: "闪烁的西瓜片", cat: "material" },
  { id: "nether_wart", name: "下界疣", cat: "material" },
  { id: "clay_ball", name: "黏土球", cat: "material" },
  { id: "brick", name: "红砖", cat: "material" },
  { id: "gravel", name: "砂砾", cat: "block" },
  { id: "andesite", name: "安山岩", cat: "building" },
  { id: "polished_andesite", name: "磨制安山岩", cat: "building" },
  { id: "polished_diorite", name: "磨制闪长岩", cat: "building" },
  { id: "polished_granite", name: "磨制花岗岩", cat: "building" },
  { id: "chiseled_stone_bricks", name: "錾制石砖", cat: "building" },
  { id: "dripstone_block", name: "滴水石块", cat: "building" },
  { id: "quartz_pillar", name: "石英柱", cat: "building" },
  { id: "glowstone", name: "荧石", cat: "block" },
  { id: "dried_kelp_block", name: "干海带块", cat: "block" },
  { id: "bell", name: "钟", cat: "misc" },
  { id: "saddle", name: "鞍", cat: "misc" },
  { id: "enchanted_book", name: "附魔书", cat: "misc" },
  { id: "filled_map", name: "已填充地图", cat: "misc" },
  { id: "item_frame", name: "物品展示框", cat: "misc" },
  { id: "painting", name: "画", cat: "misc" },
  { id: "glass_bottle", name: "玻璃瓶", cat: "misc" },
  { id: "oak_boat", name: "橡木船", cat: "misc" },
  { id: "cod_bucket", name: "鳕鱼桶", cat: "misc" },
  { id: "turtle_scute", name: "鳞甲", cat: "material" },
  { id: "leather_horse_armor", name: "皮革马铠", cat: "misc" },
  { id: "globe_banner_pattern", name: "地球旗帜图案", cat: "misc" , since: 19},
  { id: "brown_wool", name: "棕色羊毛", cat: "building" },
  { id: "gray_wool", name: "灰色羊毛", cat: "building" },
  { id: "white_wool", name: "白色羊毛", cat: "building" },
  { id: "white_carpet", name: "白色地毯", cat: "building" },
  { id: "white_bed", name: "白色床", cat: "misc" },
  { id: "white_banner", name: "白色旗帜", cat: "misc" },
  { id: "white_terracotta", name: "白色陶瓦", cat: "building" },
  { id: "white_glazed_terracotta", name: "白色带釉陶瓦", cat: "building" },
  { id: "red_candle", name: "红色蜡烛", cat: "misc" , since: 17},
  { id: "yellow_candle", name: "黄色蜡烛", cat: "misc" , since: 17},
  { id: "white_dye", name: "白色染料", cat: "material" },
  { id: "gray_dye", name: "灰色染料", cat: "material" },
  { id: "light_gray_dye", name: "淡灰色染料", cat: "material" },
  { id: "black_dye", name: "黑色染料", cat: "material" },
  { id: "brown_dye", name: "棕色染料", cat: "material" },
  { id: "red_dye", name: "红色染料", cat: "material" },
  { id: "orange_dye", name: "橙色染料", cat: "material" },
  { id: "yellow_dye", name: "黄色染料", cat: "material" },
  { id: "lime_dye", name: "黄绿色染料", cat: "material" },
  { id: "green_dye", name: "绿色染料", cat: "material" },
  { id: "cyan_dye", name: "青色染料", cat: "material" },
  { id: "light_blue_dye", name: "淡蓝色染料", cat: "material" },
  { id: "blue_dye", name: "蓝色染料", cat: "material" },
  { id: "purple_dye", name: "紫色染料", cat: "material" },
  { id: "magenta_dye", name: "品红色染料", cat: "material" },
  { id: "pink_dye", name: "粉红色染料", cat: "material" },
];

export interface GiveEnchant {
  id: string;
  name: string;
  max: number;
  /** 引入主版本，缺省 13。 */
  since?: number;
}

export function filterEnchantsByVersion(enchs: GiveEnchant[], format: GiveFormat): GiveEnchant[] {
  const max = format === "legacy" ? 20 : 21;
  return enchs.filter((e) => (e.since ?? 13) <= max);
}

/** 1.13+ 命名空间 ID。 */
export const ENCHANTMENTS: GiveEnchant[] = [
  { id: "sharpness", name: "锋利", max: 5 },
  { id: "smite", name: "亡灵杀手", max: 5 },
  { id: "bane_of_arthropods", name: "节肢杀手", max: 5 },
  { id: "knockback", name: "击退", max: 2 },
  { id: "fire_aspect", name: "火焰附加", max: 2 },
  { id: "looting", name: "抢夺", max: 3 },
  { id: "sweeping_edge", name: "横扫之刃", max: 3 },
  { id: "efficiency", name: "效率", max: 5 },
  { id: "silk_touch", name: "精准采集", max: 1 },
  { id: "fortune", name: "时运", max: 3 },
  { id: "protection", name: "保护", max: 4 },
  { id: "fire_protection", name: "火焰保护", max: 4 },
  { id: "blast_protection", name: "爆炸保护", max: 4 },
  { id: "projectile_protection", name: "弹射物保护", max: 4 },
  { id: "respiration", name: "水下呼吸", max: 3 },
  { id: "aqua_affinity", name: "水下速掘", max: 1 },
  { id: "thorns", name: "荆棘", max: 3 },
  { id: "depth_strider", name: "深海探索者", max: 3 },
  { id: "frost_walker", name: "冰霜行者", max: 2 },
  { id: "soul_speed", name: "灵魂疾行", max: 3 , since: 16},
  { id: "swift_sneak", name: "迅捷潜行", max: 3 , since: 19},
  { id: "feather_falling", name: "摔落缓冲", max: 4 },
  { id: "punch", name: "冲击", max: 2 },
  { id: "power", name: "力量", max: 5 },
  { id: "flame", name: "火焰", max: 1 },
  { id: "infinity", name: "无限", max: 1 },
  { id: "luck_of_the_sea", name: "海之眷顾", max: 3 },
  { id: "lure", name: "饵钓", max: 3 },
  { id: "mending", name: "经验修补", max: 1 },
  { id: "unbreaking", name: "耐久", max: 3 },
  { id: "curse_of_binding", name: "绑定诅咒", max: 1 },
  { id: "curse_of_vanishing", name: "消失诅咒", max: 1 },
  { id: "multishot", name: "多重射击", max: 1 },
  { id: "piercing", name: "穿透", max: 4 },
  { id: "quick_charge", name: "快速装填", max: 3 },
  { id: "channeling", name: "引雷", max: 1 },
  { id: "riptide", name: "激流", max: 3 },
  { id: "loyalty", name: "忠诚", max: 3 },
  { id: "impaling", name: "穿刺", max: 5 },
  { id: "density", name: "致密", max: 5 , since: 21},
  { id: "breach", name: "破甲", max: 4 , since: 21},
  { id: "wind_burst", name: "风爆", max: 3 , since: 21},
];

/** amount 为附加量，operation 固定相加。 */
export interface GiveAttribute {
  id: string;
  name: string;
}

export const ATTRIBUTES: GiveAttribute[] = [
  { id: "generic.attack_damage", name: "攻击伤害" },
  { id: "generic.attack_speed", name: "攻击速度" },
  { id: "generic.armor", name: "护甲值" },
  { id: "generic.armor_toughness", name: "护甲韧性" },
  { id: "generic.max_health", name: "最大生命值" },
  { id: "generic.movement_speed", name: "移动速度" },
  { id: "generic.knockback_resistance", name: "击退抗性" },
  { id: "generic.luck", name: "幸运" },
  { id: "generic.follow_range", name: "跟随范围" },
  { id: "generic.scale", name: "体型" },
  { id: "generic.step_height", name: "步高" },
  { id: "generic.gravity", name: "重力" },
  { id: "generic.safe_fall_distance", name: "安全摔落距离" },
];

/** modern = 1.20.5+ 组件；legacy = 1.13–1.20.4 NBT。 */
export type GiveFormat = "modern" | "legacy";

export const GIVE_FORMAT_OPTIONS: { value: GiveFormat; label: string }[] = [
  { value: "modern", label: "1.20.5+（组件格式）" },
  { value: "legacy", label: "1.13–1.20.4（NBT 格式）" },
];

export interface EnchantInput {
  id: string;
  level: number;
}

export interface AttributeInput {
  id: string;
  amount: number;
}

/** give 命令与交易物品槽共用。 */
export interface ItemMods {
  colorCode: string;
  customName: string;
  lore: string[];
  unbreakable: boolean;
  hideTooltip: boolean;
  enchantments: EnchantInput[];
  attributes: AttributeInput[];
  /** 自由附加内容（原样透传，不解析）：按 /give 语法的附加组件（modern 形如 `custom_model_data=7`）。 */
  rawModern: string;
  /** 自由附加内容（原样透传，不解析）：legacy NBT 标签（形如 `CustomModelData:7`）。 */
  rawLegacy: string;
}

export const EMPTY_MODS: ItemMods = {
  colorCode: "",
  customName: "",
  lore: [],
  unbreakable: false,
  hideTooltip: false,
  enchantments: [],
  attributes: [],
  rawModern: "",
  rawLegacy: "",
};

function jsonStr(s: string): string {
  return s.replace(/\\/g, "\\\\").replace(/"/g, '\\"');
}

/** key 为 give 简写名，如 enchantments。 */
export interface ModernComponent {
  key: string;
  value: string;
}

/** give 命令与交易物品槽 components 共用。`forBook`：附魔书用 stored_enchantments（普通 enchantments 在附魔书上无效）。 */
export function modernItemComponents(mods: ItemMods, forBook = false): ModernComponent[] {
  const parts: ModernComponent[] = [];
  if (mods.enchantments.length > 0) {
    const levels = mods.enchantments
      .map((e) => `"minecraft:${e.id}":${clampLevel(e.level)}`)
      .join(",");
    parts.push({ key: forBook ? "stored_enchantments" : "enchantments", value: `{levels:{${levels}}}` });
  }
  if (mods.unbreakable) {
    parts.push({ key: "unbreakable", value: "{}" });
  }
  const name = mods.customName.trim() || "";
  if (name || mods.colorCode) {
    parts.push({ key: "custom_name", value: `'"${jsonStr(`${mods.colorCode}${name}`)}"'` });
  }
  const lore = mods.lore.map((l) => l.trim()).filter((l) => l.length > 0);
  if (lore.length > 0) {
    parts.push({ key: "lore", value: `[${lore.map((l) => `'"${jsonStr(l)}"'`).join(",")}]` });
  }
  if (mods.hideTooltip) {
    parts.push({ key: "hide_additional_tooltip", value: "{}" });
  }
  if (mods.attributes.length > 0) {
    // 1.21+ 属性组件带 minecraft: 前缀
    const attrs = mods.attributes
      .map((a) => `{type:"minecraft:${a.id}",amount:${a.amount},operation:"add_value"}`)
      .join(",");
    parts.push({ key: "attribute_modifiers", value: `[${attrs}]` });
  }
  return parts;
}

/** give 命令与交易物品槽共用。`forBook`：附魔书用 StoredEnchantments。 */
export function legacyItemTags(mods: ItemMods, forBook = false): string[] {
  const tags: string[] = [];
  if (mods.enchantments.length > 0) {
    const list = mods.enchantments
      .map((e) => `{id:"minecraft:${e.id}",lvl:${clampLevel(e.level)}}`)
      .join(",");
    tags.push(`${forBook ? "StoredEnchantments" : "Enchantments"}:[${list}]`);
  }
  if (mods.unbreakable) {
    tags.push("Unbreakable:1b");
  }
  const name = mods.customName.trim() || "";
  const lore = mods.lore.map((l) => l.trim()).filter((l) => l.length > 0);
  if (name || mods.colorCode || lore.length > 0) {
    const displayParts: string[] = [];
    if (name || mods.colorCode) {
      const text = `${mods.colorCode}${name}`;
      // 颜色以 § 码置于 text（客户端渲染 JSON 文本时支持 §）
      displayParts.push(`Name:'{"text":"${jsonStr(text)}"}'`);
    }
    if (lore.length > 0) {
      displayParts.push(`Lore:[${lore.map((l) => `'{"text":"${jsonStr(l)}"}'`).join(",")}]`);
    }
    tags.push(`display:{${displayParts.join(",")}}`);
  }
  if (mods.hideTooltip) {
    // 32 = 隐藏附加信息（附魔/属性/耐久）；63 = 全部
    tags.push("HideFlags:32");
  }
  if (mods.attributes.length > 0) {
    // 1.16+ AttributeModifiers 需要 UUID（相同 UUID 不叠加，随机生成避免覆盖）
    const attrs = mods.attributes
      .map((a) => {
        const uuid = crypto.randomUUID();
        const camel = toCamelCase(a.id);
        return `{AttributeName:"${camel}",Name:"${camel}",Amount:${a.amount},Operation:0,Slot:"mainhand",UUID:[I;${uuidToIntArray(uuid)}]}`;
      })
      .join(",");
    tags.push(`AttributeModifiers:[${attrs}]`);
  }
  return tags;
}

/** 附魔等级 clamp（1–255）。 */
function clampLevel(level: number): number {
  return Math.max(1, Math.min(level, 255));
}

/** 交易物品槽：modern `{id,count,components:{...}}` / legacy `{id,Count,<NBT 标签>}`，修饰与 give 共用构建。
 *  `raw` 为槽级自由内容（trade 语法）：以 `{` 开头按完整物品栈原样覆盖；否则作为附加内容并入 components/tag。 */
export function buildTradeItem(
  id: string,
  count: number,
  mods: ItemMods,
  format: GiveFormat,
  raw?: string,
): string {
  const c = Math.max(1, Math.min(64, Math.floor(count) || 1));
  // 完整物品栈（用户自带 id/Count/tag 或 id/count/components）：判别先于归一化，原样透传
  const rawTrimmed = (raw ?? "").trim();
  if (rawTrimmed.startsWith("{")) return rawTrimmed;
  const extra = normalizeNbtFragment(rawTrimmed);
  // 附魔书：附魔落在 StoredEnchantments / stored_enchantments
  const forBook = id === "enchanted_book";
  if (format === "modern") {
    const comps = modernItemComponents(mods, forBook).map((p) => `"minecraft:${p.key}":${p.value}`);
    const rawSelf = normalizeNbtFragment(mods.rawModern);
    if (rawSelf) comps.push(rawSelf);
    if (extra) comps.push(extra);
    const compsStr = comps.length > 0 ? `,components:{${comps.join(",")}}` : "";
    return `{id:"minecraft:${id}",count:${c}${compsStr}}`;
  }
  const tags = legacyItemTags(mods, forBook);
  const rawSelf = normalizeNbtFragment(mods.rawLegacy);
  if (rawSelf) tags.push(rawSelf);
  if (extra) tags.push(extra);
  const tagsStr = tags.length > 0 ? `,${tags.join(",")}` : "";
  return `{id:"minecraft:${id}",Count:${c}b${tagsStr}}`;
}

/** 属性 ID → legacy NBT camelCase（generic.attack_damage → generic.attackDamage）。 */
function toCamelCase(id: string): string {
  return id
    .split(".")
    .map((seg) => seg.replace(/_([a-z])/g, (_, c: string) => c.toUpperCase()))
    .join(".");
}

/** UUID（hex）→ NBT [I;] 四段十进制 i32。 */
function uuidToIntArray(uuid: string): string {
  const hex = uuid.replace(/-/g, "");
  const parts: number[] = [];
  for (let i = 0; i < 4; i++) {
    const seg = hex.slice(i * 8, i * 8 + 8);
    parts.push(parseInt(seg, 16) | 0);
  }
  return parts.join(",");
}

