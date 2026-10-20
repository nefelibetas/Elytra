import type zhCN from "./messages/zh-CN";

/** 把消息树的叶子从字面量放宽为 string，供其他语言做 key/结构对齐。 */
type DeepString<T> = {
  [K in keyof T]: T[K] extends string ? string : DeepString<T[K]>;
};

export type MessageSchema = DeepString<typeof zhCN>;
