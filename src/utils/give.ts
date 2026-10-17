// /give 命令生成器：物品数据表 + 版本区间构建（1.7.10–1.20.4 为 NBT，1.20.5+ 为组件；1.13 前用数字附魔 ID/扁平化前物品 ID，1.16+ 属性改用注册名）；物品表用 1.13+ 命名空间，输出时按版本映射。

import { normalizeNbtFragment } from "./nbtText";

export interface GiveItem {
  /** 无 minecraft: 前缀。 */
  id: string;
  name: string;
  cat: string;
}

export const GIVE_ITEMS: GiveItem[] = [
  // ── 剑 ──
  { id: "wooden_sword", name: "木剑", cat: "sword" },
  { id: "stone_sword", name: "石剑", cat: "sword" },
  { id: "iron_sword", name: "铁剑", cat: "sword" },
  { id: "golden_sword", name: "金剑", cat: "sword" },
  { id: "diamond_sword", name: "钻石剑", cat: "sword" },
  { id: "netherite_sword", name: "下界合金剑", cat: "sword" },
  // ── 工具 ──
  { id: "wooden_pickaxe", name: "木镐", cat: "tool" },
  { id: "stone_pickaxe", name: "石镐", cat: "tool" },
  { id: "iron_pickaxe", name: "铁镐", cat: "tool" },
  { id: "golden_pickaxe", name: "金镐", cat: "tool" },
  { id: "diamond_pickaxe", name: "钻石镐", cat: "tool" },
  { id: "netherite_pickaxe", name: "下界合金镐", cat: "tool" },
  { id: "wooden_axe", name: "木斧", cat: "tool" },
  { id: "stone_axe", name: "石斧", cat: "tool" },
  { id: "iron_axe", name: "铁斧", cat: "tool" },
  { id: "golden_axe", name: "金斧", cat: "tool" },
  { id: "diamond_axe", name: "钻石斧", cat: "tool" },
  { id: "netherite_axe", name: "下界合金斧", cat: "tool" },
  { id: "wooden_shovel", name: "木锹", cat: "tool" },
  { id: "stone_shovel", name: "石锹", cat: "tool" },
  { id: "iron_shovel", name: "铁锹", cat: "tool" },
  { id: "golden_shovel", name: "金锹", cat: "tool" },
  { id: "diamond_shovel", name: "钻石锹", cat: "tool" },
  { id: "netherite_shovel", name: "下界合金锹", cat: "tool" },
  { id: "wooden_hoe", name: "木锄", cat: "tool" },
  { id: "stone_hoe", name: "石锄", cat: "tool" },
  { id: "iron_hoe", name: "铁锄", cat: "tool" },
  { id: "golden_hoe", name: "金锄", cat: "tool" },
  { id: "diamond_hoe", name: "钻石锄", cat: "tool" },
  { id: "netherite_hoe", name: "下界合金锄", cat: "tool" },
  { id: "bow", name: "弓", cat: "tool" },
  { id: "crossbow", name: "弩", cat: "tool" },
  { id: "trident", name: "三叉戟", cat: "tool" },
  { id: "shield", name: "盾牌", cat: "tool" },
  { id: "shears", name: "剪刀", cat: "tool" },
  { id: "fishing_rod", name: "钓鱼竿", cat: "tool" },
  { id: "flint_and_steel", name: "打火石", cat: "tool" },
  { id: "brush", name: "刷子", cat: "tool" },
  { id: "spyglass", name: "望远镜", cat: "tool" },
  { id: "compass", name: "指南针", cat: "tool" },
  { id: "recovery_compass", name: "回溯指南针", cat: "tool" },
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
  { id: "goat_horn", name: "山羊角", cat: "tool" },
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
  { id: "netherite_helmet", name: "下界合金头盔", cat: "armor" },
  { id: "netherite_chestplate", name: "下界合金胸甲", cat: "armor" },
  { id: "netherite_leggings", name: "下界合金护腿", cat: "armor" },
  { id: "netherite_boots", name: "下界合金靴子", cat: "armor" },
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
  { id: "copper_ingot", name: "铜锭", cat: "material" },
  { id: "netherite_ingot", name: "下界合金锭", cat: "material" },
  { id: "emerald", name: "绿宝石", cat: "material" },
  { id: "lapis_lazuli", name: "青金石", cat: "material" },
  { id: "redstone", name: "红石粉", cat: "material" },
  { id: "coal", name: "煤炭", cat: "material" },
  { id: "charcoal", name: "木炭", cat: "material" },
  { id: "quartz", name: "下界石英", cat: "material" },
  { id: "amethyst_shard", name: "紫水晶碎片", cat: "material" },
  { id: "echo_shard", name: "回响碎片", cat: "material" },
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
  { id: "glow_ink_sac", name: "发光墨囊", cat: "material" },
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
  { id: "copper_block", name: "铜块", cat: "block" },
  { id: "netherite_block", name: "下界合金块", cat: "block" },
  { id: "emerald_block", name: "绿宝石块", cat: "block" },
  { id: "lapis_block", name: "青金石块", cat: "block" },
  { id: "redstone_block", name: "红石块", cat: "block" },
  { id: "coal_block", name: "煤炭块", cat: "block" },
  { id: "quartz_block", name: "石英块", cat: "block" },
  { id: "amethyst_block", name: "紫水晶块", cat: "block" },
  { id: "raw_iron_block", name: "粗铁块", cat: "block" },
  { id: "raw_gold_block", name: "粗金块", cat: "block" },
  { id: "raw_copper_block", name: "粗铜块", cat: "block" },
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
  { id: "target", name: "标靶", cat: "redstone" },
  { id: "slime_block", name: "粘液块", cat: "redstone" },
  { id: "honey_block", name: "蜜块", cat: "redstone" },
  { id: "daylight_detector", name: "阳光探测器", cat: "redstone" },
  { id: "tripwire_hook", name: "绊线钩", cat: "redstone" },
  { id: "trapped_chest", name: "陷阱箱", cat: "redstone" },
  { id: "jukebox", name: "唱片机", cat: "redstone" },
  { id: "sculk_sensor", name: "潜声传感器", cat: "redstone" },
  { id: "calibrated_sculk_sensor", name: "校定潜声传感器", cat: "redstone" },
  // ── 建筑 ──
  { id: "oak_planks", name: "橡木木板", cat: "building" },
  { id: "spruce_planks", name: "云杉木板", cat: "building" },
  { id: "birch_planks", name: "白桦木板", cat: "building" },
  { id: "jungle_planks", name: "丛林木板", cat: "building" },
  { id: "acacia_planks", name: "金合欢木板", cat: "building" },
  { id: "dark_oak_planks", name: "深色橡木木板", cat: "building" },
  { id: "mangrove_planks", name: "红树木板", cat: "building" },
  { id: "cherry_planks", name: "樱花木板", cat: "building" },
  { id: "bamboo_planks", name: "竹木板", cat: "building" },
  { id: "crimson_planks", name: "绯红木板", cat: "building" },
  { id: "warped_planks", name: "诡异木板", cat: "building" },
  { id: "stone", name: "石头", cat: "building" },
  { id: "cobblestone", name: "圆石", cat: "building" },
  { id: "stone_bricks", name: "石砖", cat: "building" },
  { id: "mossy_stone_bricks", name: "苔石砖", cat: "building" },
  { id: "deepslate_bricks", name: "深板岩砖", cat: "building" },
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
  { id: "soul_lantern", name: "灵魂灯笼", cat: "building" },
  { id: "campfire", name: "营火", cat: "building" },
  { id: "bed", name: "床", cat: "building" },
  { id: "respawn_anchor", name: "重生锚", cat: "building" },
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
  { id: "music_disc_otherside", name: "音乐唱片 Otherside", cat: "misc" },
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
  { id: "globe_banner_pattern", name: "地球旗帜图案", cat: "misc" },
  { id: "brown_wool", name: "棕色羊毛", cat: "building" },
  { id: "gray_wool", name: "灰色羊毛", cat: "building" },
  { id: "white_wool", name: "白色羊毛", cat: "building" },
  { id: "white_carpet", name: "白色地毯", cat: "building" },
  { id: "white_bed", name: "白色床", cat: "misc" },
  { id: "white_banner", name: "白色旗帜", cat: "misc" },
  { id: "white_terracotta", name: "白色陶瓦", cat: "building" },
  { id: "white_glazed_terracotta", name: "白色带釉陶瓦", cat: "building" },
  { id: "red_candle", name: "红色蜡烛", cat: "misc" },
  { id: "yellow_candle", name: "黄色蜡烛", cat: "misc" },
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

/** 1.13 扁平化 ID → 1.13 前物品 ID/Damage（仅列出改名/变种；同名的自动沿用）。 */
const LEGACY_ITEM_REFS: Record<string, { id: string; damage?: number }> = {
  enchanted_golden_apple: { id: "golden_apple", damage: 1 },
  cooked_cod: { id: "cooked_fish" },
  cooked_salmon: { id: "cooked_fish", damage: 1 },
  cod: { id: "fish" },
  salmon: { id: "fish", damage: 1 },
  tropical_fish: { id: "fish", damage: 2 },
  pufferfish: { id: "fish", damage: 3 },
  lapis_lazuli: { id: "dye", damage: 4 },
  charcoal: { id: "coal", damage: 1 },
  ink_sac: { id: "dye" },
  bone_meal: { id: "dye", damage: 15 },
  glistering_melon_slice: { id: "speckled_melon" },
  white_dye: { id: "dye", damage: 15 },
  orange_dye: { id: "dye", damage: 14 },
  magenta_dye: { id: "dye", damage: 13 },
  light_blue_dye: { id: "dye", damage: 12 },
  yellow_dye: { id: "dye", damage: 11 },
  lime_dye: { id: "dye", damage: 10 },
  pink_dye: { id: "dye", damage: 9 },
  gray_dye: { id: "dye", damage: 8 },
  light_gray_dye: { id: "dye", damage: 7 },
  cyan_dye: { id: "dye", damage: 6 },
  purple_dye: { id: "dye", damage: 5 },
  blue_dye: { id: "dye", damage: 4 },
  brown_dye: { id: "dye", damage: 3 },
  green_dye: { id: "dye", damage: 2 },
  red_dye: { id: "dye", damage: 1 },
  black_dye: { id: "dye" },
  white_wool: { id: "wool" },
  brown_wool: { id: "wool", damage: 12 },
  gray_wool: { id: "wool", damage: 7 },
  black_wool: { id: "wool", damage: 15 },
  white_carpet: { id: "carpet" },
  white_bed: { id: "bed" },
  white_banner: { id: "banner" },
  white_concrete: { id: "concrete" },
  black_concrete: { id: "concrete", damage: 15 },
  white_terracotta: { id: "stained_hardened_clay" },
  white_glazed_terracotta: { id: "glazed_terracotta" },
  oak_planks: { id: "planks" },
  spruce_planks: { id: "planks", damage: 1 },
  birch_planks: { id: "planks", damage: 2 },
  jungle_planks: { id: "planks", damage: 3 },
  acacia_planks: { id: "planks", damage: 4 },
  dark_oak_planks: { id: "planks", damage: 5 },
  oak_log: { id: "log" },
  granite: { id: "stone", damage: 1 },
  polished_granite: { id: "stone", damage: 2 },
  diorite: { id: "stone", damage: 3 },
  polished_diorite: { id: "stone", damage: 4 },
  andesite: { id: "stone", damage: 5 },
  polished_andesite: { id: "stone", damage: 6 },
  stone_bricks: { id: "stonebrick" },
  mossy_stone_bricks: { id: "stonebrick", damage: 1 },
  chiseled_stone_bricks: { id: "stonebrick", damage: 3 },
  bricks: { id: "brick_block" },
  quartz_pillar: { id: "quartz_block", damage: 2 },
  oak_fence: { id: "fence" },
  oak_door: { id: "wooden_door" },
  oak_stairs: { id: "wooden_stairs" },
  oak_slab: { id: "wooden_slab" },
  oak_boat: { id: "boat" },
  redstone_repeater: { id: "repeater" },
  redstone_comparator: { id: "comparator" },
  slime_block: { id: "slime" },
  player_head: { id: "skull", damage: 3 },
  zombie_head: { id: "skull", damage: 2 },
  creeper_head: { id: "skull", damage: 4 },
  skeleton_skull: { id: "skull" },
  wither_skeleton_skull: { id: "skull", damage: 1 },
  dragon_head: { id: "skull", damage: 5 },
  music_disc_13: { id: "record_13" },
  music_disc_cat: { id: "record_cat" },
  firework_rocket: { id: "fireworks" },
  firework_star: { id: "firework_charge" },
};

/** 1.13 起沿用扁平化 ID；更早版本用扁平化前 ID/Damage。 */
export function legacyItemRef(id: string, version: GiveVersion): { id: string; damage: number } {
  const max = maxVersionOf(version);
  if (max >= 13) return { id, damage: 0 };
  // 1.9 之前喷溅药水与普通药水共用 potion，靠 Damage 第 14 位区分
  if (max < 9 && id === "splash_potion") return { id: "potion", damage: 16384 };
  const ref = LEGACY_ITEM_REFS[id];
  return { id: ref?.id ?? id, damage: ref?.damage ?? 0 };
}

/** 目标版本区间：边界覆盖 NBT 结构、属性名、附魔/效果新增与组件格式变化。 */
export type GiveVersion =
  | "1.7.10"
  | "1.8-1.8.9"
  | "1.9-1.10.2"
  | "1.11-1.11.2"
  | "1.12.2"
  | "1.13-1.15"
  | "1.16-1.18.2"
  | "1.19-1.20.4"
  | "1.20.5-1.20.6"
  | "1.21-1.21.1"
  | "1.21.2-1.21.4"
  | "1.21.5+";

export interface GiveVersionOption {
  value: GiveVersion;
  /** 下拉显示（含语法档）。 */
  label: string;
  /** 区间本身（用于输出标题）。 */
  range: string;
}

export const GIVE_VERSION_OPTIONS: GiveVersionOption[] = [
  { value: "1.7.10", label: "1.7.10（NBT 最旧）", range: "1.7.10" },
  { value: "1.8-1.8.9", label: "1.8–1.8.9（NBT）", range: "1.8–1.8.9" },
  { value: "1.9-1.10.2", label: "1.9–1.10.2（NBT）", range: "1.9–1.10.2" },
  { value: "1.11-1.11.2", label: "1.11–1.11.2（NBT）", range: "1.11–1.11.2" },
  { value: "1.12.2", label: "1.12.2（NBT 旧版）", range: "1.12.2" },
  { value: "1.13-1.15", label: "1.13–1.15（NBT）", range: "1.13–1.15" },
  { value: "1.16-1.18.2", label: "1.16–1.18.2（NBT）", range: "1.16–1.18.2" },
  { value: "1.19-1.20.4", label: "1.19–1.20.4（NBT）", range: "1.19–1.20.4" },
  { value: "1.20.5-1.20.6", label: "1.20.5–1.20.6（组件）", range: "1.20.5–1.20.6" },
  { value: "1.21-1.21.1", label: "1.21–1.21.1（组件）", range: "1.21–1.21.1" },
  { value: "1.21.2-1.21.4", label: "1.21.2–1.21.4（组件）", range: "1.21.2–1.21.4" },
  { value: "1.21.5+", label: "1.21.5+（组件）", range: "1.21.5+" },
];

export const DEFAULT_GIVE_VERSION: GiveVersion = "1.21.5+";

/** 区间上限（小数表示小版本，如 20.4 = 1.20.4），用于 since 过滤与格式分支。 */
const VERSION_MAX: Record<GiveVersion, number> = {
  // 7.10 与 7.1 数值相同；区间比较只需 >7 且 <8
  "1.7.10": 7.1,
  "1.8-1.8.9": 8.9,
  "1.9-1.10.2": 10.2,
  "1.11-1.11.2": 11.2,
  "1.12.2": 12.2,
  "1.13-1.15": 15,
  "1.16-1.18.2": 18.2,
  "1.19-1.20.4": 20.4,
  "1.20.5-1.20.6": 20.6,
  "1.21-1.21.1": 21.1,
  "1.21.2-1.21.4": 21.4,
  "1.21.5+": Number.POSITIVE_INFINITY,
};

export function maxVersionOf(version: GiveVersion): number {
  return VERSION_MAX[version];
}

/** 语法档：1.20.5 起为组件。 */
export type GiveFormat = "legacy" | "modern";

export function syntaxOfVersion(version: GiveVersion): GiveFormat {
  return maxVersionOf(version) >= 20.5 ? "modern" : "legacy";
}

/** §x 自定义颜色自 1.16 起支持。 */
export function supportsHexColor(version: GiveVersion): boolean {
  return maxVersionOf(version) >= 16;
}

export interface GiveEnchant {
  id: string;
  name: string;
  max: number;
  /** 1.13 前 NBT 数字 ID（1.13+ 不用）。 */
  numId?: number;
  /** 引入主版本，缺省 7（1.7.10 可用的经典附魔）。 */
  since?: number;
}

/** 附魔 ID（1.13+ 命名空间；1.13 前使用 numId）。 */
export const ENCHANTMENTS: GiveEnchant[] = [
  { id: "sharpness", name: "锋利", max: 5, numId: 16 },
  { id: "smite", name: "亡灵杀手", max: 5, numId: 17 },
  { id: "bane_of_arthropods", name: "节肢杀手", max: 5, numId: 18 },
  { id: "knockback", name: "击退", max: 2, numId: 19 },
  { id: "fire_aspect", name: "火焰附加", max: 2, numId: 20 },
  { id: "looting", name: "抢夺", max: 3, numId: 21 },
  { id: "sweeping_edge", name: "横扫之刃", max: 3, numId: 22, since: 11.1 },
  { id: "efficiency", name: "效率", max: 5, numId: 32 },
  { id: "silk_touch", name: "精准采集", max: 1, numId: 33 },
  { id: "fortune", name: "时运", max: 3, numId: 35 },
  { id: "protection", name: "保护", max: 4, numId: 0 },
  { id: "fire_protection", name: "火焰保护", max: 4, numId: 1 },
  { id: "blast_protection", name: "爆炸保护", max: 4, numId: 3 },
  { id: "projectile_protection", name: "弹射物保护", max: 4, numId: 4 },
  { id: "respiration", name: "水下呼吸", max: 3, numId: 5 },
  { id: "aqua_affinity", name: "水下速掘", max: 1, numId: 6 },
  { id: "thorns", name: "荆棘", max: 3, numId: 7 },
  { id: "depth_strider", name: "深海探索者", max: 3, numId: 8, since: 8 },
  { id: "frost_walker", name: "冰霜行者", max: 2, numId: 9, since: 9 },
  { id: "soul_speed", name: "灵魂疾行", max: 3 , since: 16},
  { id: "swift_sneak", name: "迅捷潜行", max: 3 , since: 19},
  { id: "feather_falling", name: "摔落缓冲", max: 4, numId: 2 },
  { id: "punch", name: "冲击", max: 2, numId: 49 },
  { id: "power", name: "力量", max: 5, numId: 48 },
  { id: "flame", name: "火焰", max: 1, numId: 50 },
  { id: "infinity", name: "无限", max: 1, numId: 51 },
  { id: "luck_of_the_sea", name: "海之眷顾", max: 3, numId: 61 },
  { id: "lure", name: "饵钓", max: 3, numId: 62 },
  { id: "mending", name: "经验修补", max: 1, numId: 70, since: 9 },
  { id: "unbreaking", name: "耐久", max: 3, numId: 34 },
  // 1.13+ 的命名空间 ID 与 1.11–1.12 的数字 ID 不同
  { id: "binding_curse", name: "绑定诅咒", max: 1, numId: 10, since: 11 },
  { id: "vanishing_curse", name: "消失诅咒", max: 1, numId: 71, since: 11 },
  { id: "multishot", name: "多重射击", max: 1 , since: 14},
  { id: "piercing", name: "穿透", max: 4 , since: 14},
  { id: "quick_charge", name: "快速装填", max: 3 , since: 14},
  { id: "channeling", name: "引雷", max: 1 , since: 13},
  { id: "riptide", name: "激流", max: 3 , since: 13},
  { id: "loyalty", name: "忠诚", max: 3 , since: 13},
  { id: "impaling", name: "穿刺", max: 5 , since: 13},
  { id: "density", name: "致密", max: 5 , since: 21},
  { id: "breach", name: "破甲", max: 4 , since: 21},
  { id: "wind_burst", name: "风爆", max: 3 , since: 21},
];

/** 按目标区间过滤附魔（查表取 since，兼容选项表与已选输入）。 */
export function filterEnchantsByVersion<T extends { id: string }>(enchs: T[], version: GiveVersion): T[] {
  const max = maxVersionOf(version);
  return enchs.filter((e) => (ENCHANTMENTS.find((x) => x.id === e.id)?.since ?? 7) <= max);
}

/** amount 为附加量，operation 固定相加。 */
export interface GiveAttribute {
  id: string;
  name: string;
  /** 引入版本：20.5 = 1.20.5；缺省 7（1.7.10 可用的经典属性）。 */
  since?: number;
}

export const ATTRIBUTES: GiveAttribute[] = [
  { id: "generic.attack_damage", name: "攻击伤害" },
  { id: "generic.attack_speed", name: "攻击速度", since: 9 },
  { id: "generic.armor", name: "护甲值", since: 9 },
  { id: "generic.armor_toughness", name: "护甲韧性", since: 9.1 },
  { id: "generic.max_health", name: "最大生命值" },
  { id: "generic.movement_speed", name: "移动速度" },
  { id: "generic.knockback_resistance", name: "击退抗性" },
  { id: "generic.luck", name: "幸运", since: 9 },
  { id: "generic.follow_range", name: "跟随范围" },
  { id: "generic.scale", name: "体型", since: 20.5 },
  { id: "generic.step_height", name: "步高", since: 20.5 },
  { id: "generic.gravity", name: "重力", since: 20.5 },
  { id: "generic.safe_fall_distance", name: "安全摔落距离", since: 20.5 },
];

/** 按目标区间过滤属性：1.20.5 前不含新增属性。 */
export function filterAttributesByVersion<T extends { id: string }>(attrs: T[], version: GiveVersion): T[] {
  const max = maxVersionOf(version);
  return attrs.filter(
    (a) => (ATTRIBUTES.find((x) => x.id === a.id)?.since ?? 7) <= max,
  );
}

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

/** 1.20.5–1.20.6 用 uuid+name；1.21+ 用 id；1.21.2+ 属性名去掉分组前缀。 */
function modernAttributeModifier(a: AttributeInput, version: GiveVersion): string {
  const id = maxVersionOf(version) >= 21.2 ? a.id.replace(/^(?:generic|zombie|horse)\./, "") : a.id;
  const type = `minecraft:${id}`;
  if (version === "1.20.5-1.20.6") {
    return `{type:"${type}",uuid:[I;${uuidToIntArray(crypto.randomUUID())}],name:"${a.id}",amount:${a.amount},operation:"add_value"}`;
  }
  return `{type:"${type}",id:"minecraft:${crypto.randomUUID()}",amount:${a.amount},operation:"add_value"}`;
}

/** give 命令与交易物品槽 components 共用。`forBook`：附魔书用 stored_enchantments（普通 enchantments 在附魔书上无效）。 */
export function modernItemComponents(mods: ItemMods, version: GiveVersion, forBook = false): ModernComponent[] {
  const parts: ModernComponent[] = [];
  const enchants = filterEnchantsByVersion(mods.enchantments, version);
  if (enchants.length > 0) {
    const levels = enchants
      .map((e) => `"minecraft:${e.id}":${clampLevel(e.level)}`)
      .join(",");
    // 1.21.5+ 直接是附魔映射；此前包在 levels 里
    const value = maxVersionOf(version) >= 21.5 ? `{${levels}}` : `{levels:{${levels}}}`;
    parts.push({ key: forBook ? "stored_enchantments" : "enchantments", value });
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
    // 1.21.5 移除 hide_additional_tooltip，改用 tooltip_display
    parts.push(
      maxVersionOf(version) >= 21.5
        ? { key: "tooltip_display", value: "{hide_tooltip:true,hidden_components:[]}" }
        : { key: "hide_additional_tooltip", value: "{}" },
    );
  }
  const attrs = filterAttributesByVersion(mods.attributes, version);
  if (attrs.length > 0) {
    parts.push({
      key: "attribute_modifiers",
      value: `[${attrs.map((a) => modernAttributeModifier(a, version)).join(",")}]`,
    });
  }
  return parts;
}

/** give 命令与交易物品槽共用。`forBook`：附魔书用 StoredEnchantments。 */
export function legacyItemTags(mods: ItemMods, version: GiveVersion, forBook = false): string[] {
  const tags: string[] = [];
  const max = maxVersionOf(version);
  const enchants = filterEnchantsByVersion(mods.enchantments, version);
  if (enchants.length > 0) {
    // 1.13 扁平化前是 ench（数字 ID）；1.13+ 是 Enchantments（命名空间 ID）
    const numeric = max < 13;
    const list = enchants
      .map((e) => {
        if (numeric) {
          const num = ENCHANTMENTS.find((x) => x.id === e.id)?.numId ?? 0;
          return `{id:${num}s,lvl:${clampLevel(e.level)}s}`;
        }
        // 1.20.5 前横扫之刃的命名空间 ID 是 sweeping
        const id = e.id === "sweeping_edge" && max < 20.5 ? "sweeping" : e.id;
        return `{id:"minecraft:${id}",lvl:${clampLevel(e.level)}}`;
      })
      .join(",");
    const tag = forBook ? "StoredEnchantments" : numeric ? "ench" : "Enchantments";
    tags.push(`${tag}:[${list}]`);
  }
  if (mods.unbreakable && max >= 8) {
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
  if (mods.hideTooltip && max >= 8) {
    // 32 = 隐藏附加信息（药水/烟花/书本等）；63 = 全部
    tags.push("HideFlags:32");
  }
  const attrs = filterAttributesByVersion(mods.attributes, version);
  if (attrs.length > 0) {
    // 1.7.10–1.15 用驼峰名 + UUIDMost/UUIDLeast；1.16+ 用注册名 + UUID 数组
    const longUuid = max < 16;
    // Slot 1.9 才加入；1.7/1.8 的属性修饰符不区分槽位
    const slot = max >= 9 ? ',Slot:"mainhand"' : "";
    const list = attrs
      .map((a) => {
        const name = longUuid ? toCamelCase(a.id) : a.id;
        const uuid = crypto.randomUUID();
        if (longUuid) {
          const { most, least } = uuidToLongPair(uuid);
          return `{AttributeName:"${name}",Name:"${name}",Amount:${a.amount},Operation:0${slot},UUIDMost:${most},UUIDLeast:${least}}`;
        }
        return `{AttributeName:"${name}",Name:"${name}",Amount:${a.amount},Operation:0${slot},UUID:[I;${uuidToIntArray(uuid)}]}`;
      })
      .join(",");
    tags.push(`AttributeModifiers:[${list}]`);
  }
  return tags;
}

/** 1.7.10–1.15 的属性名（1.16 起改用注册名）。 */
function toCamelCase(id: string): string {
  return id
    .split(".")
    .map((seg) => seg.replace(/_([a-z])/g, (_, c: string) => c.toUpperCase()))
    .join(".");
}

/** 附魔等级 clamp（1–255）。 */
function clampLevel(level: number): number {
  return Math.max(1, Math.min(level, 255));
}

/** 交易物品槽：modern `{id,count,components:{...}}` / legacy `{id,Count,Damage,tag:{...}}`，修饰与 give 共用构建。
 *  `raw` 为槽级自由内容（trade 语法）：以 `{` 开头按完整物品栈原样覆盖；否则作为附加内容并入 components/tag。 */
export function buildTradeItem(
  id: string,
  count: number,
  mods: ItemMods,
  version: GiveVersion,
  raw?: string,
): string {
  const c = Math.max(1, Math.min(64, Math.floor(count) || 1));
  // 完整物品栈（用户自带 id/Count/tag 或 id/count/components）：判别先于归一化，原样透传
  const rawTrimmed = (raw ?? "").trim();
  if (rawTrimmed.startsWith("{")) return rawTrimmed;
  const extra = normalizeNbtFragment(rawTrimmed);
  // 附魔书：附魔落在 StoredEnchantments / stored_enchantments
  const forBook = id === "enchanted_book";
  if (syntaxOfVersion(version) === "modern") {
    const comps = modernItemComponents(mods, version, forBook).map((p) => `"minecraft:${p.key}":${p.value}`);
    const rawSelf = normalizeNbtFragment(mods.rawModern);
    if (rawSelf) comps.push(rawSelf);
    if (extra) comps.push(extra);
    const compsStr = comps.length > 0 ? `,components:{${comps.join(",")}}` : "";
    return `{id:"minecraft:${id}",count:${c}${compsStr}}`;
  }
  const tags = legacyItemTags(mods, version, forBook);
  const rawSelf = normalizeNbtFragment(mods.rawLegacy);
  if (rawSelf) tags.push(rawSelf);
  if (extra) tags.push(extra);
  // 1.13 前用扁平化前 ID/Damage；1.13+ 沿用扁平化 ID
  const ref = legacyItemRef(id, version);
  const damageStr = ref.damage !== 0 ? `,Damage:${ref.damage}s` : "";
  const tagsStr = tags.length > 0 ? `,tag:{${tags.join(",")}}` : "";
  return `{id:"minecraft:${ref.id}",Count:${c}b${damageStr}${tagsStr}}`;
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

/** UUID（hex）→ 1.7.10–1.15 的 UUIDMost/UUIDLeast（64 位有符号长整，带 L 后缀）。 */
function uuidToLongPair(uuid: string): { most: string; least: string } {
  const hex = uuid.replace(/-/g, "");
  const toLong = (h: string) => `${BigInt.asIntN(64, BigInt(`0x${h}`))}L`;
  return { most: toLong(hex.slice(0, 16)), least: toLong(hex.slice(16)) };
}

