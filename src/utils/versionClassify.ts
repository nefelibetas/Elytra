/**
 * 版本目录名解析工具（命名规则 {mc}-{loader}-{ver}，如 1.18.2-forge-40.1.0）。
 * VersionSelect / VersionSetup 共用。
 */

const LOADER_KEYS = [
  "neoforge",
  "forge",
  "fabric",
  "quilt",
  "optifine",
  "liteloader",
] as const;

const TITLES: Record<string, string> = {
  vanilla: "原版",
  forge: "Forge",
  neoforge: "NeoForge",
  fabric: "Fabric",
  quilt: "Quilt",
  optifine: "OptiFine",
  liteloader: "LiteLoader",
};

export interface VersionClass {
  category: string;
  categoryTitle: string;
  /** loader 版本号（原版为空串） */
  loaderVersion: string;
  /** Minecraft 版本号（原版时为整个目录名） */
  mcVersion: string;
}

export function classifyVersion(name: string): VersionClass {
  for (const key of LOADER_KEYS) {
    const marker = `-${key}-`;
    const idx = name.indexOf(marker);
    if (idx > 0) {
      return {
        category: key,
        categoryTitle: TITLES[key],
        loaderVersion: name.slice(idx + marker.length),
        mcVersion: name.slice(0, idx),
      };
    }
  }
  return {
    category: "vanilla",
    categoryTitle: "原版",
    loaderVersion: "",
    mcVersion: name,
  };
}
