/** 种子解析工具（种子查看器共用）：输入 → 十进制 i64 字符串。
 *  数字种子不经过 JS Number（真实种子可超 2^53，中转即丢精度——后端按
 *  十进制字符串 parse::<i64>()）；非数字串按 MC Java String.hashCode。 */

/** Java String.hashCode（32 位有符号）。 */
export function javaSeedHash(text: string): number {
	let h = 0;
	for (let i = 0; i < text.length; i++) {
		h = (h * 31 + text.charCodeAt(i)) | 0;
	}
	return h;
}

/** 解析种子输入 → 十进制 i64 字符串；空/非法返回 null。
 *  合法形态：十进制整数（原样保留），或任意非数字串（转 Java 哈希）。 */
export function parseSeedText(text: string): string | null {
	const t = text.trim();
	if (!t) return null;
	if (/^-?\d+$/.test(t)) return t;
	return String(javaSeedHash(t));
}
