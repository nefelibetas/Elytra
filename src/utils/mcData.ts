// 指令生成器数据表：游戏规则（常用子集，其余用自由输入兜底）。

/** 游戏规则：type 决定取值形态（true/false 或整数）。 */
export interface GameRule {
  name: string;
  label: string;
  type: "bool" | "int";
  /** 默认值（展示提示用）。 */
  def: string;
}

/** 常用游戏规则（原版全集中的常用子集；其余可自由输入）。 */
export const GAMERULES: GameRule[] = [
  { name: "keepInventory", label: "死亡保留物品", type: "bool", def: "false" },
  { name: "mobGriefing", label: "生物破坏方块", type: "bool", def: "true" },
  { name: "doMobSpawning", label: "自然生成生物", type: "bool", def: "true" },
  { name: "doMobLoot", label: "生物掉落物", type: "bool", def: "true" },
  { name: "doTileDrops", label: "方块掉落物", type: "bool", def: "true" },
  { name: "doEntityDrops", label: "实体掉落物", type: "bool", def: "true" },
  { name: "doDaylightCycle", label: "昼夜循环", type: "bool", def: "true" },
  { name: "doWeatherCycle", label: "天气循环", type: "bool", def: "true" },
  { name: "doFireTick", label: "火焰蔓延", type: "bool", def: "true" },
  { name: "doInsomnia", label: "幻翼生成", type: "bool", def: "true" },
  { name: "doImmediateRespawn", label: "立即重生", type: "bool", def: "false" },
  { name: "doPatrolSpawning", label: "巡逻队生成", type: "bool", def: "true" },
  { name: "doTraderSpawning", label: "流浪商人生成", type: "bool", def: "true" },
  { name: "doWardenSpawning", label: "监守者生成", type: "bool", def: "true" },
  { name: "doVinesSpread", label: "藤蔓蔓延", type: "bool", def: "true" },
  { name: "doLimitedCrafting", label: "仅限配方书合成", type: "bool", def: "false" },
  { name: "naturalRegeneration", label: "自然回血", type: "bool", def: "true" },
  { name: "fallDamage", label: "摔落伤害", type: "bool", def: "true" },
  { name: "fireDamage", label: "火焰伤害", type: "bool", def: "true" },
  { name: "drowningDamage", label: "溺水伤害", type: "bool", def: "true" },
  { name: "freezingDamage", label: "冰冻伤害", type: "bool", def: "true" },
  { name: "ignoreSleepingPlayers", label: "跳过睡眠需求", type: "bool", def: "false" },
  { name: "showDeathMessages", label: "死亡消息", type: "bool", def: "true" },
  { name: "announceAdvancements", label: "进度播报", type: "bool", def: "true" },
  { name: "sendCommandFeedback", label: "命令反馈", type: "bool", def: "true" },
  { name: "commandBlockOutput", label: "命令方块输出", type: "bool", def: "true" },
  { name: "logAdminCommands", label: "记录管理员命令", type: "bool", def: "true" },
  { name: "reducedDebugInfo", label: "简化调试信息", type: "bool", def: "false" },
  { name: "spectatorsGenerateChunks", label: "旁观者加载区块", type: "bool", def: "true" },
  { name: "disableElytraMovementCheck", label: "禁用鞘翅检查", type: "bool", def: "false" },
  { name: "disableRaids", label: "禁用袭击", type: "bool", def: "false" },
  { name: "forgiveDeadPlayers", label: "死亡后原谅玩家", type: "bool", def: "true" },
  { name: "universalAnger", label: "通用愤怒", type: "bool", def: "false" },
  { name: "globalSoundEvents", label: "全局音效", type: "bool", def: "true" },
  { name: "projectilesCanBreakBlocks", label: "弹射物破坏方块", type: "bool", def: "true" },
  { name: "elytraMovementCheck", label: "鞘翅移动检查", type: "bool", def: "true" },
  { name: "tntExplosionDropDecay", label: "TNT 爆炸掉落衰减", type: "bool", def: "false" },
  { name: "mobExplosionDropDecay", label: "生物爆炸掉落衰减", type: "bool", def: "true" },
  { name: "blockExplosionDropDecay", label: "方块爆炸掉落衰减", type: "bool", def: "true" },
  { name: "waterSourceConversion", label: "水方块转化", type: "bool", def: "true" },
  { name: "lavaSourceConversion", label: "岩浆方块转化", type: "bool", def: "false" },
  { name: "randomTickSpeed", label: "随机刻速度", type: "int", def: "3" },
  { name: "spawnRadius", label: "出生点半径", type: "int", def: "10" },
  { name: "maxEntityCramming", label: "实体挤压上限", type: "int", def: "24" },
  { name: "playersSleepingPercentage", label: "跳过夜晚所需玩家百分比", type: "int", def: "100" },
  { name: "maxCommandChainLength", label: "命令链长度上限", type: "int", def: "65536" },
  { name: "maxCommandForkCount", label: "命令分叉上限", type: "int", def: "65536" },
  { name: "commandModificationBlockLimit", label: "命令方块修改上限", type: "int", def: "32768" },
  { name: "snowAccumulationHeight", label: "积雪高度", type: "int", def: "1" },
];
