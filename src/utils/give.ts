// /give 命令生成器：物品数据表 + 版本区间构建（1.7.10–1.20.4 为 NBT，1.20.5+ 为组件；1.13 前用数字附魔 ID/扁平化前物品 ID，1.16+ 属性改用注册名）；物品表用 1.13+ 命名空间，输出时按版本映射。

import { normalizeNbtFragment } from "./nbtText";
import { currentLocale } from "../i18n";

export interface GiveItem {
  /** 无 minecraft: 前缀。 */
  id: string;
  name: string;
  /** 官方英文名（缺失时展示 id）。 */
  nameEn?: string;
  cat: string;
}

/** 按当前界面语言取物品名；英文缺失时回退到注册 id。 */
export function itemName(item: GiveItem): string {
  return currentLocale.value === "en-US" ? item.nameEn ?? item.id : item.name;
}

export const GIVE_ITEMS: GiveItem[] = [
  // ── 剑 ──
  { id: "wooden_sword", name: "木剑", cat: "sword", nameEn: "Wooden Sword" },
  { id: "stone_sword", name: "石剑", cat: "sword", nameEn: "Stone Sword" },
  { id: "iron_sword", name: "铁剑", cat: "sword", nameEn: "Iron Sword" },
  { id: "golden_sword", name: "金剑", cat: "sword", nameEn: "Golden Sword" },
  { id: "diamond_sword", name: "钻石剑", cat: "sword", nameEn: "Diamond Sword" },
  { id: "netherite_sword", name: "下界合金剑", cat: "sword", nameEn: "Netherite Sword" },
  // ── 工具 ──
  { id: "wooden_pickaxe", name: "木镐", cat: "tool", nameEn: "Wooden Pickaxe" },
  { id: "stone_pickaxe", name: "石镐", cat: "tool", nameEn: "Stone Pickaxe" },
  { id: "iron_pickaxe", name: "铁镐", cat: "tool", nameEn: "Iron Pickaxe" },
  { id: "golden_pickaxe", name: "金镐", cat: "tool", nameEn: "Golden Pickaxe" },
  { id: "diamond_pickaxe", name: "钻石镐", cat: "tool", nameEn: "Diamond Pickaxe" },
  { id: "netherite_pickaxe", name: "下界合金镐", cat: "tool", nameEn: "Netherite Pickaxe" },
  { id: "wooden_axe", name: "木斧", cat: "tool", nameEn: "Wooden Axe" },
  { id: "stone_axe", name: "石斧", cat: "tool", nameEn: "Stone Axe" },
  { id: "iron_axe", name: "铁斧", cat: "tool", nameEn: "Iron Axe" },
  { id: "golden_axe", name: "金斧", cat: "tool", nameEn: "Golden Axe" },
  { id: "diamond_axe", name: "钻石斧", cat: "tool", nameEn: "Diamond Axe" },
  { id: "netherite_axe", name: "下界合金斧", cat: "tool", nameEn: "Netherite Axe" },
  { id: "wooden_shovel", name: "木锹", cat: "tool", nameEn: "Wooden Shovel" },
  { id: "stone_shovel", name: "石锹", cat: "tool", nameEn: "Stone Shovel" },
  { id: "iron_shovel", name: "铁锹", cat: "tool", nameEn: "Iron Shovel" },
  { id: "golden_shovel", name: "金锹", cat: "tool", nameEn: "Golden Shovel" },
  { id: "diamond_shovel", name: "钻石锹", cat: "tool", nameEn: "Diamond Shovel" },
  { id: "netherite_shovel", name: "下界合金锹", cat: "tool", nameEn: "Netherite Shovel" },
  { id: "wooden_hoe", name: "木锄", cat: "tool", nameEn: "Wooden Hoe" },
  { id: "stone_hoe", name: "石锄", cat: "tool", nameEn: "Stone Hoe" },
  { id: "iron_hoe", name: "铁锄", cat: "tool", nameEn: "Iron Hoe" },
  { id: "golden_hoe", name: "金锄", cat: "tool", nameEn: "Golden Hoe" },
  { id: "diamond_hoe", name: "钻石锄", cat: "tool", nameEn: "Diamond Hoe" },
  { id: "netherite_hoe", name: "下界合金锄", cat: "tool", nameEn: "Netherite Hoe" },
  { id: "bow", name: "弓", cat: "tool", nameEn: "Bow" },
  { id: "crossbow", name: "弩", cat: "tool", nameEn: "Crossbow" },
  { id: "trident", name: "三叉戟", cat: "tool", nameEn: "Trident" },
  { id: "shield", name: "盾牌", cat: "tool", nameEn: "Shield" },
  { id: "shears", name: "剪刀", cat: "tool", nameEn: "Shears" },
  { id: "fishing_rod", name: "钓鱼竿", cat: "tool", nameEn: "Fishing Rod" },
  { id: "flint_and_steel", name: "打火石", cat: "tool", nameEn: "Flint and Steel" },
  { id: "brush", name: "刷子", cat: "tool", nameEn: "Brush" },
  { id: "spyglass", name: "望远镜", cat: "tool", nameEn: "Spyglass" },
  { id: "compass", name: "指南针", cat: "tool", nameEn: "Compass" },
  { id: "recovery_compass", name: "回溯指南针", cat: "tool", nameEn: "Recovery Compass" },
  { id: "clock", name: "时钟", cat: "tool", nameEn: "Clock" },
  { id: "bucket", name: "桶", cat: "tool", nameEn: "Bucket" },
  { id: "water_bucket", name: "水桶", cat: "tool", nameEn: "Water Bucket" },
  { id: "lava_bucket", name: "熔岩桶", cat: "tool", nameEn: "Lava Bucket" },
  { id: "milk_bucket", name: "奶桶", cat: "tool", nameEn: "Milk Bucket" },
  { id: "map", name: "地图", cat: "tool", nameEn: "Map" },
  { id: "name_tag", name: "命名牌", cat: "tool", nameEn: "Name Tag" },
  { id: "lead", name: "拴绳", cat: "tool", nameEn: "Lead" },
  { id: "bone_meal", name: "骨粉", cat: "tool", nameEn: "Bone Meal" },
  { id: "ender_eye", name: "末影之眼", cat: "tool", nameEn: "Eye of Ender" },
  { id: "totem_of_undying", name: "不死图腾", cat: "tool", nameEn: "Totem of Undying" },
  { id: "elytra", name: "鞘翅", cat: "tool", nameEn: "Elytra" },
  { id: "firework_rocket", name: "烟花火箭", cat: "tool", nameEn: "Firework Rocket" },
  { id: "goat_horn", name: "山羊角", cat: "tool", nameEn: "Goat Horn" },
  // ── 盔甲 ──
  { id: "leather_helmet", name: "皮革帽子", cat: "armor", nameEn: "Leather Cap" },
  { id: "leather_chestplate", name: "皮革外套", cat: "armor", nameEn: "Leather Tunic" },
  { id: "leather_leggings", name: "皮革裤子", cat: "armor", nameEn: "Leather Pants" },
  { id: "leather_boots", name: "皮革靴子", cat: "armor", nameEn: "Leather Boots" },
  { id: "chainmail_helmet", name: "锁链头盔", cat: "armor", nameEn: "Chainmail Helmet" },
  { id: "chainmail_chestplate", name: "锁链胸甲", cat: "armor", nameEn: "Chainmail Chestplate" },
  { id: "chainmail_leggings", name: "锁链护腿", cat: "armor", nameEn: "Chainmail Leggings" },
  { id: "chainmail_boots", name: "锁链靴子", cat: "armor", nameEn: "Chainmail Boots" },
  { id: "iron_helmet", name: "铁头盔", cat: "armor", nameEn: "Iron Helmet" },
  { id: "iron_chestplate", name: "铁胸甲", cat: "armor", nameEn: "Iron Chestplate" },
  { id: "iron_leggings", name: "铁护腿", cat: "armor", nameEn: "Iron Leggings" },
  { id: "iron_boots", name: "铁靴子", cat: "armor", nameEn: "Iron Boots" },
  { id: "golden_helmet", name: "金头盔", cat: "armor", nameEn: "Golden Helmet" },
  { id: "golden_chestplate", name: "金胸甲", cat: "armor", nameEn: "Golden Chestplate" },
  { id: "golden_leggings", name: "金护腿", cat: "armor", nameEn: "Golden Leggings" },
  { id: "golden_boots", name: "金靴子", cat: "armor", nameEn: "Golden Boots" },
  { id: "diamond_helmet", name: "钻石头盔", cat: "armor", nameEn: "Diamond Helmet" },
  { id: "diamond_chestplate", name: "钻石胸甲", cat: "armor", nameEn: "Diamond Chestplate" },
  { id: "diamond_leggings", name: "钻石护腿", cat: "armor", nameEn: "Diamond Leggings" },
  { id: "diamond_boots", name: "钻石靴子", cat: "armor", nameEn: "Diamond Boots" },
  { id: "netherite_helmet", name: "下界合金头盔", cat: "armor", nameEn: "Netherite Helmet" },
  { id: "netherite_chestplate", name: "下界合金胸甲", cat: "armor", nameEn: "Netherite Chestplate" },
  { id: "netherite_leggings", name: "下界合金护腿", cat: "armor", nameEn: "Netherite Leggings" },
  { id: "netherite_boots", name: "下界合金靴子", cat: "armor", nameEn: "Netherite Boots" },
  { id: "turtle_helmet", name: "海龟壳", cat: "armor", nameEn: "Turtle Shell" },
  // ── 食物 ──
  { id: "apple", name: "苹果", cat: "food", nameEn: "Apple" },
  { id: "golden_apple", name: "金苹果", cat: "food", nameEn: "Golden Apple" },
  { id: "enchanted_golden_apple", name: "附魔金苹果", cat: "food", nameEn: "Enchanted Golden Apple" },
  { id: "bread", name: "面包", cat: "food", nameEn: "Bread" },
  { id: "cooked_beef", name: "牛排", cat: "food", nameEn: "Steak" },
  { id: "cooked_porkchop", name: "熟猪排", cat: "food", nameEn: "Cooked Porkchop" },
  { id: "cooked_chicken", name: "熟鸡肉", cat: "food", nameEn: "Cooked Chicken" },
  { id: "cooked_mutton", name: "熟羊肉", cat: "food", nameEn: "Cooked Mutton" },
  { id: "cooked_rabbit", name: "熟兔肉", cat: "food", nameEn: "Cooked Rabbit" },
  { id: "cooked_cod", name: "熟鳕鱼", cat: "food", nameEn: "Cooked Cod" },
  { id: "cooked_salmon", name: "熟鲑鱼", cat: "food", nameEn: "Cooked Salmon" },
  { id: "baked_potato", name: "烤马铃薯", cat: "food", nameEn: "Baked Potato" },
  { id: "carrot", name: "胡萝卜", cat: "food", nameEn: "Carrot" },
  { id: "golden_carrot", name: "金胡萝卜", cat: "food", nameEn: "Golden Carrot" },
  { id: "sweet_berries", name: "甜浆果", cat: "food", nameEn: "Sweet Berries" },
  { id: "glow_berries", name: "发光浆果", cat: "food", nameEn: "Glow Berries" },
  { id: "melon_slice", name: "西瓜片", cat: "food", nameEn: "Melon Slice" },
  { id: "pumpkin_pie", name: "南瓜派", cat: "food", nameEn: "Pumpkin Pie" },
  { id: "cookie", name: "曲奇", cat: "food", nameEn: "Cookie" },
  { id: "cake", name: "蛋糕", cat: "food", nameEn: "Cake" },
  { id: "honey_bottle", name: "蜂蜜瓶", cat: "food", nameEn: "Honey Bottle" },
  { id: "mushroom_stew", name: "蘑菇煲", cat: "food", nameEn: "Mushroom Stew" },
  { id: "beetroot_soup", name: "甜菜汤", cat: "food", nameEn: "Beetroot Soup" },
  { id: "chorus_fruit", name: "紫颂果", cat: "food", nameEn: "Chorus Fruit" },
  { id: "dried_kelp", name: "干海带", cat: "food", nameEn: "Dried Kelp" },
  { id: "rabbit_stew", name: "兔肉煲", cat: "food", nameEn: "Rabbit Stew" },
  // ── 材料 ──
  { id: "diamond", name: "钻石", cat: "material", nameEn: "Diamond" },
  { id: "iron_ingot", name: "铁锭", cat: "material", nameEn: "Iron Ingot" },
  { id: "gold_ingot", name: "金锭", cat: "material", nameEn: "Gold Ingot" },
  { id: "copper_ingot", name: "铜锭", cat: "material", nameEn: "Copper Ingot" },
  { id: "netherite_ingot", name: "下界合金锭", cat: "material", nameEn: "Netherite Ingot" },
  { id: "emerald", name: "绿宝石", cat: "material", nameEn: "Emerald" },
  { id: "lapis_lazuli", name: "青金石", cat: "material", nameEn: "Lapis Lazuli" },
  { id: "redstone", name: "红石粉", cat: "material", nameEn: "Redstone Dust" },
  { id: "coal", name: "煤炭", cat: "material", nameEn: "Coal" },
  { id: "charcoal", name: "木炭", cat: "material", nameEn: "Charcoal" },
  { id: "quartz", name: "下界石英", cat: "material", nameEn: "Nether Quartz" },
  { id: "amethyst_shard", name: "紫水晶碎片", cat: "material", nameEn: "Amethyst Shard" },
  { id: "echo_shard", name: "回响碎片", cat: "material", nameEn: "Echo Shard" },
  { id: "blaze_rod", name: "烈焰棒", cat: "material", nameEn: "Blaze Rod" },
  { id: "blaze_powder", name: "烈焰粉", cat: "material", nameEn: "Blaze Powder" },
  { id: "ghast_tear", name: "恶魂之泪", cat: "material", nameEn: "Ghast Tear" },
  { id: "spider_eye", name: "蜘蛛眼", cat: "material", nameEn: "Spider Eye" },
  { id: "fermented_spider_eye", name: "发酵蛛眼", cat: "material", nameEn: "Fermented Spider Eye" },
  { id: "gunpowder", name: "火药", cat: "material", nameEn: "Gunpowder" },
  { id: "string", name: "线", cat: "material", nameEn: "String" },
  { id: "feather", name: "羽毛", cat: "material", nameEn: "Feather" },
  { id: "bone", name: "骨头", cat: "material", nameEn: "Bone" },
  { id: "leather", name: "皮革", cat: "material", nameEn: "Leather" },
  { id: "rabbit_hide", name: "兔子皮", cat: "material", nameEn: "Rabbit Hide" },
  { id: "rabbit_foot", name: "兔子脚", cat: "material", nameEn: "Rabbit's Foot" },
  { id: "ink_sac", name: "墨囊", cat: "material", nameEn: "Ink Sac" },
  { id: "glow_ink_sac", name: "发光墨囊", cat: "material", nameEn: "Glow Ink Sac" },
  { id: "slime_ball", name: "粘液球", cat: "material", nameEn: "Slimeball" },
  { id: "magma_cream", name: "岩浆膏", cat: "material", nameEn: "Magma Cream" },
  { id: "phantom_membrane", name: "幻翼膜", cat: "material", nameEn: "Phantom Membrane" },
  { id: "prismarine_shard", name: "海晶碎片", cat: "material", nameEn: "Prismarine Shard" },
  { id: "prismarine_crystals", name: "海晶砂粒", cat: "material", nameEn: "Prismarine Crystals" },
  { id: "nautilus_shell", name: "鹦鹉螺壳", cat: "material", nameEn: "Nautilus Shell" },
  { id: "heart_of_the_sea", name: "海洋之心", cat: "material", nameEn: "Heart of the Sea" },
  { id: "nether_star", name: "下界之星", cat: "material", nameEn: "Nether Star" },
  { id: "shulker_shell", name: "潜影壳", cat: "material", nameEn: "Shulker Shell" },
  { id: "dragon_breath", name: "龙息", cat: "material", nameEn: "Dragon's Breath" },
  { id: "ender_pearl", name: "末影珍珠", cat: "material", nameEn: "Ender Pearl" },
  { id: "paper", name: "纸", cat: "material", nameEn: "Paper" },
  { id: "book", name: "书", cat: "material", nameEn: "Book" },
  { id: "writable_book", name: "书与笔", cat: "material", nameEn: "Book and Quill" },
  { id: "honeycomb", name: "蜜脾", cat: "material", nameEn: "Honeycomb" },
  { id: "scute", name: "鳞甲", cat: "material", nameEn: "Turtle Scute" },
  // ── 矿物块 ──
  { id: "diamond_block", name: "钻石块", cat: "block", nameEn: "Block of Diamond" },
  { id: "iron_block", name: "铁块", cat: "block", nameEn: "Block of Iron" },
  { id: "gold_block", name: "金块", cat: "block", nameEn: "Block of Gold" },
  { id: "copper_block", name: "铜块", cat: "block", nameEn: "Block of Copper" },
  { id: "netherite_block", name: "下界合金块", cat: "block", nameEn: "Block of Netherite" },
  { id: "emerald_block", name: "绿宝石块", cat: "block", nameEn: "Block of Emerald" },
  { id: "lapis_block", name: "青金石块", cat: "block", nameEn: "Block of Lapis Lazuli" },
  { id: "redstone_block", name: "红石块", cat: "block", nameEn: "Block of Redstone" },
  { id: "coal_block", name: "煤炭块", cat: "block", nameEn: "Block of Coal" },
  { id: "quartz_block", name: "石英块", cat: "block", nameEn: "Block of Quartz" },
  { id: "amethyst_block", name: "紫水晶块", cat: "block", nameEn: "Block of Amethyst" },
  { id: "raw_iron_block", name: "粗铁块", cat: "block", nameEn: "Block of Raw Iron" },
  { id: "raw_gold_block", name: "粗金块", cat: "block", nameEn: "Block of Raw Gold" },
  { id: "raw_copper_block", name: "粗铜块", cat: "block", nameEn: "Block of Raw Copper" },
  // ── 红石 ──
  { id: "redstone_repeater", name: "红石中继器", cat: "redstone", nameEn: "Redstone Repeater" },
  { id: "redstone_comparator", name: "红石比较器", cat: "redstone", nameEn: "Redstone Comparator" },
  { id: "piston", name: "活塞", cat: "redstone", nameEn: "Piston" },
  { id: "sticky_piston", name: "粘性活塞", cat: "redstone", nameEn: "Sticky Piston" },
  { id: "observer", name: "侦测器", cat: "redstone", nameEn: "Observer" },
  { id: "hopper", name: "漏斗", cat: "redstone", nameEn: "Hopper" },
  { id: "dispenser", name: "发射器", cat: "redstone", nameEn: "Dispenser" },
  { id: "dropper", name: "投掷器", cat: "redstone", nameEn: "Dropper" },
  { id: "note_block", name: "音符盒", cat: "redstone", nameEn: "Note Block" },
  { id: "lever", name: "拉杆", cat: "redstone", nameEn: "Lever" },
  { id: "redstone_lamp", name: "红石灯", cat: "redstone", nameEn: "Redstone Lamp" },
  { id: "target", name: "标靶", cat: "redstone", nameEn: "Target" },
  { id: "slime_block", name: "粘液块", cat: "redstone", nameEn: "Slime Block" },
  { id: "honey_block", name: "蜜块", cat: "redstone", nameEn: "Honey Block" },
  { id: "daylight_detector", name: "阳光探测器", cat: "redstone", nameEn: "Daylight Detector" },
  { id: "tripwire_hook", name: "绊线钩", cat: "redstone", nameEn: "Tripwire Hook" },
  { id: "trapped_chest", name: "陷阱箱", cat: "redstone", nameEn: "Trapped Chest" },
  { id: "jukebox", name: "唱片机", cat: "redstone", nameEn: "Jukebox" },
  { id: "sculk_sensor", name: "潜声传感器", cat: "redstone", nameEn: "Sculk Sensor" },
  { id: "calibrated_sculk_sensor", name: "校定潜声传感器", cat: "redstone", nameEn: "Calibrated Sculk Sensor" },
  // ── 建筑 ──
  { id: "oak_planks", name: "橡木木板", cat: "building", nameEn: "Oak Planks" },
  { id: "spruce_planks", name: "云杉木板", cat: "building", nameEn: "Spruce Planks" },
  { id: "birch_planks", name: "白桦木板", cat: "building", nameEn: "Birch Planks" },
  { id: "jungle_planks", name: "丛林木板", cat: "building", nameEn: "Jungle Planks" },
  { id: "acacia_planks", name: "金合欢木板", cat: "building", nameEn: "Acacia Planks" },
  { id: "dark_oak_planks", name: "深色橡木木板", cat: "building", nameEn: "Dark Oak Planks" },
  { id: "mangrove_planks", name: "红树木板", cat: "building", nameEn: "Mangrove Planks" },
  { id: "cherry_planks", name: "樱花木板", cat: "building", nameEn: "Cherry Planks" },
  { id: "bamboo_planks", name: "竹木板", cat: "building", nameEn: "Bamboo Planks" },
  { id: "crimson_planks", name: "绯红木板", cat: "building", nameEn: "Crimson Planks" },
  { id: "warped_planks", name: "诡异木板", cat: "building", nameEn: "Warped Planks" },
  { id: "stone", name: "石头", cat: "building", nameEn: "Stone" },
  { id: "cobblestone", name: "圆石", cat: "building", nameEn: "Cobblestone" },
  { id: "stone_bricks", name: "石砖", cat: "building", nameEn: "Stone Bricks" },
  { id: "mossy_stone_bricks", name: "苔石砖", cat: "building", nameEn: "Mossy Stone Bricks" },
  { id: "deepslate_bricks", name: "深板岩砖", cat: "building", nameEn: "Deepslate Bricks" },
  { id: "polished_andesite", name: "磨制安山岩", cat: "building", nameEn: "Polished Andesite" },
  { id: "granite", name: "花岗岩", cat: "building", nameEn: "Granite" },
  { id: "diorite", name: "闪长岩", cat: "building", nameEn: "Diorite" },
  { id: "sandstone", name: "砂岩", cat: "building", nameEn: "Sandstone" },
  { id: "red_sandstone", name: "红砂岩", cat: "building", nameEn: "Red Sandstone" },
  { id: "bricks", name: "红砖块", cat: "building", nameEn: "Bricks" },
  { id: "glass", name: "玻璃", cat: "building", nameEn: "Glass" },
  { id: "glass_pane", name: "玻璃板", cat: "building", nameEn: "Glass Pane" },
  { id: "white_wool", name: "白色羊毛", cat: "building", nameEn: "White Wool" },
  { id: "black_wool", name: "黑色羊毛", cat: "building", nameEn: "Black Wool" },
  { id: "white_concrete", name: "白色混凝土", cat: "building", nameEn: "White Concrete" },
  { id: "black_concrete", name: "黑色混凝土", cat: "building", nameEn: "Black Concrete" },
  { id: "white_terracotta", name: "白色陶瓦", cat: "building", nameEn: "White Terracotta" },
  { id: "oak_log", name: "橡木原木", cat: "building", nameEn: "Oak Log" },
  { id: "oak_fence", name: "橡木栅栏", cat: "building", nameEn: "Oak Fence" },
  { id: "oak_door", name: "橡木门", cat: "building", nameEn: "Oak Door" },
  { id: "oak_stairs", name: "橡木楼梯", cat: "building", nameEn: "Oak Stairs" },
  { id: "oak_slab", name: "橡木台阶", cat: "building", nameEn: "Oak Slab" },
  { id: "bookshelf", name: "书架", cat: "building", nameEn: "Bookshelf" },
  { id: "enchanting_table", name: "附魔台", cat: "building", nameEn: "Enchanting Table" },
  { id: "anvil", name: "铁砧", cat: "building", nameEn: "Anvil" },
  { id: "grindstone", name: "砂轮", cat: "building", nameEn: "Grindstone" },
  { id: "crafting_table", name: "工作台", cat: "building", nameEn: "Crafting Table" },
  { id: "furnace", name: "熔炉", cat: "building", nameEn: "Furnace" },
  { id: "blast_furnace", name: "高炉", cat: "building", nameEn: "Blast Furnace" },
  { id: "smoker", name: "烟熏炉", cat: "building", nameEn: "Smoker" },
  { id: "beacon", name: "信标", cat: "building", nameEn: "Beacon" },
  { id: "chest", name: "箱子", cat: "building", nameEn: "Chest" },
  { id: "ender_chest", name: "末影箱", cat: "building", nameEn: "Ender Chest" },
  { id: "barrel", name: "木桶", cat: "building", nameEn: "Barrel" },
  { id: "shulker_box", name: "潜影盒", cat: "building", nameEn: "Shulker Box" },
  { id: "ladder", name: "梯子", cat: "building", nameEn: "Ladder" },
  { id: "torch", name: "火把", cat: "building", nameEn: "Torch" },
  { id: "lantern", name: "灯笼", cat: "building", nameEn: "Lantern" },
  { id: "soul_lantern", name: "灵魂灯笼", cat: "building", nameEn: "Soul Lantern" },
  { id: "campfire", name: "营火", cat: "building", nameEn: "Campfire" },
  { id: "bed", name: "床", cat: "building", nameEn: "Bed" },
  { id: "respawn_anchor", name: "重生锚", cat: "building", nameEn: "Respawn Anchor" },
  // ── 杂项 ──
  { id: "experience_bottle", name: "附魔之瓶", cat: "misc", nameEn: "Bottle o' Enchanting" },
  { id: "potion", name: "药水", cat: "misc", nameEn: "Potion" },
  { id: "splash_potion", name: "喷溅药水", cat: "misc", nameEn: "Splash Potion" },
  { id: "lingering_potion", name: "滞留药水", cat: "misc", nameEn: "Lingering Potion" },
  { id: "tipped_arrow", name: "药水箭", cat: "misc", nameEn: "Tipped Arrow" },
  { id: "arrow", name: "箭", cat: "misc", nameEn: "Arrow" },
  { id: "spectral_arrow", name: "光谱之箭", cat: "misc", nameEn: "Spectral Arrow" },
  { id: "snowball", name: "雪球", cat: "misc", nameEn: "Snowball" },
  { id: "player_head", name: "玩家头颅", cat: "misc", nameEn: "Player Head" },
  { id: "zombie_head", name: "僵尸头颅", cat: "misc", nameEn: "Zombie Head" },
  { id: "creeper_head", name: "苦力怕头颅", cat: "misc", nameEn: "Creeper Head" },
  { id: "skeleton_skull", name: "骷髅头颅", cat: "misc", nameEn: "Skeleton Skull" },
  { id: "wither_skeleton_skull", name: "凋灵骷髅头颅", cat: "misc", nameEn: "Wither Skeleton Skull" },
  { id: "dragon_head", name: "龙首", cat: "misc", nameEn: "Dragon Head" },
  { id: "music_disc_13", name: "音乐唱片 13", cat: "misc", nameEn: "Music Disc 13" },
  { id: "music_disc_cat", name: "音乐唱片 Cat", cat: "misc", nameEn: "Music Disc Cat" },
  { id: "music_disc_otherside", name: "音乐唱片 Otherside", cat: "misc", nameEn: "Music Disc Otherside" },
  { id: "debug_stick", name: "调试棒", cat: "misc", nameEn: "Debug Stick" },
  { id: "knowledge_book", name: "知识之书", cat: "misc", nameEn: "Knowledge Book" },
  { id: "stick", name: "木棍", cat: "misc", nameEn: "Stick" },
  { id: "bowl", name: "碗", cat: "misc", nameEn: "Bowl" },
  { id: "flint", name: "燧石", cat: "misc", nameEn: "Flint" },
  // ── 村民交易常用 ──
  { id: "wheat", name: "小麦", cat: "material", nameEn: "Wheat" },
  { id: "potato", name: "马铃薯", cat: "food", nameEn: "Potato" },
  { id: "beetroot", name: "甜菜根", cat: "food", nameEn: "Beetroot" },
  { id: "pumpkin", name: "南瓜", cat: "block", nameEn: "Pumpkin" },
  { id: "melon", name: "西瓜", cat: "block", nameEn: "Melon" },
  { id: "rotten_flesh", name: "腐肉", cat: "food", nameEn: "Rotten Flesh" },
  { id: "beef", name: "生牛肉", cat: "food", nameEn: "Raw Beef" },
  { id: "porkchop", name: "生猪排", cat: "food", nameEn: "Raw Porkchop" },
  { id: "chicken", name: "生鸡肉", cat: "food", nameEn: "Raw Chicken" },
  { id: "mutton", name: "生羊肉", cat: "food", nameEn: "Raw Mutton" },
  { id: "rabbit", name: "生兔肉", cat: "food", nameEn: "Raw Rabbit" },
  { id: "cod", name: "生鳕鱼", cat: "food", nameEn: "Raw Cod" },
  { id: "salmon", name: "生鲑鱼", cat: "food", nameEn: "Raw Salmon" },
  { id: "tropical_fish", name: "热带鱼", cat: "food", nameEn: "Tropical Fish" },
  { id: "pufferfish", name: "河豚", cat: "food", nameEn: "Pufferfish" },
  { id: "suspicious_stew", name: "迷之炖菜", cat: "food", nameEn: "Suspicious Stew" },
  { id: "glistering_melon_slice", name: "闪烁的西瓜片", cat: "material", nameEn: "Glistering Melon Slice" },
  { id: "nether_wart", name: "下界疣", cat: "material", nameEn: "Nether Wart" },
  { id: "clay_ball", name: "黏土球", cat: "material", nameEn: "Clay Ball" },
  { id: "brick", name: "红砖", cat: "material", nameEn: "Brick" },
  { id: "gravel", name: "砂砾", cat: "block", nameEn: "Gravel" },
  { id: "andesite", name: "安山岩", cat: "building", nameEn: "Andesite" },
  { id: "polished_andesite", name: "磨制安山岩", cat: "building", nameEn: "Polished Andesite" },
  { id: "polished_diorite", name: "磨制闪长岩", cat: "building", nameEn: "Polished Diorite" },
  { id: "polished_granite", name: "磨制花岗岩", cat: "building", nameEn: "Polished Granite" },
  { id: "chiseled_stone_bricks", name: "錾制石砖", cat: "building", nameEn: "Chiseled Stone Bricks" },
  { id: "dripstone_block", name: "滴水石块", cat: "building", nameEn: "Dripstone Block" },
  { id: "quartz_pillar", name: "石英柱", cat: "building", nameEn: "Quartz Pillar" },
  { id: "glowstone", name: "荧石", cat: "block", nameEn: "Glowstone" },
  { id: "dried_kelp_block", name: "干海带块", cat: "block", nameEn: "Dried Kelp Block" },
  { id: "bell", name: "钟", cat: "misc", nameEn: "Bell" },
  { id: "saddle", name: "鞍", cat: "misc", nameEn: "Saddle" },
  { id: "enchanted_book", name: "附魔书", cat: "misc", nameEn: "Enchanted Book" },
  { id: "filled_map", name: "已填充地图", cat: "misc", nameEn: "Map" },
  { id: "item_frame", name: "物品展示框", cat: "misc", nameEn: "Item Frame" },
  { id: "painting", name: "画", cat: "misc", nameEn: "Painting" },
  { id: "glass_bottle", name: "玻璃瓶", cat: "misc", nameEn: "Glass Bottle" },
  { id: "oak_boat", name: "橡木船", cat: "misc", nameEn: "Oak Boat" },
  { id: "cod_bucket", name: "鳕鱼桶", cat: "misc", nameEn: "Bucket of Cod" },
  { id: "turtle_scute", name: "鳞甲", cat: "material", nameEn: "Turtle Scute" },
  { id: "leather_horse_armor", name: "皮革马铠", cat: "misc", nameEn: "Leather Horse Armor" },
  { id: "globe_banner_pattern", name: "地球旗帜图案", cat: "misc", nameEn: "Globe Banner Pattern" },
  { id: "brown_wool", name: "棕色羊毛", cat: "building", nameEn: "Brown Wool" },
  { id: "gray_wool", name: "灰色羊毛", cat: "building", nameEn: "Gray Wool" },
  { id: "white_wool", name: "白色羊毛", cat: "building", nameEn: "White Wool" },
  { id: "white_carpet", name: "白色地毯", cat: "building", nameEn: "White Carpet" },
  { id: "white_bed", name: "白色床", cat: "misc", nameEn: "White Bed" },
  { id: "white_banner", name: "白色旗帜", cat: "misc", nameEn: "White Banner" },
  { id: "white_terracotta", name: "白色陶瓦", cat: "building", nameEn: "White Terracotta" },
  { id: "white_glazed_terracotta", name: "白色带釉陶瓦", cat: "building", nameEn: "White Glazed Terracotta" },
  { id: "red_candle", name: "红色蜡烛", cat: "misc", nameEn: "Red Candle" },
  { id: "yellow_candle", name: "黄色蜡烛", cat: "misc", nameEn: "Yellow Candle" },
  { id: "white_dye", name: "白色染料", cat: "material", nameEn: "White Dye" },
  { id: "gray_dye", name: "灰色染料", cat: "material", nameEn: "Gray Dye" },
  { id: "light_gray_dye", name: "淡灰色染料", cat: "material", nameEn: "Light Gray Dye" },
  { id: "black_dye", name: "黑色染料", cat: "material", nameEn: "Black Dye" },
  { id: "brown_dye", name: "棕色染料", cat: "material", nameEn: "Brown Dye" },
  { id: "red_dye", name: "红色染料", cat: "material", nameEn: "Red Dye" },
  { id: "orange_dye", name: "橙色染料", cat: "material", nameEn: "Orange Dye" },
  { id: "yellow_dye", name: "黄色染料", cat: "material", nameEn: "Yellow Dye" },
  { id: "lime_dye", name: "黄绿色染料", cat: "material", nameEn: "Lime Dye" },
  { id: "green_dye", name: "绿色染料", cat: "material", nameEn: "Green Dye" },
  { id: "cyan_dye", name: "青色染料", cat: "material", nameEn: "Cyan Dye" },
  { id: "light_blue_dye", name: "淡蓝色染料", cat: "material", nameEn: "Light Blue Dye" },
  { id: "blue_dye", name: "蓝色染料", cat: "material", nameEn: "Blue Dye" },
  { id: "purple_dye", name: "紫色染料", cat: "material", nameEn: "Purple Dye" },
  { id: "magenta_dye", name: "品红色染料", cat: "material", nameEn: "Magenta Dye" },
  { id: "pink_dye", name: "粉红色染料", cat: "material", nameEn: "Pink Dye" },
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
  /** 英文下拉显示。 */
  labelEn: string;
  /** 区间本身（用于输出标题）。 */
  range: string;
}

