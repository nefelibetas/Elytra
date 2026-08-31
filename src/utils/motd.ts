// Minecraft MOTD / 文本格式工具：§ 代码解析渲染、渐变生成、颜色表。
// 供 MOTD 生成器与服务器状态检测（MOTD 展示）共用。

/** MC 经典 16 色表（§ 代码 → CSS 色）。 */
export const MC_COLORS: Record<string, string> = {
  "0": "#000000", // black
  "1": "#0000AA", // dark_blue
  "2": "#00AA00", // dark_green
  "3": "#00AAAA", // dark_aqua
  "4": "#AA0000", // dark_red
  "5": "#AA00AA", // dark_purple
  "6": "#FFAA00", // gold
  "7": "#AAAAAA", // gray
  "8": "#555555", // dark_gray
  "9": "#5555FF", // blue
  a: "#55FF55", // green
  b: "#55FFFF", // aqua
  c: "#FF5555", // red
  d: "#FF55FF", // light_purple
  e: "#FFFF55", // yellow
  f: "#FFFFFF", // white
};

/** 16 色可读名（生成器色板 tooltip）。 */
export const MC_COLOR_NAMES: Record<string, string> = {
  "0": "黑色",
  "1": "深蓝色",
  "2": "深绿色",
  "3": "深青色",
  "4": "深红色",
  "5": "深紫色",
  "6": "金色",
  "7": "灰色",
  "8": "深灰色",
  "9": "蓝色",
  a: "绿色",
  b: "青色",
  c: "红色",
  d: "浅紫色",
  e: "黄色",
  f: "白色",
};

/** 格式码常量（样式类）。 */
export const MC_STYLES = {
  bold: "l",
  strikethrough: "m",
  underline: "n",
  italic: "o",
  obfuscated: "k",
  reset: "r",
} as const;

/** 渲染段：parseMotd 的输出，一段连续相同格式的文本。 */
export interface MotdSegment {
  text: string;
  /** CSS 颜色（§x 自定义色或经典 16 色映射）。 */
  color?: string;
  bold: boolean;
  italic: boolean;
  underline: boolean;
  strikethrough: boolean;
  /** 乱码（§k）：游戏内随机字符跳动，预览用闪烁模拟。 */
  obfuscated: boolean;
}

/** § 字符（兼容全角？MOTD 一律半角 §）。 */
const SECTION = "§";

/**
 * 解析 MOTD 文本（含 § 格式码）为渲染段序列。
 * 支持经典 16 色、样式码（l/m/n/o/k/r）与 1.16+ 自定义色 §x§R§R§G§G§B§B。
 */
export function parseMotd(input: string): MotdSegment[] {
  const segments: MotdSegment[] = [];
  let color: string | undefined;
  let bold = false;
  let italic = false;
  let underline = false;
  let strikethrough = false;
  let obfuscated = false;
  let buf = "";

  const flush = () => {
    if (buf) {
      segments.push({
        text: buf,
        color,
        bold,
        italic,
        underline,
        strikethrough,
        obfuscated,
      });
      buf = "";
    }
  };

  let i = 0;
  while (i < input.length) {
    const ch = input[i];
    if (ch !== SECTION || i + 1 >= input.length) {
      buf += ch;
      i += 1;
      continue;
    }
    const code = input[i + 1].toLowerCase();
    // §x：后面跟 §R§R§G§G§B§B（6 个 hex，均带 § 前缀，1.16+）
    if (code === "x") {
      const hex = input.slice(i + 2, i + 14); // 每字符 2 长度（§ + hex）
      const matches = [...hex.matchAll(/§([0-9a-fA-F])/g)];
      if (matches.length === 6) {
        flush();
        color = `#${matches.map((m) => m[1]).join("")}`;
        i += 14;
        continue;
      }
      // 格式不完整 → 当普通字符
      buf += ch;
      i += 1;
      continue;
    }
    flush();
    if (code >= "0" && code <= "9") {
      color = MC_COLORS[code];
    } else if (code >= "a" && code <= "f") {
      color = MC_COLORS[code];
    } else if (code === "l") {
      bold = true;
    } else if (code === "m") {
      strikethrough = true;
    } else if (code === "n") {
      underline = true;
    } else if (code === "o") {
      italic = true;
    } else if (code === "k") {
      obfuscated = true;
    } else if (code === "r") {
      color = undefined;
      bold = italic = underline = strikethrough = obfuscated = false;
    } else {
      // 未知代码：原样输出
      buf += ch;
      i += 1;
      continue;
    }
    i += 2;
  }
  flush();
  return segments;
}

