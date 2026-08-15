// Mod 详情 store：社区搜索结果点击后选中的 item + 跳转详情页。
// 详情页与 TitleBar 都从这里读数据（路由只带 source/id 用于标题兜底）。
// 详情支持依赖链跳转：进入子 mod（前置依赖）时压栈，返回时弹栈恢复上一个详情；
// 版本筛选（ModLoader/游戏版本）随依赖链同步到下一个详情，返回时还原各层自身筛选。
import { reactive } from "vue";
import router from "../router";
import type { ModDisplayItem } from "./modSearch";
import { downloadTabState, type DownloadTab } from "./downloadTab";

/** 详情历史栈元素：item + 进入下一层时该层的版本筛选/选中版本（返回时还原）。 */
export interface DetailHistoryEntry {
  item: ModDisplayItem;
  loader: string;
  gameVersion: string;
  /** 该层选中的版本文件 id（返回时恢复选中态，依赖卡片随之恢复） */
  selectedVersion: string;
}

export const modDetail = reactive({
  item: null as ModDisplayItem | null,
  /** 进入详情时记录的来源 tab（返回下载页时恢复） */
  backTab: "mod" as DownloadTab,
  /** 详情历史栈：从详情页进入子 mod 时压栈当前；弹栈恢复上一个详情及其筛选/选中 */
  history: [] as DetailHistoryEntry[],
  /** 进入当前详情的版本筛选（从上一详情同步；详情页加载时应用） */
  pendingLoader: "",
  pendingGameVersion: "",
  /** 进入当前详情时该层选中的版本 id（返回时恢复选中态） */
  pendingSelectedVersion: "",
});

/** 选中 item → 存入 store 并跳转 /mod-detail（返回键可回下载页并恢复来源 tab）。
 *  已在详情页时（点击前置依赖）将当前 item + 当前筛选/选中压栈，返回可回到上一个详情；
 *  从列表进入时清空历史栈（避免跨会话残留污染返回链）。
 *  `filters`：当前详情页的版本筛选与选中版本，同步给新详情并随压栈存档。 */
export function openModDetail(
  item: ModDisplayItem,
  filters?: { loader: string; gameVersion: string; selectedVersion: string },
) {
  if (router.currentRoute.value.path === "/mod-detail") {
    // 详情内跳转（前置依赖链）：压栈当前详情与筛选/选中
    if (modDetail.item) {
      modDetail.history.push({
        item: modDetail.item,
        loader: filters?.loader ?? "",
        gameVersion: filters?.gameVersion ?? "",
        selectedVersion: filters?.selectedVersion ?? "",
      });
    }
  } else {
    // 从列表/其他页面进入：重置历史栈
    modDetail.history.length = 0;
  }
  modDetail.pendingLoader = filters?.loader ?? "";
  modDetail.pendingGameVersion = filters?.gameVersion ?? "";
  modDetail.pendingSelectedVersion = filters?.selectedVersion ?? "";
  modDetail.item = item;
  modDetail.backTab = downloadTabState.current;
  router.push({
    path: "/mod-detail",
    query: { source: item.source, id: item.id },
  });
}

/** 详情页返回：弹栈回到上一个详情（恢复其筛选与选中，不重新请求）；栈空则回下载页并恢复来源 tab。 */
export function goBackDetail() {
  const prev = modDetail.history.pop();
  if (prev) {
    modDetail.item = prev.item;
    modDetail.pendingLoader = prev.loader;
    modDetail.pendingGameVersion = prev.gameVersion;
    modDetail.pendingSelectedVersion = prev.selectedVersion;
    router.replace({
      path: "/mod-detail",
      query: { source: prev.item.source, id: prev.item.id },
    });
    return;
  }
  downloadTabState.current = modDetail.backTab;
  router.push("/download");
}

/** 详情页标题：有中文译名时显示 "翻译名（原名）"，否则原名。 */
export function detailTitle(): string {
  const it = modDetail.item;
  if (!it) return "";
  return it.title_zh ? `${it.title_zh}（${it.title}）` : it.title;
}