/** 按当前界面语言取版本区间下拉名。 */
export function versionLabel(option: GiveVersionOption): string {
  return currentLocale.value === "en-US" ? option.labelEn : option.label;
}

export const GIVE_VERSION_OPTIONS: GiveVersionOption[] = [
  { value: "1.7.10", label: "1.7.10（NBT 最旧）", labelEn: "1.7.10 (oldest NBT)", range: "1.7.10" },
  { value: "1.8-1.8.9", label: "1.8–1.8.9（NBT）", labelEn: "1.8–1.8.9 (NBT)", range: "1.8–1.8.9" },
  { value: "1.9-1.10.2", label: "1.9–1.10.2（NBT）", labelEn: "1.9–1.10.2 (NBT)", range: "1.9–1.10.2" },
  { value: "1.11-1.11.2", label: "1.11–1.11.2（NBT）", labelEn: "1.11–1.11.2 (NBT)", range: "1.11–1.11.2" },
  { value: "1.12.2", label: "1.12.2（NBT 旧版）", labelEn: "1.12.2 (legacy NBT)", range: "1.12.2" },
  { value: "1.13-1.15", label: "1.13–1.15（NBT）", labelEn: "1.13–1.15 (NBT)", range: "1.13–1.15" },
  { value: "1.16-1.18.2", label: "1.16–1.18.2（NBT）", labelEn: "1.16–1.18.2 (NBT)", range: "1.16–1.18.2" },
  { value: "1.19-1.20.4", label: "1.19–1.20.4（NBT）", labelEn: "1.19–1.20.4 (NBT)", range: "1.19–1.20.4" },
  { value: "1.20.5-1.20.6", label: "1.20.5–1.20.6（组件）", labelEn: "1.20.5–1.20.6 (components)", range: "1.20.5–1.20.6" },
  { value: "1.21-1.21.1", label: "1.21–1.21.1（组件）", labelEn: "1.21–1.21.1 (components)", range: "1.21–1.21.1" },
  { value: "1.21.2-1.21.4", label: "1.21.2–1.21.4（组件）", labelEn: "1.21.2–1.21.4 (components)", range: "1.21.2–1.21.4" },
  { value: "1.21.5+", label: "1.21.5+（组件）", labelEn: "1.21.5+ (components)", range: "1.21.5+" },
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
  /** 官方英文名（缺失时展示 id）。 */
  nameEn?: string;
  max: number;
  /** 1.13 前 NBT 数字 ID（1.13+ 不用）。 */
  numId?: number;
  /** 引入主版本，缺省 7（1.7.10 可用的经典附魔）。 */
  since?: number;
}

