// Mod 搜索 store（Modrinth / CurseForge 聚合）：模块级 reactive 单例，页面与组件直接 import。
// 来源为本地状态：Modrinth 无需 key；CurseForge 需 .env 配置的 API Key（后端校验）
import { reactive, computed } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { showToast } from "../utils/toast";

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

export interface ModVersion {
  version_id: string;
  version_number: string;
  game_versions: string[];
  loaders: string[];
  filename: string;
  download_url: string;
  file_size: number;
  sha1: string | null;
  date_published: string;
  downloads: number;
}

export interface ModDisplayItem extends ModSearchResult {
  /** 该结果实际来自哪个源（聚合时区分；key = source:id，两源 id 体系不同防撞） */
  source: "modrinth" | "curseforge";
  key: string;
}

const MOD_PAGE_SIZE = 30;

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
function cleanText(s: string): string {
  return s
    .replace(/\$\$(.*?)\$\$/g, "$1")
    .replace(/\*\*(.*?)\*\*/g, "$1")
    .replace(/\[(.*?)\]\(.*?\)/g, "$1")
    .replace(/<[^>]+>/g, "")
    .trim();
}

function toDisplayItem(h: ModSearchResult, source: "modrinth" | "curseforge"): ModDisplayItem {
  return {
    ...h,
    title: cleanText(h.title),
    description: cleanText(h.description),
    source,
    key: `${source}:${h.id}`,
  };
}

/** 聚合搜索：两源并发，单源失败降级为空；按标题去重保留下载量高的一侧；总数取两源较大值（去重无法精确） */
async function searchAllSources(
  query: string,
  gameVersion: string,
  loader: string,
  offset: number,
): Promise<{ items: ModDisplayItem[]; total: number }> {
  const params = {
    query,
    gameVersion: gameVersion || null,
    loader: loader || null,
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
  versions: {} as Record<string, ModVersion[]>,
  versionsLoading: {} as Record<string, boolean>,
  expanded: {} as Record<string, boolean>,

  /** 清空版本相关缓存（换页/重搜时调用） */
  clearVersions() {
    this.versions = {};
    this.versionsLoading = {};
    this.expanded = {};
  },

  async search(page = 1) {
    this.searching = true;
    this.error = "";
    try {
      const offset = (page - 1) * MOD_PAGE_SIZE;
      if (this.source === "all") {
        const res = await searchAllSources(this.query.trim(), this.gameVersion, this.loader, offset);
        this.results = res.items;
        this.total = res.total;
      } else {
        const res = await invoke<ModSearchPage>("search_mods", {
          source: this.source,
          query: this.query.trim(),
          gameVersion: this.gameVersion || null,
          loader: this.loader || null,
          offset,
        });
        this.results = res.items.map(h => toDisplayItem(h, this.source as "modrinth" | "curseforge"));
        this.total = res.total;
      }
      this.page = page;
      this.searched = true;
      this.clearVersions();
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
    this.clearVersions();
    this.search();
  },

  async toggleVersions(item: ModDisplayItem) {
    this.expanded[item.key] = !this.expanded[item.key];
    if (!this.expanded[item.key]) return;
    if (this.versions[item.key] || this.versionsLoading[item.key]) return;
    this.versionsLoading[item.key] = true;
    try {
      this.versions[item.key] = await invoke<ModVersion[]>("get_mod_versions", {
        source: item.source,
        projectId: item.id,
        gameVersion: this.gameVersion || null,
        loader: this.loader || null,
      });
    } catch (e) {
      this.versions[item.key] = [];
      showToast(String(e));
    } finally {
      this.versionsLoading[item.key] = false;
    }
  },

  /** 首次进入 Mod 页自动搜索热门（对齐 PCL2）；仅触发一次 */
  ensureAutoSearch() {
    if (autoSearched || this.searching) return;
    autoSearched = true;
    this.search();
  },
});

export const modTotalPages = computed(() => Math.max(1, Math.ceil(modSearch.total / MOD_PAGE_SIZE)));
