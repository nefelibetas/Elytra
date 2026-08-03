// 离线皮肤类型/模型与选项元数据（Settings 选项、Launch 渲染共用）。
// "random 哈希选 Steve/Alex" 规则与 Rust 侧 apply_offline_skin 的 hash_name 保持一致。

export type OfflineSkinType = "random" | "steve" | "alex" | "player" | "custom";
export type OfflineSkinModel = "steve" | "alex" | "auto";

export interface SkinTypeOption {
  value: OfflineSkinType;
  label: string;
  desc: string;
}

export const SKIN_TYPE_OPTIONS: SkinTypeOption[] = [
  { value: "random", label: "随机", desc: "按用户名哈希确定性选择 Steve/Alex 默认皮肤" },
  { value: "steve", label: "Steve", desc: "固定使用 Steve 默认皮肤" },
  { value: "alex", label: "Alex", desc: "固定使用 Alex 默认皮肤" },
  { value: "player", label: "正版玩家", desc: "显示指定正版玩家的皮肤" },
  { value: "custom", label: "自定义", desc: "使用本地皮肤文件" },
];

/** FNV-1a 哈希（与 Rust 侧 config/launcher/skin.rs hash_name 一致）。 */
export function hashName(name: string): number {
  let h = 2166136261;
  for (let i = 0; i < name.length; i++) {
    h ^= name.charCodeAt(i);
    h = Math.imul(h, 16777619);
  }
  return h >>> 0;
}

/** Random 皮肤模型决策：哈希奇数 → Alex（slim）。 */
export function randomIsAlex(name: string): boolean {
  return hashName(name) % 2 === 1;
}
