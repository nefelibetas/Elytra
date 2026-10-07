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

/** 加载器 key 联合类型。 */
export type LoaderKey = (typeof LOADER_KEYS)[number];

/** 各加载器支持的最低 MC 版本（下载页据此过滤不可用加载器）。
 *  `Record<LoaderKey, string>` 约束键集：LOADER_KEYS 增删加载器时此处必须同步（否则类型报错）；
 *  保留索引签名以支持按任意 string key 查询（非加载器 key → undefined，调用方据此跳过过滤）。 */
export const LOADER_MIN_VERSION: Record<string, string> & Record<LoaderKey, string> = {
  neoforge: "1.20.2",
  forge: "1.1",
  fabric: "1.14",
  quilt: "1.14.4",
  optifine: "1.2.5",
};

/** 加载器展示键集：LOADER_KEYS 全部 + 原版（分类卡片需要原版条目）。 */
export type LoaderDisplayKey = LoaderKey | "vanilla";

export const LOADER_TITLES: Record<string, string> & Record<LoaderDisplayKey, string> = {
  vanilla: "原版",
  forge: "Forge",
  neoforge: "NeoForge",
  fabric: "Fabric",
  quilt: "Quilt",
  optifine: "OptiFine",
};

export const LOADER_ICONS: Record<string, string> & Record<LoaderDisplayKey, string> = {
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
