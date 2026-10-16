// NBT 文本片段的轻量工具：括号/引号配对校验与归一化。不做语义解析——片段原样透传给命令，
// 语法对错以游戏为准；这里只拦「括号没配平」这类必然出错的写法。

/**
 * 校验 NBT 片段是否括号/引号配平：返回 `null` 合法；否则给首个问题的中文描述。
 * 支持 `{}`/`[]` 嵌套、`"`/`'` 字符串（含 `\"` 转义）与未加引号的裸值。
 */
export function checkNbtFragment(raw: string): string | null {
	const stack: string[] = [];
	let quote: string | null = null;
	for (let i = 0; i < raw.length; i++) {
		const ch = raw[i];
		if (quote) {
			if (ch === "\\") {
				i++; // 跳过转义字符（如 \" 在字符串内）
				continue;
			}
			if (ch === quote) quote = null;
			continue;
		}
		if (ch === '"' || ch === "'") {
			quote = ch;
		} else if (ch === "{" || ch === "[") {
			stack.push(ch);
		} else if (ch === "}" || ch === "]") {
			const open = stack.pop();
			const want = ch === "}" ? "{" : "[";
			if (open !== want) {
				return `第 ${i + 1} 个字符处括号不匹配：见到「${ch}」但缺少「${want}」`;
			}
		}
	}
	if (quote) return `字符串引号「${quote}」未闭合`;
	if (stack.length > 0) {
		const missing = stack
			.map((c) => (c === "{" ? "}" : "]"))
			.reverse()
			.join("");
		return `括号未闭合，缺少「${missing}」`;
	}
	return null;
}

/**
 * 归一化附加片段：去首尾空白；容忍用户多包了一层花括号（`{k:v}` → `k:v`）。
 * 仅当首尾花括号互相配对时剥一层（`{a},{b}` 这类不剥）。返回空串表示无附加内容。
 */
export function normalizeNbtFragment(raw: string): string {
	let t = raw.trim();
	while (t.startsWith("{") && t.endsWith("}") && outerBracesMatch(t)) {
		t = t.slice(1, -1).trim();
	}
	return t;
}

/** 首字符 `{` 与末字符 `}` 是否为同一对（中间深度先归零则不是）。 */
function outerBracesMatch(s: string): boolean {
	let depth = 0;
	let quote: string | null = null;
	for (let i = 0; i < s.length; i++) {
		const ch = s[i];
		if (quote) {
			if (ch === "\\") i++;
			else if (ch === quote) quote = null;
			continue;
		}
		if (ch === '"' || ch === "'") quote = ch;
		else if (ch === "{") depth++;
		else if (ch === "}") {
			depth--;
			if (depth === 0) return i === s.length - 1;
		}
	}
	return false;
}
