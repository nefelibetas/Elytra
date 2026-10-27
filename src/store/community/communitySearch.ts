// 社区资源搜索 store（Modrinth / CurseForge 聚合）：模块级 reactive 单例，页面与组件直接 import。来源为本地状态：Modrinth 无需 key，CurseForge 需 .env 配置的 API Key（后端校验）。
import { reactive, computed } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { dlTranslateMode } from "../config/downloaderConfig";
import { errMsg } from "../../utils/error";
import { t } from "../../i18n";

export interface CommunitySearchResult {
  id: string;
  title: string;
  title_zh: string | null;
  /** MC百科条目 id（内置表 slug 匹配；未收录为 null）→ https://www.mcmod.cn/class/{id}.html */
  mcmod_id: number | null;
  description: string;
  translated: string | null;
  loaders: string[];
  /** 支持的 MC 游戏版本范围（"最低 ~ 最高"；无则 null） */
  game_version_range: string | null;
  author: string;
  downloads: number;
  icon_url: string | null;
  page_url: string;
  category: string | null;
}

export interface CommunitySearchPage {
  items: CommunitySearchResult[];
  total: number;
}

export interface CommunityDisplayItem extends CommunitySearchResult {
  /** 该结果实际来自哪个源（聚合时区分；key = source:id，两源 id 体系不同防撞） */
  source: "modrinth" | "curseforge";
  key: string;
}

/** 项目版本文件（镜像 Rust downloader::community::CommunityVersion；命令 get_community_versions）。 */
export interface CommunityVersion {
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
  version_type: string;
  dependencies: CommunityDependency[];
}

/** 版本文件依赖（镜像 Rust downloader::community::CommunityDependency）。 */
export interface CommunityDependency {
  project_id: string;
  relation: string;
  /** Modrinth 精确版本提示；CurseForge 为 null */
  version_id: string | null;
}

/** 依赖项目详情（镜像 Rust downloader::community::CommunityDependencyItem；命令 get_community_details）。 */
export interface CommunityDependencyItem {
  id: string;
  title: string;
  description: string;
  icon_url: string | null;
  translated: string | null;
}

export function fetchVersions(source: string, projectId: string): Promise<CommunityVersion[]> {
  return invoke<CommunityVersion[]>("get_community_versions", { source, projectId });
}

export function fetchCommunityDetails(source: string, ids: string[]): Promise<CommunityDependencyItem[]> {
  return invoke<CommunityDependencyItem[]>("get_community_details", { source, ids, translate: needsTranslation() });
}

const MOD_PAGE_SIZE = 30;
/** 聚合模式每源条数（页大小一半）：两源合并去重后展示 ≤15 条，每页图片请求量减半 */
const AGG_PAGE_SIZE = 15;

export const modLoaderOptions = computed<{ value: string; label: string }[]>(() => [
  { value: "", label: t("download.search.allLoaders") },
  { value: "fabric", label: "Fabric" },
  { value: "forge", label: "Forge" },
  { value: "neoforge", label: "NeoForge" },
  { value: "quilt", label: "Quilt" },
]);

let autoSearched = false;

/** 剥离 markdown/KaTeX 标记（Modrinth 描述支持 KaTeX，标记会原样返回，UI 纯文本需清洗） */
export function cleanText(s: string): string {
  return s
    .replace(/\$\$(.*?)\$\$/g, "$1")
    .replace(/\*\*(.*?)\*\*/g, "$1")
    .replace(/\[(.*?)\]\(.*?\)/g, "$1")
    .replace(/<[^>]+>/g, "")
    .trim();
}

export function toDisplayItem(h: CommunitySearchResult, source: "modrinth" | "curseforge"): CommunityDisplayItem {
  // 译名与原名相同（忽略大小写，如 Fabric API）时视为无译名：只显示一个，避免 "X | X"
  const titleZh =
    h.title_zh && h.title_zh.trim().toLowerCase() !== h.title.trim().toLowerCase()
      ? h.title_zh
      : null;
  return {
    ...h,
    title_zh: titleZh,
    title: cleanText(h.title),
    description: cleanText(h.description),
    source,
    key: `${source}:${h.id}`,
  };
}

