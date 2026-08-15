// 分类资源搜索 store（光影包 / 资源包等非 mod 项目类型）：
// 工厂生成各自独立的 reactive 单例（状态互不干扰），复用 modSearch 的聚合逻辑，
// 固定 category（project_type / CurseForge classId），无加载器。
import { reactive, computed } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { searchAllSources, toDisplayItem, needsTranslation, type ModDisplayItem } from "./modSearch";
import { errMsg } from "../utils/error";

const PAGE_SIZE = 30;
/** 聚合模式每源条数（页大小一半）：两源合并去重后展示 ≤15 条，每页图片请求量减半 */
const AGG_PAGE_SIZE = 15;

export function createCategorySearch(category: string) {
  /** 首次进入对应 tab 自动搜索热门只触发一次（预加载/重置后不重复） */
  let autoSearched = false;

  return reactive({
    category,
    source: "all" as "all" | "modrinth" | "curseforge",
    query: "",
    gameVersion: "",
    searching: false,
    error: "",
    searched: false,
    results: [] as ModDisplayItem[],
    total: 0,
    page: 1,

    async search(page = 1) {
      this.searching = true;
      this.error = "";
      try {
        const pageSize = this.source === "all" ? AGG_PAGE_SIZE : PAGE_SIZE;
        const offset = (page - 1) * pageSize;
        if (this.source === "all") {
          const res = await searchAllSources(this.query.trim(), this.gameVersion, "", category, offset, pageSize);
          this.results = res.items;
          this.total = res.total;
        } else {
          const res = await invoke<{ items: ModDisplayItem[]; total: number }>("search_mods", {
            source: this.source,
            query: this.query.trim(),
            gameVersion: this.gameVersion || null,
            loader: null,
            category,
            pageSize,
            offset,
            translate: needsTranslation(),
          });
          this.results = res.items.map(h => toDisplayItem(h, this.source as "modrinth" | "curseforge"));
          this.total = res.total;
        }
        this.page = page;
        this.searched = true;
      } catch (e) {
        this.error = errMsg(e);
        this.searched = true;
        this.results = [];
        this.total = 0;
      } finally {
        this.searching = false;
      }
    },

    reset() {
      this.query = "";
      this.source = "all";
      this.gameVersion = "";
      this.results = [];
      this.total = 0;
      this.page = 1;
      this.searched = false;
      this.error = "";
      this.search();
    },

    /** 自动搜索热门（对齐 PCL2）；仅触发一次，预加载与 tab 切换共用 */
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

export const shaderTotalPages = computed(() => totalPages(shaderSearch));
export const resourcepackTotalPages = computed(() => totalPages(resourcepackSearch));
export const datapackTotalPages = computed(() => totalPages(datapackSearch));
export const modpackTotalPages = computed(() => totalPages(modpackSearch));
