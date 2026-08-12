// Mod 搜索 store（Modrinth / CurseForge 聚合）：模块级 reactive 单例，页面与组件直接 import。
// 来源为本地状态：Modrinth 无需 key；CurseForge 需 .env 配置的 API Key（后端校验）
import { reactive, computed } from "vue";
import { invoke } from "@tauri-apps/api/core";

export interface ModSearchResult {
  id: string;
  title: string;
  description: string;
  author: string;
  downloads: number;
  icon_url: string | null;
  page_url: string;
}

export interface ModSearchPage {
  items: ModSearchResult[];
  total: number;
}

export interface ModDisplayItem extends ModSearchResult {
  /** 该结果实际来自哪个源（聚合时区分；key = source:id，两源 id 体系不同防撞） */
  source: "modrinth" | "curseforge";
  key: string;
}

const MOD_PAGE_SIZE = 30;
/** 聚合模式每源条数（页大小一半）：两源合并去重后展示 ≤15 条，每页图片请求量减半 */
const AGG_PAGE_SIZE = 15;

export const modLoaderOptions = [
  { value: "", label: "不限加载器" },
  { value: "fabric", label: "Fabric" },
  { value: "forge", label: "Forge" },
  { value: "neoforge", label: "NeoForge" },
  { value: "quilt", label: "Quilt" },
];

/** 首次进入 Mod 页自动搜索热门只触发一次（重置后不重复自动触发） */
let autoSearched = false;

/** 剥离 markdown/KaTeX 标记（Modrinth 描述支持 KaTeX，`$$...$$`、`**加粗**` 等会原样返回，UI 纯文本显示需清洗） */
export function cleanText(s: string): string {
  return s
    .replace(/\$\$(.*?)\$\$/g, "$1")
    .replace(/\*\*(.*?)\*\*/g, "$1")
    .replace(/\[(.*?)\]\(.*?\)/g, "$1")
    .replace(/<[^>]+>/g, "")
    .trim();
}

export function toDisplayItem(h: ModSearchResult, source: "modrinth" | "curseforge"): ModDisplayItem {
  return {
    ...h,
    title: cleanText(h.title),
    description: cleanText(h.description),
    source,
    key: `${source}:${h.id}`,
  };
}

/** 聚合搜索：两源并发，单源失败降级为空；按标题去重保留下载量高的一侧；总数取两源较大值（去重无法精确）。
 *  `category` 为项目类型过滤（如 "shader"），普通 mod 传 null。
 *  `pageSize` 每源条数：聚合模式传页大小一半（15），合并去重后展示 ≤15 条。 */
export async function searchAllSources(
  query: string,
  gameVersion: string,
  loader: string,
  category: string | null,
  offset: number,
  pageSize: number,
): Promise<{ items: ModDisplayItem[]; total: number }> {
  const params = {
    query,
    gameVersion: gameVersion || null,
    loader: loader || null,
    category,
    pageSize,
    offset,
  };
  const [mr, cf] = await Promise.all([
    invoke<ModSearchPage>("search_mods", { ...params, source: "modrinth" }).catch(() => null),
    invoke<ModSearchPage>("search_mods", { ...params, source: "curseforge" }).catch(() => null),
  ]);
  const merged = new Map<string, ModDisplayItem>();
  const all: Array<[ModSearchResult, "modrinth" | "curseforge"]> = [
    ...(cf?.items ?? []).map<[ModSearchResult, "curseforge"]>(h => [h, "curseforge"]),
    ...(mr?.items ?? []).map<[ModSearchResult, "modrinth"]>(h => [h, "modrinth"]),
  ];
  for (const [h, src] of all) {
    const key = cleanText(h.title).toLowerCase();
    const prev = merged.get(key);
    if (!prev || h.downloads > prev.downloads) {
      merged.set(key, toDisplayItem(h, src));
    }
  }
  return {
    items: Array.from(merged.values()).sort((a, b) => b.downloads - a.downloads),
    total: Math.max(mr?.total ?? 0, cf?.total ?? 0),
  };
}

export const modSearch = reactive({
  source: "all" as "all" | "modrinth" | "curseforge",
  query: "",
  gameVersion: "",
  loader: "",
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
      const pageSize = this.source === "all" ? AGG_PAGE_SIZE : MOD_PAGE_SIZE;
      const offset = (page - 1) * pageSize;
      if (this.source === "all") {
        const res = await searchAllSources(this.query.trim(), this.gameVersion, this.loader, null, offset, pageSize);
        this.results = res.items;
        this.total = res.total;
      } else {
        const res = await invoke<ModSearchPage>("search_mods", {
          source: this.source,
          query: this.query.trim(),
          gameVersion: this.gameVersion || null,
          loader: this.loader || null,
          pageSize,
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
    this.loader = "";
    this.results = [];
    this.total = 0;
    this.page = 1;
    this.searched = false;
    this.error = "";
    this.search();
  },

  /** 首次进入 Mod 页自动搜索热门（对齐 PCL2）；仅触发一次 */
  ensureAutoSearch() {
    if (autoSearched || this.searching) return;
    autoSearched = true;
    this.search();
  },
});

export const modTotalPages = computed(() => {
  const pageSize = modSearch.source === "all" ? AGG_PAGE_SIZE : MOD_PAGE_SIZE;
  return Math.max(1, Math.ceil(modSearch.total / pageSize));
});
