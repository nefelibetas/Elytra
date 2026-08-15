// 错误消息展示收口：后端约定错误消息为 "中文短文案\n技术细节"（首行友好，细节随行）。
// toast / 页面内联错误只取首行；AlertDialog(copyable) 走全文（首行显示 + 复制细节）。
/** 取错误消息首行（未知值兜底友好文案）。 */
export function errMsg(e: unknown): string {
  const s = String(e);
  const first = s.split("\n")[0]?.trim();
  return first || "操作失败，请重试";
}
