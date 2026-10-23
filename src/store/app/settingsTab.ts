// 设置页 tab 状态：模块级 ref 单例，路由切换/自定义外观返回不丢。
import { ref } from "vue";

export type SettingsTab = "launch" | "download" | "appearance" | "online" | "log" | "external" | "tools" | "about";

export const settingsTab = ref<SettingsTab>("launch");