/** 16 色 key 列表（生成器色板顺序：按 MC 惯例深色优先）。 */
export const MC_COLOR_KEYS = ["0", "1", "2", "3", "4", "5", "6", "7", "8", "9", "a", "b", "c", "d", "e", "f"];

/**
 * 生成 §x 渐变序列（1.16+）：对 text 每个可见字符插值 from→to 颜色。
 * - 已存在的 § 格式码序列（§c / §x+6 段）原样透传，不参与染色（否则格式码
 *   字符本身被渐变覆盖，原格式失效且文本膨胀）。
 * - 空白字符不染色（保持默认色，与主流 MOTD 工具一致）。
 */
export function gradientMotd(text: string, from: string, to: string): string {
  const start = hexToRgb(from);
  const end = hexToRgb(to);
  if (!start || !end) return text;

  // 第一遍：收集可见字符（跳过格式码序列），计算染色数量
  const visible: string[] = [];
  let i = 0;
  while (i < text.length) {
    if (text[i] === SECTION && i + 1 < text.length) {
      const skip = formatCodeLength(text, i);
      if (skip > 0) {
        i += skip;
        continue;
      }
    }
    if (!/\s/.test(text[i])) visible.push(text[i]);
    i += 1;
  }
  if (visible.length === 0) return text;

  // 第二遍：重建文本，可见字符逐一染色，格式码透传
  let idx = 0;
  let out = "";
  i = 0;
  while (i < text.length) {
    if (text[i] === SECTION && i + 1 < text.length) {
      const skip = formatCodeLength(text, i);
      if (skip > 0) {
        out += text.slice(i, i + skip);
        i += skip;
        continue;
      }
    }
    const c = text[i];
    if (/\s/.test(c)) {
      out += c;
      i += 1;
      continue;
    }
    const t = visible.length <= 1 ? 0 : idx / (visible.length - 1);
    const r = Math.round(start.r + (end.r - start.r) * t);
    const g = Math.round(start.g + (end.g - start.g) * t);
    const b = Math.round(start.b + (end.b - start.b) * t);
    const hex = [r, g, b].map((v) => v.toString(16).padStart(2, "0"));
    out += `§x§${hex[0][0]}§${hex[0][1]}§${hex[1][0]}§${hex[1][1]}§${hex[2][0]}§${hex[2][1]}`;
    out += c;
    idx += 1;
    i += 1;
  }
  return out;
}

/** § 格式码序列长度（§c = 2；§x+6 段 = 14；非法 = 0）。 */
function formatCodeLength(text: string, i: number): number {
  const code = text[i + 1].toLowerCase();
  if (code === "x") {
    const matches = [...text.slice(i + 2, i + 14).matchAll(/§([0-9a-fA-F])/g)];
    if (matches.length === 6) return 14;
    return 0; // §x 后跟非完整 6 段：按未知字符处理（与 parseMotd 一致）
  }
  if (/[0-9a-fk-or]/.test(code)) return 2;
  return 0;
}

function hexToRgb(hex: string): { r: number; g: number; b: number } | null {
  const m = /^#?([0-9a-fA-F]{6})$/.exec(hex.trim());
  if (!m) return null;
  const v = parseInt(m[1], 16);
  return { r: (v >> 16) & 0xff, g: (v >> 8) & 0xff, b: v & 0xff };
}

/** 十六进制色（#RRGGBB）→ §x 自定义颜色序列（1.16+，§x§R§R§G§G§B§B）。非法输入返回 null。 */
export function hexToSectionX(hex: string): string | null {
  const rgb = hexToRgb(hex);
  if (!rgb) return null;
  const h = [rgb.r, rgb.g, rgb.b].map((v) => v.toString(16).padStart(2, "0"));
  return `§x§${h[0][0]}§${h[0][1]}§${h[1][0]}§${h[1][1]}§${h[2][0]}§${h[2][1]}`;
}

/** server.properties 安全转义：§ → \u00A7（属性文件读取时还原为 §）。 */
export function escapeServerProperties(motd: string): string {
  return motd.replace(/§/g, "\\u00A7");
}

/** 统计可见字符数（不含 § 代码）。 */
export function visibleLength(motd: string): number {
  let count = 0;
  let i = 0;
  while (i < motd.length) {
    if (motd[i] === SECTION && i + 1 < motd.length) {
      const code = motd[i + 1].toLowerCase();
      if (code === "x") {
        const matches = [...motd.slice(i + 2, i + 14).matchAll(/§([0-9a-fA-F])/g)];
        if (matches.length === 6) {
          i += 14;
          continue;
        }
      }
      i += 2;
      continue;
    }
    count += 1;
    i += 1;
  }
  return count;
}
