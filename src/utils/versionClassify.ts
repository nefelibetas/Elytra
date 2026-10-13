/** 版本目录名解析，命名规则 {mc}-{loader}-{ver}（如 1.18.2-forge-40.1.0）；元数据见 versionMeta.ts。 */
import { LOADER_KEYS, LOADER_TITLES } from "./versionMeta";

export interface VersionClass {
  category: string;
  categoryTitle: string;
  /** loader 版本号（原版为空串） */
  loaderVersion: string;
  /** Minecraft 版本号（原版时为整个目录名） */
  mcVersion: string;
}

/** 先剥 `_数字` 冲突重命名后缀；loader 标记大小写不敏感，切片索引基于小写串（长度不变）。 */
export function classifyVersion(name: string): VersionClass {
  const base = name.replace(/_\d+$/, ""); // 冲突重命名 {id}_n → {id}
  const lower = base.toLowerCase();
  for (const key of LOADER_KEYS) {
    const marker = `-${key}-`;
    const idx = lower.indexOf(marker);
    if (idx > 0) {
      return {
        category: key,
        categoryTitle: LOADER_TITLES[key],
        loaderVersion: base.slice(idx + marker.length),
        mcVersion: base.slice(0, idx),
      };
    }
  }
  return {
    category: "vanilla",
    categoryTitle: LOADER_TITLES.vanilla,
    loaderVersion: "",
    mcVersion: base,
  };
}