/** 附魔 ID（1.13+ 命名空间；1.13 前使用 numId）。 */
export const ENCHANTMENTS: GiveEnchant[] = [
  { id: "sharpness", name: "锋利", max: 5, numId: 16, nameEn: "Sharpness" },
  { id: "smite", name: "亡灵杀手", max: 5, numId: 17, nameEn: "Smite" },
  { id: "bane_of_arthropods", name: "节肢杀手", max: 5, numId: 18, nameEn: "Bane of Arthropods" },
  { id: "knockback", name: "击退", max: 2, numId: 19, nameEn: "Knockback" },
  { id: "fire_aspect", name: "火焰附加", max: 2, numId: 20, nameEn: "Fire Aspect" },
  { id: "looting", name: "抢夺", max: 3, numId: 21, nameEn: "Looting" },
  { id: "sweeping_edge", name: "横扫之刃", max: 3, numId: 22, since: 11.1, nameEn: "Sweeping Edge" },
  { id: "efficiency", name: "效率", max: 5, numId: 32, nameEn: "Efficiency" },
  { id: "silk_touch", name: "精准采集", max: 1, numId: 33, nameEn: "Silk Touch" },
  { id: "fortune", name: "时运", max: 3, numId: 35, nameEn: "Fortune" },
  { id: "protection", name: "保护", max: 4, numId: 0, nameEn: "Protection" },
  { id: "fire_protection", name: "火焰保护", max: 4, numId: 1, nameEn: "Fire Protection" },
  { id: "blast_protection", name: "爆炸保护", max: 4, numId: 3, nameEn: "Blast Protection" },
  { id: "projectile_protection", name: "弹射物保护", max: 4, numId: 4, nameEn: "Projectile Protection" },
  { id: "respiration", name: "水下呼吸", max: 3, numId: 5, nameEn: "Respiration" },
  { id: "aqua_affinity", name: "水下速掘", max: 1, numId: 6, nameEn: "Aqua Affinity" },
  { id: "thorns", name: "荆棘", max: 3, numId: 7, nameEn: "Thorns" },
  { id: "depth_strider", name: "深海探索者", max: 3, numId: 8, since: 8, nameEn: "Depth Strider" },
  { id: "frost_walker", name: "冰霜行者", max: 2, numId: 9, since: 9, nameEn: "Frost Walker" },
  { id: "soul_speed", name: "灵魂疾行", max: 3 , since: 16, nameEn: "Soul Speed" },
  { id: "swift_sneak", name: "迅捷潜行", max: 3 , since: 19, nameEn: "Swift Sneak" },
  { id: "feather_falling", name: "摔落缓冲", max: 4, numId: 2, nameEn: "Feather Falling" },
  { id: "punch", name: "冲击", max: 2, numId: 49, nameEn: "Punch" },
  { id: "power", name: "力量", max: 5, numId: 48, nameEn: "Power" },
  { id: "flame", name: "火焰", max: 1, numId: 50, nameEn: "Flame" },
  { id: "infinity", name: "无限", max: 1, numId: 51, nameEn: "Infinity" },
  { id: "luck_of_the_sea", name: "海之眷顾", max: 3, numId: 61, nameEn: "Luck of the Sea" },
  { id: "lure", name: "饵钓", max: 3, numId: 62, nameEn: "Lure" },
  { id: "mending", name: "经验修补", max: 1, numId: 70, since: 9, nameEn: "Mending" },
  { id: "unbreaking", name: "耐久", max: 3, numId: 34, nameEn: "Unbreaking" },
  // 1.13+ 的命名空间 ID 与 1.11–1.12 的数字 ID 不同
  { id: "binding_curse", name: "绑定诅咒", max: 1, numId: 10, since: 11, nameEn: "Curse of Binding" },
  { id: "vanishing_curse", name: "消失诅咒", max: 1, numId: 71, since: 11, nameEn: "Curse of Vanishing" },
  { id: "multishot", name: "多重射击", max: 1 , since: 14, nameEn: "Multishot" },
  { id: "piercing", name: "穿透", max: 4 , since: 14, nameEn: "Piercing" },
  { id: "quick_charge", name: "快速装填", max: 3 , since: 14, nameEn: "Quick Charge" },
  { id: "channeling", name: "引雷", max: 1 , since: 13, nameEn: "Channeling" },
  { id: "riptide", name: "激流", max: 3 , since: 13, nameEn: "Riptide" },
  { id: "loyalty", name: "忠诚", max: 3 , since: 13, nameEn: "Loyalty" },
  { id: "impaling", name: "穿刺", max: 5 , since: 13, nameEn: "Impaling" },
  { id: "density", name: "致密", max: 5 , since: 21, nameEn: "Density" },
  { id: "breach", name: "破甲", max: 4 , since: 21, nameEn: "Breach" },
  { id: "wind_burst", name: "风爆", max: 3 , since: 21, nameEn: "Wind Burst" },
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
  /** 官方英文名（缺失时展示 id）。 */
  nameEn?: string;
  /** 引入版本：20.5 = 1.20.5；缺省 7（1.7.10 可用的经典属性）。 */
  since?: number;
}

