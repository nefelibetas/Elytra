// 设置页 tab 状态：模块级持久（路由切换/自定义外观返回不丢）。
// 对齐 downloadTabState 先例：tab 切换是纯前端 UI 状态，模块级 ref 单例即可。
import { ref } from "vue";

export type SettingsTab = "launch" | "download" | "appearance" | "online" | "log";

export const settingsTab = ref<SettingsTab>("launch");