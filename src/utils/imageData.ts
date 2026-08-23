// 图片字节 → data URL 统一工具（版本图标 / 世界缩略图共用）。

/** 魔数嗅探图片 mime（平台图标可能是 png/webp/jpeg/gif，data URL mime 必须正确）。 */
function sniffImageMime(bytes: Uint8Array): string {
  if (bytes.length >= 8 && bytes[0] === 0x89 && bytes[1] === 0x50) return "image/png";
  if (bytes.length >= 3 && bytes[0] === 0xff && bytes[1] === 0xd8) return "image/jpeg";
  if (bytes.length >= 12 && bytes[0] === 0x52 && bytes[8] === 0x57 && bytes[9] === 0x45) return "image/webp";
  if (bytes.length >= 3 && bytes[0] === 0x47 && bytes[1] === 0x49) return "image/gif";
  return "image/png";
}

/** 字节数组 → base64 data URL（分块防大数组展开栈溢出；mime 缺省按魔数嗅探）。 */
export function bytesToDataUrl(bytes: Uint8Array | number[], mime?: string): string {
  const u8 = bytes instanceof Uint8Array ? bytes : new Uint8Array(bytes);
  let bin = "";
  const CHUNK = 0x8000;
  for (let i = 0; i < u8.length; i += CHUNK) {
    bin += String.fromCharCode(...u8.subarray(i, i + CHUNK));
  }
  return `data:${mime ?? sniffImageMime(u8)};base64,${btoa(bin)}`;
}
