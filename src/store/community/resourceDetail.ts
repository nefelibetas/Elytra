// 资源详情 store（Mod/光影包/资源包/数据包/整合包共用）：选中 item + 跳转详情页，标题栏与详情页从此读数据（路由只带 source/id 兜底）。依赖链跳转压栈/弹栈恢复筛选与已加载快照（返回不重新请求）。
import { reactive } from "vue";
import router, { ROUTE_DOWNLOAD, ROUTE_RESOURCE_DETAIL } from "../../router";
import type { CommunityDisplayItem, CommunityVersion, CommunityDependencyItem } from "../community/communitySearch";
import { downloadTabState, type DownloadTab } from "../task/downloadTab";

/** 详情历史栈元素：item + 压栈时的版本筛选/选中 + 已加载快照（返回时还原，不重新请求）。 */
export interface DetailHistoryEntry {
  item: CommunityDisplayItem;
  loader: string;
  gameVersion: string;
  selectedVersion: string;
  versions: CommunityVersion[];
  versionError: string;
  depItems: CommunityDependencyItem[];
  depError: string;
}

export interface DetailSnapshot {
  versions: CommunityVersion[];
  versionError: string;
  depItems: CommunityDependencyItem[];
  depError: string;
}

export const resourceDetail = reactive({
  item: null as CommunityDisplayItem | null,
  backTab: "mod" as DownloadTab,
  history: [] as DetailHistoryEntry[],
  pendingLoader: "",
  pendingGameVersion: "",
  /** 进入当前详情时该层选中的版本 id（返回时恢复选中态） */
  pendingSelectedVersion: "",
  /** 返回恢复：上一层已加载的版本列表快照（页面 onItemChange 恢复，跳过重新请求；null = 非返回路径） */
  pendingVersions: null as CommunityVersion[] | null,
  pendingVersionError: "",
  /** 返回恢复：上一层已加载的前置依赖快照（页面 loadDeps 恢复，跳过重新请求；null = 非返回路径） */
  pendingDepItems: null as CommunityDependencyItem[] | null,
  pendingDepError: "",
});

/** 选中 item 存入 store 并跳转 /resource-detail。已在详情页时压栈当前 item + 筛选/选中 + 快照；从列表进入时清空历史栈。 */
export function openResourceDetail(
  item: CommunityDisplayItem,
  filters?: { loader: string; gameVersion: string; selectedVersion: string },
  snapshot?: DetailSnapshot,
) {
  // 每次新进入：清掉返回恢复标记（返回恢复只由 goBackDetail 设置、一次性消费）
  resourceDetail.pendingVersions = null;
  resourceDetail.pendingDepItems = null;
  if (router.currentRoute.value.path === ROUTE_RESOURCE_DETAIL) {
    // 详情内跳转（前置依赖链）：压栈当前详情与筛选/选中
    if (resourceDetail.item) {
      resourceDetail.history.push({
        item: resourceDetail.item,
        loader: filters?.loader ?? "",
        gameVersion: filters?.gameVersion ?? "",
        selectedVersion: filters?.selectedVersion ?? "",
        versions: snapshot?.versions ?? [],
        versionError: snapshot?.versionError ?? "",
        depItems: snapshot?.depItems ?? [],
        depError: snapshot?.depError ?? "",
      });
    }
  } else {
    resourceDetail.history.length = 0;
  }
  resourceDetail.pendingLoader = filters?.loader ?? "";
  resourceDetail.pendingGameVersion = filters?.gameVersion ?? "";
  resourceDetail.pendingSelectedVersion = filters?.selectedVersion ?? "";
  resourceDetail.item = item;
  resourceDetail.backTab = downloadTabState.current;
  router.push({
    path: ROUTE_RESOURCE_DETAIL,
    query: { source: item.source, id: item.id },
  });
}

/** 详情页返回：弹栈回到上一详情（恢复其筛选、选中与快照，不重新请求）；栈空则回下载页并恢复来源 tab。 */
export function goBackDetail() {
  const prev = resourceDetail.history.pop();
  if (prev) {
    resourceDetail.item = prev.item;
    resourceDetail.pendingLoader = prev.loader;
    resourceDetail.pendingGameVersion = prev.gameVersion;
    resourceDetail.pendingSelectedVersion = prev.selectedVersion;
    // 已加载数据快照随返回传给详情页（onItemChange/loadDeps 直接恢复，跳过重新请求）
    resourceDetail.pendingVersions = prev.versions;
    resourceDetail.pendingVersionError = prev.versionError;
    resourceDetail.pendingDepItems = prev.depItems;
    resourceDetail.pendingDepError = prev.depError;
    router.replace({
      path: ROUTE_RESOURCE_DETAIL,
      query: { source: prev.item.source, id: prev.item.id },
    });
    return;
  }
  downloadTabState.current = resourceDetail.backTab;
  router.push(ROUTE_DOWNLOAD);
}

export function detailTitle(): string {
  const it = resourceDetail.item;
  if (!it) return "";
  return it.title_zh ? `${it.title_zh}（${it.title}）` : it.title;
}
