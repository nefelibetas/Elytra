// 整合包版本图标（versions/{dir}/pack-icon.png 懒加载缓存）：安装期后端持久化平台 logo，此处按需读转 data URL，缺失/失败保持占位。
import { invoke } from "@tauri-apps/api/core";
import { createBoundedCache } from "../../utils/boundedCache";

/** 版本目录名 → 图标 data URL（懒加载缓存）。单图上限见后端 1MiB；上限 128 按 FIFO 淘汰防无界增长。 */
const versionIconsCache = createBoundedCache<string>(128);
export const versionIcons = versionIconsCache.record;
const loadingCache = createBoundedCache<boolean>(256);
export async function loadVersionIcon(versionDirName: string) {
  if (versionIcons[versionDirName] || loadingCache.record[versionDirName]) return;
  loadingCache.put(versionDirName, true);
  try {
    // 后端返回 { mime, base64 } 载荷（魔数嗅探 + 服务端编码，替代 JSON 数字数组）
    const icon = await invoke<{ mime: string; data: string } | null>("get_version_icon", {
      versionDirName,
    });
    if (icon) {
      versionIconsCache.put(versionDirName, `data:${icon.mime};base64,${icon.data}`);
    }
  } catch {
    // 缺失/读取失败：保持 Map 占位
  } finally {
    loadingCache.put(versionDirName, false);
  }
}

/** 图标来源：custom=用户自定义 / pack=整合包项目图 / default=无（loader 图标）。 */
export type VersionIconSource = "custom" | "pack" | "default";
/** 版本目录名 → 当前图标来源（懒加载缓存，上限 256，与图标缓存同键空间）。 */
const iconSourcesCache = createBoundedCache<VersionIconSource>(256);
export const iconSources = iconSourcesCache.record;

export async function loadIconSource(versionDirName: string): Promise<VersionIconSource> {
  try {
    const info = await invoke<{ source: VersionIconSource }>("get_version_icon_source", {
      versionDirName,
    });
    iconSourcesCache.put(versionDirName, info.source);
    return info.source;
  } catch {
    iconSourcesCache.put(versionDirName, "default");
    return "default";
  }
}

async function refresh(versionDirName: string) {
  versionIconsCache.drop(versionDirName);
  await Promise.all([loadVersionIcon(versionDirName), loadIconSource(versionDirName)]);
}

export async function setVersionIcon(versionDirName: string, sourcePath: string) {
  await invoke("set_version_icon", { versionDirName, sourcePath });
  await refresh(versionDirName);
}

export async function resetVersionIcon(versionDirName: string) {
  await invoke("reset_version_icon", { versionDirName });
  await refresh(versionDirName);
}
