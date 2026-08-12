// Mod 详情 store：社区搜索结果点击后选中的 item + 跳转详情页。
// 详情页与 TitleBar 都从这里读数据（路由只带 source/id 用于标题兜底）。
import { reactive } from "vue";
import router from "../router";
import type { ModDisplayItem } from "./modSearch";

export const modDetail = reactive({
  item: null as ModDisplayItem | null,
});

/** 选中 item → 存入 store 并跳转 /mod-detail（返回键可用 router.back 回来源页）。 */
export function openModDetail(item: ModDisplayItem) {
  modDetail.item = item;
  router.push({
    path: "/mod-detail",
    query: { source: item.source, id: item.id },
  });
}

/** 详情页标题：有中文译名时显示 "翻译名（原名）"，否则原名。 */
export function detailTitle(): string {
  const it = modDetail.item;
  if (!it) return "";
  return it.title_zh ? `${it.title_zh}（${it.title}）` : it.title;
}
