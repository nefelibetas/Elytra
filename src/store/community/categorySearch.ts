// 分类资源搜索 store（光影包 / 资源包等非 mod 类型）：工厂生成各自独立的 reactive 单例（状态互不干扰），复用 communitySearch 的聚合逻辑，固定 category（project_type / CurseForge classId），无加载器。
import { reactive, computed } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { searchAllSources, toDisplayItem, needsTranslation, type CommunityDisplayItem } from "../community/communitySearch";
import { errMsg } from "../../utils/error";

const PAGE_SIZE = 30;
/** 聚合模式每源条数（页大小一半）：两源合并去重后展示 ≤15 条，每页图片请求量减半 */
const AGG_PAGE_SIZE = 15;

export function createCategorySearch(
  category: string,
  initialSource: "all" | "modrinth" | "curseforge" = "all",
) {
  let autoSearched = false;

  return reactive({
    category,
    source: initialSource,
    query: "",
    gameVersion: "",
    searching: false,
    error: "",
    searched: false,
    results: [] as CommunityDisplayItem[],
    /** 结果总数：单源为平台精确命中数；聚合（source="all"）为两源较大值＝已知总量下限（非精确），UI 文案须标「至少」。 */
    total: 0,
    page: 1,
    /** 请求序号：筛选快速连切时只应用最后一次发起的结果（防过期响应覆盖） */
    searchSeq: 0,

    async search(page = 1) {
      const seq = ++this.searchSeq;
      this.searching = true;
      this.error = "";
      try {
        const pageSize = this.source === "all" ? AGG_PAGE_SIZE : PAGE_SIZE;
        const offset = (page - 1) * pageSize;
        if (this.source === "all") {
          const res = await searchAllSources(this.query.trim(), this.gameVersion, "", category, offset, pageSize);
          if (seq !== this.searchSeq) return;
          this.results = res.items;
          this.total = res.maxKnownTotal;
        } else {
          const res = await invoke<{ items: CommunityDisplayItem[]; total: number }>("search_community", {
            source: this.source,
            query: {
              query: this.query.trim(),
              gameVersion: this.gameVersion || null,
              loader: null,
              category,
              pageSize,
              offset,
            },
            translate: needsTranslation(),
          });
          if (seq !== this.searchSeq) return;
          this.results = res.items.map(h => toDisplayItem(h, this.source as "modrinth" | "curseforge"));
          this.total = res.total;
        }
        this.page = page;
        this.searched = true;
      } catch (e) {
        if (seq !== this.searchSeq) return;
        this.error = errMsg(e);
        this.searched = true;
        this.results = [];
        this.total = 0;
      } finally {
        if (seq === this.searchSeq) this.searching = false;
      }
    },

    reset() {
      this.query = "";
      this.source = initialSource;
      this.gameVersion = "";
      this.results = [];
      this.total = 0;
      this.page = 1;
      this.searched = false;
      this.error = "";
      this.search();
    },

    ensureAutoSearch() {
      if (autoSearched || this.searching) return;
      autoSearched = true;
      this.search();
    },
  });
}

function totalPages(search: { source: string; total: number }): number {
  const pageSize = search.source === "all" ? AGG_PAGE_SIZE : PAGE_SIZE;
  return Math.max(1, Math.ceil(search.total / pageSize));
}

export const shaderSearch = createCategorySearch("shader");
export const resourcepackSearch = createCategorySearch("resourcepack");
export const datapackSearch = createCategorySearch("datapack");
export const modpackSearch = createCategorySearch("modpack");
/** worlds（地图存档）：CurseForge 单源（Modrinth 无 worlds project_type，锁定 source 防无效请求） */
export const worldsSearch = createCategorySearch("worlds", "curseforge");

export const shaderTotalPages = computed(() => totalPages(shaderSearch));
export const resourcepackTotalPages = computed(() => totalPages(resourcepackSearch));
export const datapackTotalPages = computed(() => totalPages(datapackSearch));
export const modpackTotalPages = computed(() => totalPages(modpackSearch));
export const worldsTotalPages = computed(() => totalPages(worldsSearch));
