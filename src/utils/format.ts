// 通用格式化（各页面共用，勿重复定义）
import { currentLocale, t } from "../i18n";

let countLocale = "";
let countFormatter = new Intl.NumberFormat("zh-CN", {
  notation: "compact",
  maximumFractionDigits: 1,
});

export function fmtCount(n: number): string {
  if (currentLocale.value !== countLocale) {
    countLocale = currentLocale.value;
    countFormatter = new Intl.NumberFormat(countLocale, {
      notation: "compact",
      maximumFractionDigits: 1,
    });
  }
  return countFormatter.format(n);
}

export function fmtSize(bytes: number): string {
  if (!bytes) return "";
  if (bytes >= 1024 * 1024) return (bytes / 1024 / 1024).toFixed(1) + " MB";
  return Math.max(1, Math.round(bytes / 1024)) + " KB";
}

export function fmtModDate(iso: string): string {
  return iso ? iso.substring(0, 10) : "";
}

export function fmtDateTime(iso: string): string {
  if (!iso) return "";
  return iso.substring(0, 10) + " " + iso.substring(11, 19);
}

export function fmtSpeed(bps: number): string {
  if (bps >= 1_000_000) return (bps / 1_000_000).toFixed(2) + " MB/s";
  if (bps >= 1_000) return (bps / 1_000).toFixed(2) + " KB/s";
  return bps.toFixed(2) + " B/s";
}

/** 剩余秒数；null（停滞/速度过低）显示 —。 */
export function fmtEta(secs: number | null): string {
  if (secs == null) return "—";
  if (secs < 60) return t("task.eta.seconds", { count: secs });
  const m = Math.floor(secs / 60);
  if (m < 60) return t("task.eta.minutes", { m, s: secs % 60 });
  return t("task.eta.hours", { h: Math.floor(m / 60), m: m % 60 });
}

export function fmtUnix(sec: number): string {
  if (!sec) return "";
  const d = new Date(sec * 1000);
  const p = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}`;
}

export function fmtGb(mb: number): string {
  return (mb / 1024).toFixed(1) + "GB";
}

/** 标签位置，钳制到 4%–96% 避免贴边溢出。 */
export function memLabelLeft(pct: number): string {
  return Math.min(Math.max(pct, 4), 96) + "%";
}
