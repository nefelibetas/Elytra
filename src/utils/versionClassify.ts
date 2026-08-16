/**
 * 版本目录名解析工具（命名规则 {mc}-{loader}-{ver}，如 1.18.2-forge-40.1.0）。
 * VersionSelect / VersionSetup 共用。元数据（loader key/标题）来自 versionMeta.ts。
 */
import { LOADER_KEYS, LOADER_TITLES } from "./versionMeta";

export interface VersionClass {
  category: string;
  categoryTitle: string;
  /** loader 版本号（原版为空串） */
  loaderVersion: string;
  /** Minecraft 版本号（原版时为整个目录名） */
  mcVersion: string;
}

/**
 * 解析版本目录名。目录可能被冲突重命名（如 {id}_1），先剥掉 `_数字` 后缀再分类。
 * loader 标记大小写不敏感（PCL2 等外部启动器目录名可能是大写，如 26.1.2-Fabric），
 * 切片索引基于小写串（小写化不改变长度，索引与原串一致）。
 */
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