/** 翻译显示模式：仅英文（en）时后端跳过翻译请求 */
export function needsTranslation(): boolean {
  return dlTranslateMode.value !== "en";
}

/** 聚合搜索：两源并发，单源失败降级为空；按标题去重保留下载量高的一侧。`pageSize` 每源条数（聚合传 15）。
 *  返回 `maxKnownTotal` = 两源 total 较大值（已知总量下限，真实并集需拉全两源）；**勿改成 sum**（去重后会重复计数）。 */
export async function searchAllSources(
  query: string,
  gameVersion: string,
  loader: string,
  category: string | null,
  offset: number,
  pageSize: number,
): Promise<{ items: CommunityDisplayItem[]; maxKnownTotal: number; partial: boolean }> {
  const queryParams = {
    query,
    gameVersion: gameVersion || null,
    loader: loader || null,
    category,
    pageSize,
    offset,
  };
  const [mr, cf] = await Promise.all([
    invoke<CommunitySearchPage>("search_community", { source: "modrinth", query: queryParams, translate: needsTranslation() }).catch(() => null),
    invoke<CommunitySearchPage>("search_community", { source: "curseforge", query: queryParams, translate: needsTranslation() }).catch(() => null),
  ]);
  if (mr === null && cf === null) {
    throw new Error(t("download.search.allSourcesFailed"));
  }
  const merged = new Map<string, CommunityDisplayItem>();
  const all: Array<[CommunitySearchResult, "modrinth" | "curseforge"]> = [
    ...(cf?.items ?? []).map<[CommunitySearchResult, "curseforge"]>(h => [h, "curseforge"]),
    ...(mr?.items ?? []).map<[CommunitySearchResult, "modrinth"]>(h => [h, "modrinth"]),
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
    maxKnownTotal: Math.max(mr?.total ?? 0, cf?.total ?? 0),
    partial: mr === null || cf === null,
  };
}

export const communitySearch = reactive({
  source: "all" as "all" | "modrinth" | "curseforge",
  query: "",
  gameVersion: "",
  loader: "",
  searching: false,
  error: "",
  partialError: "",
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
      const pageSize = this.source === "all" ? AGG_PAGE_SIZE : MOD_PAGE_SIZE;
      const offset = (page - 1) * pageSize;
      if (this.source === "all") {
        const res = await searchAllSources(this.query.trim(), this.gameVersion, this.loader, null, offset, pageSize);
        if (seq !== this.searchSeq) return;
        this.results = res.items;
        this.total = res.maxKnownTotal;
        this.partialError = res.partial ? t("download.search.partialSource") : "";
      } else {
        const res = await invoke<CommunitySearchPage>("search_community", {
          source: this.source,
          query: {
            query: this.query.trim(),
            gameVersion: this.gameVersion || null,
            loader: this.loader || null,
            pageSize,
            offset,
          },
          translate: needsTranslation(),
        });
        if (seq !== this.searchSeq) return;
        this.results = res.items.map(h => toDisplayItem(h, this.source as "modrinth" | "curseforge"));
        this.total = res.total;
        this.partialError = "";
      }
      this.page = page;
      this.searched = true;
    } catch (e) {
      if (seq !== this.searchSeq) return;
      this.error = errMsg(e);
      this.searched = true;
      this.results = [];
      this.total = 0;
      this.partialError = "";
    } finally {
      if (seq === this.searchSeq) this.searching = false;
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

  ensureAutoSearch() {
    if (autoSearched || this.searching) return;
    autoSearched = true;
    this.search();
  },
});

export const communityTotalPages = computed(() => {
  const pageSize = communitySearch.source === "all" ? AGG_PAGE_SIZE : MOD_PAGE_SIZE;
  return Math.max(1, Math.ceil(communitySearch.total / pageSize));
});
