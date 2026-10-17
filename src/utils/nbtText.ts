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

/** 顶层逗号分裂：忽略 `{}` `[]` `""` `''` 内的逗号（仅服务物品栈片段的浅层解析，不做完整 NBT 语义）。 */
export function splitTopLevel(s: string): string[] {
	const out: string[] = [];
	let depth = 0;
	let quote: string | null = null;
	let start = 0;
	for (let i = 0; i < s.length; i++) {
		const ch = s[i];
		if (quote) {
			if (ch === "\\") i++;
			else if (ch === quote) quote = null;
			continue;
		}
		if (ch === '"' || ch === "'") {
			quote = ch;
			continue;
		}
		if (ch === "{" || ch === "[") depth++;
		else if (ch === "}" || ch === "]") depth--;
		else if (ch === "," && depth === 0) {
			out.push(s.slice(start, i));
			start = i + 1;
		}
	}
	out.push(s.slice(start));
	return out.map((x) => x.trim()).filter((x) => x.length > 0);
}

/** 顶层首个冒号分裂（键/值）；无顶层冒号返回 null。 */
export function splitTopLevelPair(pair: string): [string, string] | null {
	let depth = 0;
	let quote: string | null = null;
	for (let i = 0; i < pair.length; i++) {
		const ch = pair[i];
		if (quote) {
			if (ch === "\\") i++;
			else if (ch === quote) quote = null;
			continue;
		}
		if (ch === '"' || ch === "'") quote = ch;
		else if (ch === "{" || ch === "[") depth++;
		else if (ch === "}" || ch === "]") depth--;
		else if (ch === ":" && depth === 0) {
			return [pair.slice(0, i).trim(), pair.slice(i + 1).trim()];
		}
	}
	return null;
}

/** 引号外的顶层是否出现 `=`：用于判别「组件形态」（`k=v`）与「NBT 标签」（`K:v`）。 */
export function hasTopLevelEquals(s: string): boolean {
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
		else if (ch === "{" || ch === "[") depth++;
		else if (ch === "}" || ch === "]") depth--;
		else if (ch === "=" && depth === 0) return true;
	}
	return false;
}

function unquote(s: string): string {
	if (s.length >= 2 && ((s.startsWith('"') && s.endsWith('"')) || (s.startsWith("'") && s.endsWith("'")))) {
		return s.slice(1, -1);
	}
	return s;
}

export interface ParsedItemStack {
	/** 去命名空间前缀的物品 id；无 id（非物品栈）时为 null。 */
	item: string | null;
	count: number | null;
	/** modern：components 内文转 give 参数语法（`minecraft:x=v,...`，供 /give `[...]`）；无则 null。 */
	modernArgs: string | null;
	/** modern：components 内文原样（`"minecraft:x":v,...`，供物品栈 components:{...}）；无则 null。 */
	modernComponents: string | null;
	/** legacy：除 id/Count/components 外的标签重组（`Key:value,...`）；无则 null。 */
	legacyTags: string | null;
}

/**
 * 解析物品栈片段（NBT 生成器输出的 `{id:...,count:...,components:{...}}` / legacy `{id:...,Count:1b,...}`）。
 * 解析不出 `id` 时返回 null（调用方按普通 NBT 处理）。仅浅层解析，复杂值原样透传。
 */
export function parseItemStackFragment(raw: string): ParsedItemStack | null {
	const body = normalizeNbtFragment(raw);
	if (!body) return null;
	let item: string | null = null;
	let count: number | null = null;
	let modernArgs: string | null = null;
	let modernComponents: string | null = null;
	const legacyParts: string[] = [];
	for (const pair of splitTopLevel(body)) {
		const kv = splitTopLevelPair(pair);
		if (!kv) {
			legacyParts.push(pair);
			continue;
		}
		const key = unquote(kv[0]);
		if (key === "id") {
			item = unquote(kv[1]).replace(/^minecraft:/, "");
		} else if (key === "count" || key === "Count") {
			const num = parseInt(kv[1], 10);
			if (Number.isFinite(num)) count = num;
		} else if (key === "components") {
			const inner = kv[1].trim();
			if (inner.startsWith("{") && inner.endsWith("}")) {
				const body = inner.slice(1, -1);
				modernComponents = body;
				modernArgs = splitTopLevel(body)
					.map((p) => {
						const c = splitTopLevelPair(p);
						return c ? `${unquote(c[0])}=${c[1]}` : p;
					})
					.join(",");
			}
		} else if (key === "tag") {
			// 物品栈的自定义数据在 tag 内；/give 的附加段需要内层内容
			const inner = kv[1].trim();
			if (inner.startsWith("{") && inner.endsWith("}")) {
				const tags = inner.slice(1, -1).trim();
				if (tags) legacyParts.push(tags);
			} else {
				legacyParts.push(pair);
			}
		} else if (key === "Damage" || key === "Slot") {
			// 物品栈的耐久/变种与槽位字段，不属于 give 附加标签
		} else {
			legacyParts.push(pair);
		}
	}
	if (!item) return null;
	return {
		item,
		count,
		modernArgs,
		modernComponents,
		legacyTags: legacyParts.length > 0 ? legacyParts.join(",") : null,
	};
}
