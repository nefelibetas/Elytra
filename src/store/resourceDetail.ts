// 资源详情 store（Mod / 光影包 / 资源包 / 数据包 / 整合包共用）：
// 社区搜索结果点击后选中的 item + 跳转详情页。
// 详情页与 TitleBar 都从这里读数据（路由只带 source/id 用于标题兜底）。
// 详情支持依赖链跳转：进入子 mod（前置依赖）时压栈，返回时弹栈恢复上一个详情；
// 版本筛选（ModLoader/游戏版本）随依赖链同步到下一个详情，返回时还原各层自身筛选。
// 各层已加载的版本列表/前置依赖随压栈存档，返回时直接恢复快照（不重新请求）。
import { reactive } from "vue";
import router from "../router";
import type { ModDisplayItem, ModVersion, ModDependencyItem } from "./modSearch";
import { downloadTabState, type DownloadTab } from "./downloadTab";

/** 详情历史栈元素：item + 进入下一层时该层的版本筛选/选中版本 + 已加载数据快照（返回时还原，不重新请求）。 */
export interface DetailHistoryEntry {
  item: ModDisplayItem;
  loader: string;
  gameVersion: string;
  /** 该层选中的版本文件 id（返回时恢复选中态，依赖卡片随之恢复） */
  selectedVersion: string;
  /** 该层已加载的版本列表（返回时恢复，不重新请求） */
  versions: ModVersion[];
  /** 该层版本列表加载错误（返回时恢复错误态） */
  versionError: string;
  /** 该层已加载的前置依赖（返回时恢复，不重新请求） */
  depItems: ModDependencyItem[];
  /** 该层前置依赖加载错误（返回时恢复错误态） */
  depError: string;
}

/** 详情页已加载数据快照：压栈时由详情页传入存档，返回时恢复（不重新请求）。 */
export interface DetailSnapshot {
  versions: ModVersion[];
  versionError: string;
  depItems: ModDependencyItem[];
  depError: string;
}

export const resourceDetail = reactive({
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
  /** 返回恢复：上一层已加载的版本列表快照（页面 onItemChange 恢复，跳过重新请求；null = 非返回路径） */
  pendingVersions: null as ModVersion[] | null,
  pendingVersionError: "",
  /** 返回恢复：上一层已加载的前置依赖快照（页面 loadDeps 恢复，跳过重新请求；null = 非返回路径） */
  pendingDepItems: null as ModDependencyItem[] | null,
  pendingDepError: "",
});

/** 选中 item → 存入 store 并跳转 /resource-detail（返回键可回下载页并恢复来源 tab）。
 *  已在详情页时（点击前置依赖）将当前 item + 当前筛选/选中 + 已加载数据快照压栈，返回可回到上一个详情；
 *  从列表进入时清空历史栈（避免跨会话残留污染返回链）。
 *  `filters`：当前详情页的版本筛选与选中版本，同步给新详情并随压栈存档；
 *  `snapshot`：当前详情页已加载的版本列表/前置依赖快照，随压栈存档（返回时恢复，不重新请求）。 */
export function openResourceDetail(
  item: ModDisplayItem,
  filters?: { loader: string; gameVersion: string; selectedVersion: string },
  snapshot?: DetailSnapshot,
) {
  // 每次新进入：清掉返回恢复标记（返回恢复只由 goBackDetail 设置、一次性消费）
  resourceDetail.pendingVersions = null;
  resourceDetail.pendingDepItems = null;
  if (router.currentRoute.value.path === "/resource-detail") {
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
    // 从列表/其他页面进入：重置历史栈
    resourceDetail.history.length = 0;
  }
  resourceDetail.pendingLoader = filters?.loader ?? "";
  resourceDetail.pendingGameVersion = filters?.gameVersion ?? "";
  resourceDetail.pendingSelectedVersion = filters?.selectedVersion ?? "";
  resourceDetail.item = item;
  resourceDetail.backTab = downloadTabState.current;
  router.push({
    path: "/resource-detail",
    query: { source: item.source, id: item.id },
  });
}

/** 详情页返回：弹栈回到上一个详情（恢复其筛选、选中与已加载数据快照，不重新请求）；栈空则回下载页并恢复来源 tab。 */
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
      path: "/resource-detail",
      query: { source: prev.item.source, id: prev.item.id },
    });
    return;
  }
  downloadTabState.current = resourceDetail.backTab;
  router.push("/download");
}

/** 详情页标题：有中文译名时显示 "翻译名（原名）"，否则原名。 */
export function detailTitle(): string {
  const it = resourceDetail.item;
  if (!it) return "";
  return it.title_zh ? `${it.title_zh}（${it.title}）` : it.title;
}
