// 后端错误约定为 "中文文案\n技术细节"：toast/内联取首行，AlertDialog 用全文。
/** 取首行；未知值兜底友好文案。 */
export function errMsg(e: unknown): string {
  const s = String(e);
  const first = s.split("\n")[0]?.trim();
  return first || "操作失败，请重试";
}
