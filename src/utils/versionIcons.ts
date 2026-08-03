// 分类图标映射（VersionSelect 分类卡片 / VersionSetup 概览共用）。
// 数据源收敛到 versionMeta.ts，本文件仅保留兼容导出。
import { CATEGORIES } from "./versionMeta";

export const CATEGORY_ICONS: Record<string, string> = Object.fromEntries(
  CATEGORIES.map(c => [c.key, c.icon]),
);
