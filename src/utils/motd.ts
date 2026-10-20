import { currentLocale } from "../i18n";

export const MC_COLORS: Record<string, string> = {
  "0": "#000000",
  "1": "#0000AA",
  "2": "#00AA00",
  "3": "#00AAAA",
  "4": "#AA0000",
  "5": "#AA00AA",
  "6": "#FFAA00",
  "7": "#AAAAAA",
  "8": "#555555",
  "9": "#5555FF",
  a: "#55FF55",
  b: "#55FFFF",
  c: "#FF5555",
  d: "#FF55FF",
  e: "#FFFF55",
  f: "#FFFFFF",
};

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

export const MC_COLOR_NAMES_EN: Record<string, string> = {
  "0": "Black",
  "1": "Dark Blue",
  "2": "Dark Green",
  "3": "Dark Aqua",
  "4": "Dark Red",
  "5": "Dark Purple",
  "6": "Gold",
  "7": "Gray",
  "8": "Dark Gray",
  "9": "Blue",
  a: "Green",
  b: "Aqua",
  c: "Red",
  d: "Light Purple",
  e: "Yellow",
  f: "White",
};

/** 按当前界面语言取格式码颜色名。 */
export function colorName(key: string): string {
  return currentLocale.value === "en-US"
    ? MC_COLOR_NAMES_EN[key] ?? key
    : MC_COLOR_NAMES[key] ?? key;
}

export const MC_STYLES = {
  bold: "l",
  strikethrough: "m",
  underline: "n",
  italic: "o",
  obfuscated: "k",
  reset: "r",
} as const;

/** 一段连续相同格式的文本。 */
export interface MotdSegment {
  text: string;
  color?: string;
  bold: boolean;
  italic: boolean;
  underline: boolean;
  strikethrough: boolean;
  /** 乱码（§k），预览以闪烁模拟。 */
  obfuscated: boolean;
}

const SECTION = "§";

/** 解析 § 格式码为渲染段：16 色、样式码 l/m/n/o/k/r、1.16+ §x§R§R§G§G§B§B。 */
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
    // §x 后跟 6 个带 § 前缀的 hex（1.16+）
    if (code === "x") {
      const hex = input.slice(i + 2, i + 14); // 每字符 2 长度
      const matches = [...hex.matchAll(/§([0-9a-fA-F])/g)];
      if (matches.length === 6) {
        flush();
        color = `#${matches.map((m) => m[1]).join("")}`;
        i += 14;
        continue;
      }
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
      buf += ch;
      i += 1;
      continue;
    }
    i += 2;
  }
  flush();
  return segments;
}

/** 16 色 key（深色优先）。 */
export const MC_COLOR_KEYS = ["0", "1", "2", "3", "4", "5", "6", "7", "8", "9", "a", "b", "c", "d", "e", "f"];

/** 生成 §x 渐变（1.16+）：对可见字符插值 from→to，空白不染色；已存在的 § 格式码原样透传（否则被覆盖失效）。 */
export function gradientMotd(text: string, from: string, to: string): string {
  const start = hexToRgb(from);
  const end = hexToRgb(to);
  if (!start || !end) return text;

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

/** § 格式码长度：§c=2，§x + 6 段=14，非法=0。 */
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

/** #RRGGBB → §x§R§R§G§G§B§B（1.16+）；非法输入返回 null。 */
export function hexToSectionX(hex: string): string | null {
  const rgb = hexToRgb(hex);
  if (!rgb) return null;
  const h = [rgb.r, rgb.g, rgb.b].map((v) => v.toString(16).padStart(2, "0"));
  return `§x§${h[0][0]}§${h[0][1]}§${h[1][0]}§${h[1][1]}§${h[2][0]}§${h[2][1]}`;
}

/** server.properties 转义：§ → \u00A7（读取时还原）。 */
export function escapeServerProperties(motd: string): string {
  return motd.replace(/§/g, "\\u00A7");
}

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
