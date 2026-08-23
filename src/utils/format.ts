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

/** 字节/秒 → 可读速度：12.34 MB/s / 3.45 KB/s / 1.23 B/s */
export function fmtSpeed(bps: number): string {
  if (bps >= 1_000_000) return (bps / 1_000_000).toFixed(2) + " MB/s";
  if (bps >= 1_000) return (bps / 1_000).toFixed(2) + " KB/s";
  return bps.toFixed(2) + " B/s";
}

/** 预计剩余时间格式化（秒）；null（停滞/速度过低）显示 —。 */
export function fmtEta(secs: number | null): string {
  if (secs == null) return "—";
  if (secs < 60) return `${secs} 秒`;
  const m = Math.floor(secs / 60);
  if (m < 60) return `${m} 分 ${secs % 60} 秒`;
  return `${Math.floor(m / 60)} 时 ${m % 60} 分`;
}

/** unix 秒 → "yyyy-MM-dd HH:mm"（存档最后游玩时间等）。 */
export function fmtUnix(sec: number): string {
  if (!sec) return "";
  const d = new Date(sec * 1000);
  const p = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}`;
}

/** MB → "x.y GB"（内存容量展示用）。 */
export function fmtGb(mb: number): string {
  return (mb / 1024).toFixed(1) + "GB";
}

/** 内存占用百分比 → 进度条文字标签位置（钳制在 4%–96%，避免贴边溢出）。 */
export function memLabelLeft(pct: number): string {
  return Math.min(Math.max(pct, 4), 96) + "%";
}
