// 指令生成器数据表：游戏规则（常用子集，其余用自由输入兜底）。

/** 游戏规则：type 决定取值形态（true/false 或整数）。 */
export interface GameRule {
  name: string;
  label: string;
  /** 英文界面标签。 */
  labelEn: string;
  type: "bool" | "int";
  /** 默认值（展示提示用）。 */
  def: string;
}

/** 常用游戏规则（原版全集中的常用子集；其余可自由输入）。 */
export const GAMERULES: GameRule[] = [
  { name: "keepInventory", label: "死亡保留物品", labelEn: "Keep Inventory", type: "bool", def: "false" },
  { name: "mobGriefing", label: "生物破坏方块", labelEn: "Mob Griefing", type: "bool", def: "true" },
  { name: "doMobSpawning", label: "自然生成生物", labelEn: "Do Mob Spawning", type: "bool", def: "true" },
  { name: "doMobLoot", label: "生物掉落物", labelEn: "Do Mob Loot", type: "bool", def: "true" },
  { name: "doTileDrops", label: "方块掉落物", labelEn: "Do Tile Drops", type: "bool", def: "true" },
  { name: "doEntityDrops", label: "实体掉落物", labelEn: "Do Entity Drops", type: "bool", def: "true" },
  { name: "doDaylightCycle", label: "昼夜循环", labelEn: "Do Daylight Cycle", type: "bool", def: "true" },
  { name: "doWeatherCycle", label: "天气循环", labelEn: "Do Weather Cycle", type: "bool", def: "true" },
  { name: "doFireTick", label: "火焰蔓延", labelEn: "Do Fire Tick", type: "bool", def: "true" },
  { name: "doInsomnia", label: "幻翼生成", labelEn: "Do Insomnia", type: "bool", def: "true" },
  { name: "doImmediateRespawn", label: "立即重生", labelEn: "Do Immediate Respawn", type: "bool", def: "false" },
  { name: "doPatrolSpawning", label: "巡逻队生成", labelEn: "Do Patrol Spawning", type: "bool", def: "true" },
  { name: "doTraderSpawning", label: "流浪商人生成", labelEn: "Do Trader Spawning", type: "bool", def: "true" },
  { name: "doWardenSpawning", label: "监守者生成", labelEn: "Do Warden Spawning", type: "bool", def: "true" },
  { name: "doVinesSpread", label: "藤蔓蔓延", labelEn: "Do Vines Spread", type: "bool", def: "true" },
  { name: "doLimitedCrafting", label: "仅限配方书合成", labelEn: "Do Limited Crafting", type: "bool", def: "false" },
  { name: "naturalRegeneration", label: "自然回血", labelEn: "Natural Regeneration", type: "bool", def: "true" },
  { name: "fallDamage", label: "摔落伤害", labelEn: "Fall Damage", type: "bool", def: "true" },
  { name: "fireDamage", label: "火焰伤害", labelEn: "Fire Damage", type: "bool", def: "true" },
  { name: "drowningDamage", label: "溺水伤害", labelEn: "Drowning Damage", type: "bool", def: "true" },
  { name: "freezingDamage", label: "冰冻伤害", labelEn: "Freezing Damage", type: "bool", def: "true" },
  { name: "ignoreSleepingPlayers", label: "跳过睡眠需求", labelEn: "Ignore Sleeping Players", type: "bool", def: "false" },
  { name: "showDeathMessages", label: "死亡消息", labelEn: "Show Death Messages", type: "bool", def: "true" },
  { name: "announceAdvancements", label: "进度播报", labelEn: "Announce Advancements", type: "bool", def: "true" },
  { name: "sendCommandFeedback", label: "命令反馈", labelEn: "Send Command Feedback", type: "bool", def: "true" },
  { name: "commandBlockOutput", label: "命令方块输出", labelEn: "Command Block Output", type: "bool", def: "true" },
  { name: "logAdminCommands", label: "记录管理员命令", labelEn: "Log Admin Commands", type: "bool", def: "true" },
  { name: "reducedDebugInfo", label: "简化调试信息", labelEn: "Reduced Debug Info", type: "bool", def: "false" },
  { name: "spectatorsGenerateChunks", label: "旁观者加载区块", labelEn: "Spectators Generate Chunks", type: "bool", def: "true" },
  { name: "disableElytraMovementCheck", label: "禁用鞘翅检查", labelEn: "Disable Elytra Movement Check", type: "bool", def: "false" },
  { name: "disableRaids", label: "禁用袭击", labelEn: "Disable Raids", type: "bool", def: "false" },
  { name: "forgiveDeadPlayers", label: "死亡后原谅玩家", labelEn: "Forgive Dead Players", type: "bool", def: "true" },
  { name: "universalAnger", label: "通用愤怒", labelEn: "Universal Anger", type: "bool", def: "false" },
  { name: "globalSoundEvents", label: "全局音效", labelEn: "Global Sound Events", type: "bool", def: "true" },
  { name: "projectilesCanBreakBlocks", label: "弹射物破坏方块", labelEn: "Projectiles Can Break Blocks", type: "bool", def: "true" },
  { name: "elytraMovementCheck", label: "鞘翅移动检查", labelEn: "Elytra Movement Check", type: "bool", def: "true" },
  { name: "tntExplosionDropDecay", label: "TNT 爆炸掉落衰减", labelEn: "TNT Explosion Drop Decay", type: "bool", def: "false" },
  { name: "mobExplosionDropDecay", label: "生物爆炸掉落衰减", labelEn: "Mob Explosion Drop Decay", type: "bool", def: "true" },
  { name: "blockExplosionDropDecay", label: "方块爆炸掉落衰减", labelEn: "Block Explosion Drop Decay", type: "bool", def: "true" },
  { name: "waterSourceConversion", label: "水方块转化", labelEn: "Water Source Conversion", type: "bool", def: "true" },
  { name: "lavaSourceConversion", label: "岩浆方块转化", labelEn: "Lava Source Conversion", type: "bool", def: "false" },
  { name: "randomTickSpeed", label: "随机刻速度", labelEn: "Random Tick Speed", type: "int", def: "3" },
  { name: "spawnRadius", label: "出生点半径", labelEn: "Spawn Radius", type: "int", def: "10" },
  { name: "maxEntityCramming", label: "实体挤压上限", labelEn: "Max Entity Cramming", type: "int", def: "24" },
  { name: "playersSleepingPercentage", label: "跳过夜晚所需玩家百分比", labelEn: "Players Sleeping Percentage", type: "int", def: "100" },
  { name: "maxCommandChainLength", label: "命令链长度上限", labelEn: "Max Command Chain Length", type: "int", def: "65536" },
  { name: "maxCommandForkCount", label: "命令分叉上限", labelEn: "Max Command Fork Count", type: "int", def: "65536" },
  { name: "commandModificationBlockLimit", label: "命令方块修改上限", labelEn: "Command Modification Block Limit", type: "int", def: "32768" },
  { name: "snowAccumulationHeight", label: "积雪高度", labelEn: "Snow Accumulation Height", type: "int", def: "1" },
];
