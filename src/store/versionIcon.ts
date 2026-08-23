// 整合包版本图标（versions/{dir}/pack-icon.png 懒加载缓存）。
// 安装期后端把平台项目 logo 持久化到版本目录；此处按需读取转 data URL，
// 缺失/失败保持 Map 占位（UI 回退 loader 图标）。模式对齐 versionDetail.ts 的世界缩略图。
import { reactive } from "vue";
import { invoke } from "@tauri-apps/api/core";

/** 版本目录名 → 图标 data URL（懒加载缓存）。 */
export const versionIcons = reactive<Record<string, string>>({});
/** 加载中标记（幂等，防并发重复请求）。 */
const loading = reactive<Record<string, boolean>>({});

/** 魔数嗅探图片 mime（平台图标可能是 png/webp/jpeg/gif，data URL mime 必须正确）。 */
function mimeOf(bytes: Uint8Array): string {
  if (bytes.length >= 8 && bytes[0] === 0x89 && bytes[1] === 0x50) return "image/png";
  if (bytes.length >= 3 && bytes[0] === 0xff && bytes[1] === 0xd8) return "image/jpeg";
  if (bytes.length >= 12 && bytes[0] === 0x52 && bytes[8] === 0x57 && bytes[9] === 0x45) return "image/webp";
  if (bytes.length >= 3 && bytes[0] === 0x47 && bytes[1] === 0x49) return "image/gif";
  return "image/png";
}

/** number[] → data URL（分块防大数组展开栈溢出，对齐 versionDetail.ts）。 */
function toDataUrl(bytes: number[]): string {
  const u8 = new Uint8Array(bytes);
  let bin = "";
  const CHUNK = 0x8000;
  for (let i = 0; i < u8.length; i += CHUNK) {
    bin += String.fromCharCode(...u8.subarray(i, i + CHUNK));
  }
  return `data:${mimeOf(u8)};base64,${btoa(bin)}`;
}

/** 懒加载单个版本图标（缺失/读取失败静默保持占位）。 */
export async function loadVersionIcon(versionDirName: string) {
  if (versionIcons[versionDirName] || loading[versionDirName]) return;
  loading[versionDirName] = true;
  try {
    const bytes = await invoke<number[] | null>("get_version_icon", { versionDirName });
    if (bytes && bytes.length > 0) {
      versionIcons[versionDirName] = toDataUrl(bytes);
    }
  } catch {
    // 缺失/读取失败：保持 Map 占位
  } finally {
    loading[versionDirName] = false;
  }
}

/** 图标来源：custom=用户自定义 / pack=整合包项目图 / default=无（loader 图标）。 */
export type VersionIconSource = "custom" | "pack" | "default";
/** 版本目录名 → 当前图标来源（懒加载缓存，设置页展示状态用）。 */
export const iconSources = reactive<Record<string, VersionIconSource>>({});

/** 查询图标来源（缺失按 default 占位）。 */
export async function loadIconSource(versionDirName: string): Promise<VersionIconSource> {
  try {
    const info = await invoke<{ source: VersionIconSource }>("get_version_icon_source", {
      versionDirName,
    });
    iconSources[versionDirName] = info.source;
    return info.source;
  } catch {
    iconSources[versionDirName] = "default";
    return "default";
  }
}

/** 失效缓存并重新加载（set/reset 后调用，卡片与设置页立即反映新图）。 */
async function refresh(versionDirName: string) {
  delete versionIcons[versionDirName];
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
