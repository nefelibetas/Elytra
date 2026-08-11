// 通用格式化函数（各页面共用，勿在页面内重复定义）

/** 大数转中文单位：1.2万 / 3.4亿 */
export function fmtCount(n: number): string {
  if (n >= 100_000_000) return (n / 100_000_000).toFixed(1).replace(/\.0$/, "") + "亿";
  if (n >= 10_000) return (n / 10_000).toFixed(1).replace(/\.0$/, "") + "万";
  return String(n);
}

/** 字节转可读大小：1.5 MB / 32 KB */
export function fmtSize(bytes: number): string {
  if (!bytes) return "";
  if (bytes >= 1024 * 1024) return (bytes / 1024 / 1024).toFixed(1) + " MB";
  return Math.max(1, Math.round(bytes / 1024)) + " KB";
}

/** ISO 时间截取日期部分（Mod 版本发布日期展示用） */
export function fmtModDate(iso: string): string {
  return iso ? iso.substring(0, 10) : "";
}

/** ISO 时间转 "yyyy-MM-dd HH:mm:ss"（版本列表发布时间展示用） */
export function fmtDateTime(iso: string): string {
  if (!iso) return "";
  return iso.substring(0, 10) + " " + iso.substring(11, 19);
}
