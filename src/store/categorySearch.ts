// 分类资源搜索 store（光影包 / 资源包等非 mod 项目类型）：
// 工厂生成各自独立的 reactive 单例（状态互不干扰），复用 modSearch 的聚合逻辑，
// 固定 category（project_type / CurseForge classId），无加载器。
import { reactive, computed } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { searchAllSources, toDisplayItem, type ModDisplayItem } from "./modSearch";

const PAGE_SIZE = 30;

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
        const offset = (page - 1) * PAGE_SIZE;
        if (this.source === "all") {
          const res = await searchAllSources(this.query.trim(), this.gameVersion, "", category, offset);
          this.results = res.items;
          this.total = res.total;
        } else {
          const res = await invoke<{ items: ModDisplayItem[]; total: number }>("search_mods", {
            source: this.source,
            query: this.query.trim(),
            gameVersion: this.gameVersion || null,
            loader: null,
            category,
            offset,
          });
          this.results = res.items.map(h => toDisplayItem(h, this.source as "modrinth" | "curseforge"));
          this.total = res.total;
        }
        this.page = page;
        this.searched = true;
      } catch (e) {
        this.error = String(e);
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

export const shaderSearch = createCategorySearch("shader");
export const resourcepackSearch = createCategorySearch("resourcepack");
export const datapackSearch = createCategorySearch("datapack");
export const modpackSearch = createCategorySearch("modpack");

export const shaderTotalPages = computed(() => Math.max(1, Math.ceil(shaderSearch.total / PAGE_SIZE)));
export const resourcepackTotalPages = computed(() =>
  Math.max(1, Math.ceil(resourcepackSearch.total / PAGE_SIZE))
);
export const datapackTotalPages = computed(() =>
  Math.max(1, Math.ceil(datapackSearch.total / PAGE_SIZE))
);
export const modpackTotalPages = computed(() =>
  Math.max(1, Math.ceil(modpackSearch.total / PAGE_SIZE))
);
