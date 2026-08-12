// 下载页 tab 状态：模块级持久（路由切换/详情页返回不丢）。
// 搜索条件与结果本身在 modSearch/categorySearch store 中，这里只管 tab 与滚动位置。
import { reactive } from "vue";

export type DownloadTab =
  | "vanilla"
  | "mod"
  | "modpack"
  | "datapack"
  | "resourcepack"
  | "shader";

export const downloadTabState = reactive({
  current: "vanilla" as DownloadTab,
  /** 各 tab 在下载页滚动容器中的滚动位置（离开时记录，返回时恢复） */
  scrollTop: {
    vanilla: 0,
    mod: 0,
    modpack: 0,
    datapack: 0,
    resourcepack: 0,
    shader: 0,
  } as Record<DownloadTab, number>,
});
