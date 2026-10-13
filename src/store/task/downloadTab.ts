// 下载页 tab 状态：模块级持久（切路由/详情页返回不丢）。只管 tab 与滚动位置；搜索条件与结果在 communitySearch/categorySearch store。
import { reactive } from "vue";

export type DownloadTab =
  | "vanilla"
  | "mod"
  | "modpack"
  | "datapack"
  | "resourcepack"
  | "shader"
  | "worlds";

export const downloadTabState = reactive({
  current: "vanilla" as DownloadTab,
  /** 各 tab 在资源中心页滚动容器中的滚动位置（离开时记录，返回时恢复） */
  scrollTop: {
    vanilla: 0,
    mod: 0,
    modpack: 0,
    datapack: 0,
    resourcepack: 0,
    shader: 0,
    worlds: 0,
  } as Record<DownloadTab, number>,
});
