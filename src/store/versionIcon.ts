// 整合包版本图标（versions/{dir}/pack-icon.png 懒加载缓存）。
// 安装期后端把平台项目 logo 持久化到版本目录；此处按需读取转 data URL，
// 缺失/失败保持 Map 占位（UI 回退 loader 图标）。模式对齐 versionDetail.ts 的世界缩略图。
import { invoke } from "@tauri-apps/api/core";
import { createBoundedCache } from "../utils/boundedCache";

/** 版本目录名 → 图标 data URL（懒加载缓存）。
 *  条目为 base64 data URL（后端单图上限 1MiB，典型 pack-icon 约 10~60KB）；
 *  上限 128：远大于同屏渲染的已安装版本数（版本列表一次性渲染全部已安装版本），
 *  超出后按 FIFO 淘汰最早写入的图标，避免长期会话随浏览版本数无界增长。 */
const versionIconsCache = createBoundedCache<string>(128);
export const versionIcons = versionIconsCache.record;
/** 加载中标记（幂等，防并发重复请求；有界，键与图标缓存同空间）。 */
const loadingCache = createBoundedCache<boolean>(256);
/** 懒加载单个版本图标（缺失/读取失败静默保持占位）。 */
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
/** 版本目录名 → 当前图标来源（懒加载缓存，设置页展示状态用）。
 *  条目为短枚举字符串，上限放宽到 256（与图标缓存同键空间，不早于图标被淘汰）。 */
const iconSourcesCache = createBoundedCache<VersionIconSource>(256);
export const iconSources = iconSourcesCache.record;

/** 查询图标来源（缺失按 default 占位）。 */
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

/** 失效缓存并重新加载（set/reset 后调用，卡片与设置页立即反映新图）。 */
async function refresh(versionDirName: string) {
  versionIconsCache.drop(versionDirName);
  await Promise.all([loadVersionIcon(versionDirName), loadIconSource(versionDirName)]);
}

/** 设置自定义图标（本地图片路径 → 后端魔数校验后复制为 version-icon.png）。 */
export async function setVersionIcon(versionDirName: string, sourcePath: string) {
  await invoke("set_version_icon", { versionDirName, sourcePath });
  await refresh(versionDirName);
}

/** 还原为默认图标（删除自定义；整合包项目图不受影响）。 */
export async function resetVersionIcon(versionDirName: string) {
  await invoke("reset_version_icon", { versionDirName });
  await refresh(versionDirName);
}
