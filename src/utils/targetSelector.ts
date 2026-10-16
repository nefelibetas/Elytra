// 目标选择器（@e[type=zombie,distance=..10,limit=1,sort=nearest]）的解析与序列化：
// 组合器 UI 的模型层——识别常用条件（type/distance/limit+sort/name/tag/gamemode），
// 不认识的键原样保留到 extra（手写条件不丢失），条件顺序稳定输出。

export type SelectorBase = "@e" | "@a" | "@p" | "@s" | "@r";

export interface NegatableText {
  value: string;
  /** `!` 前缀（排除）。 */
  negate: boolean;
}

export interface TargetSelector {
  base: SelectorBase;
  /** 实体类型（仅 @e 常用，允许其他选择器手填）。 */
  type: string;
  /** 距离范围：min/max 为 null 表示开区间（`..10` / `5..` / `5..10`）。 */
  distance: { min: number | null; max: number | null };
  /** 数量上限（0 = 未设置）。 */
  limit: number;
  /** nearest / furthest / random / arbitrary；空 = 未设置。 */
  sort: string;
  name: NegatableText;
  tag: NegatableText;
  /** 游戏模式（仅玩家选择器有意义）。 */
  gamemode: NegatableText;
  /** 未识别条件原文（按输入顺序保留）。 */
  extra: string[];
}

const BASE_RE = /^@[eaprs]\b/;

/** 解析选择器文本；不可识别（不以 @x 开头）时返回带默认 base 的空模型（原文进 extra）。 */
export function parseTargetSelector(text: string): TargetSelector {
  const t = text.trim();
  const out = emptyTarget();
  if (!BASE_RE.test(t)) {
    if (t) out.extra.push(t);
    return out;
  }
  out.base = t.slice(0, 2) as SelectorBase;
  const open = t.indexOf("[");
  if (open < 0) return out;
  const inner = t.slice(open + 1).replace(/\]\s*$/, "");
  for (const pair of splitConditions(inner)) {
    const eq = pair.indexOf("=");
    if (eq < 0) {
      out.extra.push(pair);
      continue;
    }
    const key = pair.slice(0, eq).trim();
    const raw = pair.slice(eq + 1).trim();
    switch (key) {
      case "type":
        out.type = raw;
        break;
      case "distance":
        out.distance = parseRange(raw);
        break;
      case "limit":
        out.limit = Math.max(0, Math.min(parseInt(raw, 10) || 0, 2147483647));
        break;
      case "sort":
        out.sort = raw;
        break;
      case "name":
        out.name = parseNegatable(raw);
        break;
      case "tag":
        out.tag = parseNegatable(raw);
        break;
      case "gamemode":
        out.gamemode = parseNegatable(raw);
        break;
      default:
        out.extra.push(`${key}=${raw}`);
    }
  }
  return out;
}

/** 序列化：固定条件顺序（type/distance/limit/sort/name/tag/gamemode）+ extra 原样附后。 */
export function serializeTargetSelector(sel: TargetSelector): string {
  const conds: string[] = [];
  if (sel.type.trim()) conds.push(`type=${sel.type.trim()}`);
  const dist = serializeRange(sel.distance);
  if (dist) conds.push(`distance=${dist}`);
  if (sel.limit > 0) conds.push(`limit=${Math.round(sel.limit)}`);
  if (sel.sort) conds.push(`sort=${sel.sort}`);
  if (sel.name.value.trim()) conds.push(`name=${negPrefix(sel.name)}${sel.name.value.trim()}`);
  if (sel.tag.value.trim()) conds.push(`tag=${negPrefix(sel.tag)}${sel.tag.value.trim()}`);
  if (sel.gamemode.value) conds.push(`gamemode=${negPrefix(sel.gamemode)}${sel.gamemode.value}`);
  conds.push(...sel.extra.map((x) => x.trim()).filter((x) => x));
  return conds.length > 0 ? `${sel.base}[${conds.join(",")}]` : sel.base;
}

function negPrefix(n: NegatableText): string {
  return n.negate ? "!" : "";
}

function parseNegatable(raw: string): NegatableText {
  if (raw.startsWith("!")) return { value: raw.slice(1).trim(), negate: true };
  return { value: raw, negate: false };
}

/** `..10` / `5..` / `5..10` → {min,max}；不可识别按双边无界。 */
function parseRange(raw: string): { min: number | null; max: number | null } {
  const t = raw.trim();
  if (t.startsWith("..")) {
    const max = parseInt(t.slice(2), 10);
    return { min: null, max: Number.isFinite(max) ? max : null };
  }
  if (t.endsWith("..")) {
    const min = parseInt(t.slice(0, -2), 10);
    return { min: Number.isFinite(min) ? min : null, max: null };
  }
  if (t.includes("..")) {
    const [a, b] = t.split("..");
    const min = parseInt(a, 10);
    const max = parseInt(b, 10);
    return {
      min: Number.isFinite(min) ? min : null,
      max: Number.isFinite(max) ? max : null,
    };
  }
  return { min: null, max: null };
}

function serializeRange(r: { min: number | null; max: number | null }): string {
  const hasMin = r.min !== null;
  const hasMax = r.max !== null;
  if (hasMin && hasMax) return `${r.min}..${r.max}`;
  if (hasMin) return `${r.min}..`;
  if (hasMax) return `..${r.max}`;
  return "";
}

/** 顶层逗号分裂（忽略 `{}` `[]` `""` `''` 内的逗号）——`nbt=` 等含对象的条件可安全手写。 */
function splitConditions(inner: string): string[] {
  const out: string[] = [];
  let depth = 0;
  let quote: string | null = null;
  let start = 0;
  for (let i = 0; i < inner.length; i++) {
    const ch = inner[i];
    if (quote) {
      if (ch === "\\") i++;
      else if (ch === quote) quote = null;
      continue;
    }
    if (ch === '"' || ch === "'") quote = ch;
    else if (ch === "{" || ch === "[") depth++;
    else if (ch === "}" || ch === "]") depth--;
    else if (ch === "," && depth === 0) {
      out.push(inner.slice(start, i).trim());
      start = i + 1;
    }
  }
  const tail = inner.slice(start).trim();
  if (tail) out.push(tail);
  return out.filter((x) => x.length > 0);
}

/** 每次调用返回一份独立模型（避免共享引用被 UI 改写）。 */
export function emptyTarget(): TargetSelector {
  return {
    base: "@e",
    type: "",
    distance: { min: null, max: null },
    limit: 0,
    sort: "",
    name: { value: "", negate: false },
    tag: { value: "", negate: false },
    gamemode: { value: "", negate: false },
    extra: [],
  };
}
