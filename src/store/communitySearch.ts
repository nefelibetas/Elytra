// 社区资源搜索 store（Modrinth / CurseForge 聚合，覆盖 Mod/光影/资源包等）：模块级 reactive 单例，页面与组件直接 import。
// 来源为本地状态：Modrinth 无需 key；CurseForge 需 .env 配置的 API Key（后端校验）
import { reactive, computed } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { dlTranslateMode } from "./downloaderConfig";
import { errMsg } from "../utils/error";

export interface CommunitySearchResult {
  id: string;
  title: string;
  /** 内置百科表提供的中文译名（无译名/未开启翻译为 null） */
  title_zh: string | null;
  /** MC百科条目 id（内置表 slug 匹配；未收录为 null）→ https://www.mcmod.cn/class/{id}.html */
  mcmod_id: number | null;
  description: string;
  /** MCIM 中文简介翻译（无翻译/未开启翻译为 null） */
  translated: string | null;
  /** 支持的 ModLoader（fabric/forge/neoforge/quilt，可能为空） */
  loaders: string[];
  /** 支持的 MC 游戏版本范围（"最低 ~ 最高"；无则 null） */
  game_version_range: string | null;
  author: string;
  downloads: number;
  icon_url: string | null;
  page_url: string;
  /** 搜索结果所属分类（shader/worlds 等；普通 mod 搜索为 null） */
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
  /** 版本类型（两源归一）：release | beta | alpha | unknown */
  version_type: string;
  /** 该版本文件的依赖列表（两源归一：project_id + relation） */
  dependencies: CommunityDependency[];
}

/** 版本文件依赖（镜像 Rust downloader::community::CommunityDependency）。 */
export interface CommunityDependency {
  project_id: string;
  /** required | optional | incompatible | embedded */
  relation: string;
}

/** 依赖项目详情（镜像 Rust downloader::community::CommunityDependencyItem；命令 get_community_details）。 */
export interface CommunityDependencyItem {
  id: string;
  title: string;
  description: string;
  icon_url: string | null;
  /** MCIM 中文简介翻译（翻译关闭/未命中为 null，UI 降级显示原文） */
  translated: string | null;
}

/** 查询项目版本文件列表（Modrinth/CurseForge 均支持；分类项目同样适用）。 */
export function fetchVersions(source: string, projectId: string): Promise<CommunityVersion[]> {
  return invoke<CommunityVersion[]>("get_community_versions", { source, projectId });
}

/** 批量查询项目详情（前置依赖卡片数据源；同源查询；按翻译模式决定是否查中文简介）。 */
export function fetchCommunityDetails(source: string, ids: string[]): Promise<CommunityDependencyItem[]> {
  return invoke<CommunityDependencyItem[]>("get_community_details", { source, ids, translate: needsTranslation() });
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

/** 聚合搜索：两源并发，单源失败降级为空；按标题去重保留下载量高的一侧。
 *  `category` 为项目类型过滤（如 "shader"），普通 mod 传 null。
 *  `pageSize` 每源条数：聚合模式传页大小一半（15），合并去重后展示 ≤15 条。
 *  返回的 `maxKnownTotal` 为「已知总量下限」：两源各自的 `total` 是平台精确命中数，
 *  聚合去重后真实总数需拉全两源才能得（跨源标题去重无法离线求并），故取两者较大值——
 *  并集 ≥ 任一源，因此它是下限；同时与分页口径一致（每页每源各取 15 条、offset 同步推进，
 *  能翻到的最深页数正是 ceil(max/每源页大小)）。**不要改成 sum**（去重后会重复计数）。 */
export async function searchAllSources(
  query: string,
  gameVersion: string,
  loader: string,
  category: string | null,
  offset: number,
  pageSize: number,
): Promise<{ items: CommunityDisplayItem[]; maxKnownTotal: number }> {
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
  };
}

export const communitySearch = reactive({
  source: "all" as "all" | "modrinth" | "curseforge",
  query: "",
  gameVersion: "",
  loader: "",
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
      const pageSize = this.source === "all" ? AGG_PAGE_SIZE : MOD_PAGE_SIZE;
      const offset = (page - 1) * pageSize;
      if (this.source === "all") {
        const res = await searchAllSources(this.query.trim(), this.gameVersion, this.loader, null, offset, pageSize);
        if (seq !== this.searchSeq) return;
        this.results = res.items;
        this.total = res.maxKnownTotal;
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

export const communityTotalPages = computed(() => {
  const pageSize = communitySearch.source === "all" ? AGG_PAGE_SIZE : MOD_PAGE_SIZE;
  return Math.max(1, Math.ceil(communitySearch.total / pageSize));
});