/** 按当前界面语言取附魔名；英文缺失时回退到注册 id。 */
export function enchantName(id: string): string {
  const ench = ENCHANTMENTS.find((e) => e.id === id);
  return currentLocale.value === "en-US" ? ench?.nameEn ?? id : ench?.name ?? id;
}

/** 按当前界面语言取属性名；英文缺失时回退到注册 id。 */
export function attributeName(id: string): string {
  const attr = ATTRIBUTES.find((a) => a.id === id);
  return currentLocale.value === "en-US" ? attr?.nameEn ?? id : attr?.name ?? id;
}

export const ATTRIBUTES: GiveAttribute[] = [
  { id: "generic.attack_damage", name: "攻击伤害", nameEn: "Attack Damage" },
  { id: "generic.attack_speed", name: "攻击速度", since: 9, nameEn: "Attack Speed" },
  { id: "generic.armor", name: "护甲值", since: 9, nameEn: "Armor" },
  { id: "generic.armor_toughness", name: "护甲韧性", since: 9.1, nameEn: "Armor Toughness" },
  { id: "generic.max_health", name: "最大生命值", nameEn: "Max Health" },
  { id: "generic.movement_speed", name: "移动速度", nameEn: "Movement Speed" },
  { id: "generic.knockback_resistance", name: "击退抗性", nameEn: "Knockback Resistance" },
  { id: "generic.luck", name: "幸运", since: 9, nameEn: "Luck" },
  { id: "generic.follow_range", name: "跟随范围", nameEn: "Follow Range" },
  { id: "generic.scale", name: "体型", since: 20.5, nameEn: "Scale" },
  { id: "generic.step_height", name: "步高", since: 20.5, nameEn: "Step Height" },
  { id: "generic.gravity", name: "重力", since: 20.5, nameEn: "Gravity" },
  { id: "generic.safe_fall_distance", name: "安全摔落距离", since: 20.5, nameEn: "Safe Fall Distance" },
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
