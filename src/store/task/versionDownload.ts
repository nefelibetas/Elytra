// 原版版本下载流程 store（manifest/版本选择/ModLoader/Fabric API/提交下载）：模块级 reactive 单例，状态与 manifest 缓存跨路由保留。
import { reactive, computed, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { dlState, registerVersionTarget } from "../task/download";
import { CATEGORIES, LOADER_MIN_VERSION } from "../../utils/versionMeta";
import { errMsg } from "../../utils/error";
import { createBoundedCache } from "../../utils/boundedCache";

export interface VersionMeta {
  id: string;
  type: string;
  url: string;
  time: string;
  releaseTime: string;
  sha1: string;
  complianceLevel: number;
}

export interface LatestVersion {
  release: string;
  snapshot: string;
}

export interface VersionManifest {
  latest: LatestVersion;
  versions: VersionMeta[];
}

export interface LoaderItem {
  key: string;
  name: string;
  icon: string;
}

export interface FabricApiVer {
  version: string;
  download_url: string;
  filename: string;
}

// 模块级 manifest 缓存：仅首次进入下载页拉取版本列表。
let manifestLoaded = false;
let manifestLoading = false;
let cachedManifest: VersionManifest | null = null;

// loader 版本缓存（key `${loader}_${mcVersion}`）：上限 64，当前所选版本条目每次写入必不被淘汰。
const loaderDataCache = createBoundedCache<string[]>(64);

// 在途请求键集合：去重须按 MC 版本维度，否则 A 在途时切 B 选同 loader 会误吞 B 请求、其缓存键永不写入。
const loaderInflight = new Set<string>();

// Fabric API 版本缓存（key mcVersion）：换版本且选了 Fabric 时整体清空，上限 8 仅为兜底。
const fabricApiDataCache = createBoundedCache<FabricApiVer[]>(8);

// Fabric API 在途键集合（按 MC 版本去重）：布尔去重会吞切换后的请求，A 迟到响应会回填 B 的选择。
const fabricApiInflight = new Set<string>();

export const loaderIcon: Record<string, string> = Object.fromEntries(
  CATEGORIES.filter(c => c.key !== "vanilla").map(c => [c.key, c.icon]),
);

const loaders: LoaderItem[] = CATEGORIES
  .filter(c => c.key !== "vanilla")
  .map(c => ({ key: c.key, name: c.title, icon: c.icon }));

function isMainLoader(key: string) { return key !== "optifine"; }

function versionTuple(v: string): number[] {
  return v.split(".").map(Number);
}

function versionGte(ver: string, min: string): boolean {
  const a = versionTuple(ver);
  const b = versionTuple(min);
  for (let i = 0; i < Math.max(a.length, b.length); i++) {
    const va = a[i] ?? 0;
    const vb = b[i] ?? 0;
    if (va !== vb) return va > vb;
  }
  return true;
}

function isAprilFools(v: VersionMeta): boolean {
  return v.releaseTime?.substring(5, 10) === "04-01";
}

export const vdState = reactive({
  manifest: cachedManifest as VersionManifest | null,
  loading: !manifestLoaded,
  error: "",
  expanded: {
    release: false, snapshot: false, old: false, aprilfools: false,
  } as Record<string, boolean>,

  selectedVersion: null as VersionMeta | null,
  versionInput: "",
  resolvedDirName: "",

  // Loader 加载态（数据本体见模块级 loaderDataCache；此表键为 loader 名，恒 ≤5 条）
  loaderLoading: {} as Record<string, boolean>,
  selectedMainLoader: null as string | null,
  selectedLoaderVersions: {} as Record<string, string>,
  expandedLoader: {} as Record<string, boolean>,

  fabricApiLoading: false,
  fabricApiExpanded: false,

  dlSubmitting: false,

  isAprilFools,
  loaderSupported(ld: string, mcVersion: string): boolean {
    const min = LOADER_MIN_VERSION[ld];
    if (min && !versionGte(mcVersion, min)) return false;
    return true;
  },
  showLoader(ld: string): boolean {
    const mc = this.selectedVersion?.id;
    if (mc && !this.loaderSupported(ld, mc)) return false;
    if (!isMainLoader(ld)) {
      // OptiFine 可独立于 ModLoader 安装；Fabric/Quilt 仅在 1.21+ 隐藏（OptiFabric 不支持）
      if (ld === "optifine") {
        if (mc && versionGte(mc, "1.21")) {
          const main = this.selectedMainLoader;
          if (main === "fabric" || main === "quilt") return false;
        }
        return true;
      }
      return false;
    }
    if (!this.selectedMainLoader) return true;
    return ld === this.selectedMainLoader;
  },

  async fetchManifest() {
    if (manifestLoading) return;
    if (manifestLoaded) return;
    manifestLoading = true;
    this.loading = true;
    this.error = "";
    try {
      this.manifest = await invoke<VersionManifest>("get_version_list");
      cachedManifest = this.manifest;
      manifestLoaded = true;
    } catch (e) {
      this.error = errMsg(e);
    } finally {
      this.loading = false;
      manifestLoading = false;
    }
  },

  refresh() {
    this.selectedVersion = null;
    this.expanded = { release: false, snapshot: false, old: false, aprilfools: false };
    this.fetchManifest();
  },

  toggle(key: string) { this.expanded[key] = !this.expanded[key]; },

  async selectVersion(v: VersionMeta) {
    this.selectedVersion = v;
    this.versionInput = v.id;
    this.expandedLoader = {};
    this.selectedMainLoader = null;
    this.selectedLoaderVersions = {};

    const mcVer = v.id;
    await Promise.all([
      this.fetchLoaderVersions("forge", mcVer),
      this.fetchLoaderVersions("neoforge", mcVer),
      this.fetchLoaderVersions("fabric", mcVer),
      this.fetchLoaderVersions("quilt", mcVer),
      this.fetchLoaderVersions("optifine", mcVer),
    ]);
  },

  backToList() {
    this.selectedVersion = null;
  },

  toggleLoader(key: string) {
    this.expandedLoader[key] = !this.expandedLoader[key];
  },

  selectLoaderVer(loader: string, ver: string) {
    if (isMainLoader(loader)) {
      this.selectedMainLoader = loader;
      for (const ld of loaders) {
        if (isMainLoader(ld.key) && ld.key !== loader) {
          this.selectedLoaderVersions[ld.key] = "";
          delete this.selectedLoaderVersions[ld.key];
        }
      }
      this.selectedLoaderVersions[loader] = ver;
    } else {
      this.selectedLoaderVersions[loader] = ver;
    }
    this.versionInput = computedVersionName.value;
    this.expandedLoader[loader] = false;
  },

  clearLoaderVer(loader: string) {
    if (isMainLoader(loader)) this.selectedMainLoader = null;
    delete this.selectedLoaderVersions[loader];
    this.versionInput = this.selectedVersion?.id ?? "";
  },

  async fetchLoaderVersions(ld: string, mcVersion: string) {
    const key = `${ld}_${mcVersion}`;
    if (loaderDataCache.record[key]) return;
    // 去重键含 MC 版本：A 版本在途时切到 B 不得吞掉 B 的请求（否则 B 键永不写入）
    if (loaderInflight.has(key)) return;

    loaderInflight.add(key);
    this.loaderLoading[ld] = true;
    const cmdName = ({
      forge: "get_forge_versions",
      fabric: "get_fabric_versions",
      optifine: "get_optifine_versions",
      neoforge: "get_neoforge_versions",
      quilt: "get_quilt_versions",
    } as Record<string, string>)[ld];
    try {
      const versions = await invoke<string[]>(cmdName, { mcVersion });
      loaderDataCache.put(key, versions);
    } catch {
      loaderDataCache.put(key, []);
    } finally {
      loaderInflight.delete(key);
      this.loaderLoading[ld] = false;
    }
  },

  getLoaderVersions(ld: string, mcVersion: string): string[] {
    return loaderDataCache.record[`${ld}_${mcVersion}`] ?? [];
  },

  // ── Fabric API ──
  async fetchFabricApi(mcVer: string) {
    if (fabricApiDataCache.record[mcVer]) return;
    // 去重键为 MC 版本：切版本后 B 的请求不得被 A 的在途请求吞掉
    if (fabricApiInflight.has(mcVer)) return;
    fabricApiInflight.add(mcVer);
    this.fabricApiLoading = true;
    try {
      const versions = await invoke<FabricApiVer[]>("get_fabric_api_versions", { mcVersion: mcVer });
      fabricApiDataCache.put(mcVer, versions);
      // 回填前校验当前选中版本仍为请求的 MC 版本：迟到响应不得把 A 的 api 写进 B 的选择。
      if (
        versions.length > 0 &&
        !this.selectedLoaderVersions["fabric-api"] &&
        this.selectedVersion?.id === mcVer
      ) {
        this.selectedLoaderVersions["fabric-api"] = versions[0].version;
      }
    } catch {
      fabricApiDataCache.put(mcVer, []);
    } finally {
      fabricApiInflight.delete(mcVer);
      this.fabricApiLoading = false;
    }
  },

  toggleFabricApi() {
    this.fabricApiExpanded = !this.fabricApiExpanded;
  },

  selectFabricApi(ver: FabricApiVer) {
    this.selectedLoaderVersions["fabric-api"] = ver.version;
    this.fabricApiExpanded = false;
  },

  clearFabricApi() {
    delete this.selectedLoaderVersions["fabric-api"];
    const mc = this.selectedVersion?.id;
    if (mc) {
      const data = fabricApiDataCache.record[mc];
      if (data && data.length > 0) {
        this.selectedLoaderVersions["fabric-api"] = data[0].version;
      }
    }
  },

  getFabricApiVersions(mcVer: string): FabricApiVer[] {
    return fabricApiDataCache.record[mcVer] ?? [];
  },

  fabricApiUnready(): boolean {
    if (this.selectedMainLoader !== "fabric") return false;
    const sv = this.selectedVersion;
    if (!sv) return false;
    if (this.selectedLoaderVersions["fabric-api"]) return false;
    const data = fabricApiDataCache.record[sv.id];
    return this.fabricApiLoading || !data || data.length === 0;
  },

  async updateResolvedDirName() {
    const sv = this.selectedVersion;
    if (!sv) { this.resolvedDirName = ""; return; }
    const mainLd = this.selectedMainLoader;
    const mainVer = mainLd ? this.selectedLoaderVersions[mainLd] : undefined;
    const ofVer = this.selectedLoaderVersions["optifine"];
    const base = mainLd && mainVer
      ? `${sv.id}-${mainLd}-${mainVer}`
      : ofVer
        ? `${sv.id}-optifine-${ofVer}`
        : sv.id;
    try {
      this.resolvedDirName = await invoke<string>("resolve_version_name", { baseName: base });
    } catch { this.resolvedDirName = base; }
  },

  async startDownload() {
    const sv = this.selectedVersion;
    if (!sv) return;
    if (this.fabricApiUnready()) return;

    const mainLd = this.selectedMainLoader;
    const mainVer = mainLd ? this.selectedLoaderVersions[mainLd] : undefined;
    const ofVer = this.selectedLoaderVersions["optifine"];
    const faVer = this.selectedLoaderVersions["fabric-api"];
    let faUrl = "";
    let faVerStr = "";
    if (faVer) {
      const data = fabricApiDataCache.record[sv.id] ?? [];
      const found = data.find(v => v.version === faVer);
      if (found) {
        faUrl = found.download_url;
        faVerStr = found.version;
      }
    }

    this.dlSubmitting = true;
    // 提交前重新解析目标目录：watcher 异步更新有竞态，重名场景须拿到最新 _n，否则覆盖旧目录。
    await this.updateResolvedDirName();
    const taskId = await dlState.startTask(
      sv.id, sv.url, sv.id,
      mainLd ?? undefined, mainVer, ofVer, this.resolvedDirName || undefined,
      faUrl || undefined, faVerStr || undefined,
    );
    // 注册安装目录名：下载完成后 App.vue 自动选中该版本（Mod 下载目标）
    if (taskId) {
      registerVersionTarget(taskId, this.resolvedDirName || sv.id);
    }
    this.dlSubmitting = false;
    this.selectedVersion = null;
  },
});

// ── 派生数据 ──

export const vanillaIcon = CATEGORIES[0].icon;

export const displayLoaders = computed<LoaderItem[]>(() => {
  const list: LoaderItem[] = [];
  for (const ld of loaders) {
    list.push(ld);
    // fabric-api 紧接 fabric 注入（严格顺序）
    if (ld.key === "fabric" && vdState.selectedMainLoader === "fabric") {
      list.push({ key: "fabric-api", name: "Fabric API", icon: loaderIcon.fabric });
    }
  }
  return list;
});

export const latestRelease = computed(() =>
  vdState.manifest?.versions.find(v => v.id === vdState.manifest?.latest.release) ?? null
);
export const latestSnapshot = computed(() =>
  vdState.manifest?.versions.find(v => v.id === vdState.manifest?.latest.snapshot) ?? null
);
export const releases = computed(() =>
  vdState.manifest?.versions.filter(v => v.type === "release") ?? []
);
export const snapshots = computed(() =>
  vdState.manifest?.versions.filter(v => v.type === "snapshot" && !isAprilFools(v)) ?? []
);
export const oldVersions = computed(() =>
  vdState.manifest?.versions.filter(v => v.type.startsWith("old_")) ?? []
);
export const aprilFools = computed(() =>
  vdState.manifest?.versions.filter(v => v.type === "snapshot" && isAprilFools(v)) ?? []
);

export const computedVersionName = computed(() => {
  const base = vdState.selectedVersion?.id ?? "";
  const mainLd = vdState.selectedMainLoader;
  const mainVer = mainLd ? vdState.selectedLoaderVersions[mainLd] : "";
  if (mainVer) {
    const label = loaders.find(l => l.key === mainLd)?.name ?? mainLd;
    return `${base}-${label} ${mainVer}`;
  }
  return base;
});

// ── 响应式联动（模块级 watch，生命周期与应用一致） ──

watch(
  () => [vdState.selectedVersion, vdState.selectedLoaderVersions, vdState.selectedMainLoader],
  () => vdState.updateResolvedDirName(),
  { deep: true },
);

watch(() => vdState.selectedMainLoader, (ld) => {
  const mc = vdState.selectedVersion?.id;
  // OptiFabric 仅支持至 1.20.x：Fabric/Quilt + 1.21+ 时清掉 OptiFine 残留
  if ((ld === "fabric" || ld === "quilt") && mc && versionGte(mc, "1.21")) {
    delete vdState.selectedLoaderVersions["optifine"];
  }
  if (ld === "fabric" && mc) {
    vdState.fetchFabricApi(mc);
  } else if (ld !== "fabric") {
    delete vdState.selectedLoaderVersions["fabric-api"];
  }
});

watch(() => vdState.selectedVersion?.id, (mc) => {
  if (vdState.selectedMainLoader === "fabric" && mc) {
    delete vdState.selectedLoaderVersions["fabric-api"];
    fabricApiDataCache.clear();
    vdState.fetchFabricApi(mc);
  }
});
