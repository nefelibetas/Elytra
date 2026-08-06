// 版本分类/加载器元数据单一数据源。
// 合并原先 versionClassify.TITLES、versionIcons.CATEGORY_ICONS、
// Download.vue loaders/loaderIcon、VersionSelect.vue CATEGORIES 四处平行表。
import grassIcon from "../assets/grass.png";
import forgeIcon from "../assets/forge.png";
import neoForgeIcon from "../assets/neoForge.png";
import fabricIcon from "../assets/Fabric.png";
import optifineIcon from "../assets/optifined.png";
import quiltIcon from "../assets/quilt.png";

/** 加载器 key（按目录名匹配优先级排序：neoforge 必须在 forge 前）。 */
export const LOADER_KEYS = [
  "neoforge",
  "forge",
  "fabric",
  "quilt",
  "optifine",
] as const;

export const LOADER_TITLES: Record<string, string> = {
  vanilla: "原版",
  forge: "Forge",
  neoforge: "NeoForge",
  fabric: "Fabric",
  quilt: "Quilt",
  optifine: "OptiFine",
};

export const LOADER_ICONS: Record<string, string> = {
  vanilla: grassIcon,
  forge: forgeIcon,
  neoforge: neoForgeIcon,
  fabric: fabricIcon,
  quilt: quiltIcon,
  optifine: optifineIcon,
};

export interface CategoryMeta {
  key: string;
  title: string;
  icon: string;
}

/** 全部分类（含原版）：VersionSelect 分类卡片、Download 加载器选择共用。 */
export const CATEGORIES: CategoryMeta[] = [
  { key: "vanilla", title: LOADER_TITLES.vanilla, icon: LOADER_ICONS.vanilla },
  ...LOADER_KEYS.map(k => ({ key: k, title: LOADER_TITLES[k], icon: LOADER_ICONS[k] })),
];
