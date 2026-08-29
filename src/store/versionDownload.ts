// 原版版本下载流程 store（manifest 列表 / 版本选择 / ModLoader 选择 / Fabric API / 提交下载）：
// 模块级 reactive 单例，页面与组件直接 import。
// 状态跨路由保留（切到设置页再返回不丢选择进度）；manifest 缓存跨组件重建保留。
import { reactive, computed, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { dlState, registerVersionTarget } from "./download";
import { CATEGORIES } from "../utils/versionMeta";
import { errMsg } from "../utils/error";

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

// 模块级 manifest 缓存：仅首次进入下载页拉取版本列表，之后直接展示缓存
let manifestLoaded = false;
let manifestLoading = false;
let cachedManifest: VersionManifest | null = null;

export const loaderIcon: Record<string, string> = Object.fromEntries(
  CATEGORIES.filter(c => c.key !== "vanilla").map(c => [c.key, c.icon]),
);

const loaders: LoaderItem[] = CATEGORIES
  .filter(c => c.key !== "vanilla")
  .map(c => ({ key: c.key, name: c.title, icon: c.icon }));

const loaderMinVersion: Record<string, string> = {
  forge: "1.1", neoforge: "1.20.2", fabric: "1.14", quilt: "1.14.4",
  optifine: "1.2.5",
};

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
  // 初始加载态由缓存状态决定：已有缓存则首帧直接展示列表，不闪加载卡片
  loading: !manifestLoaded,
  error: "",
  expanded: {
    release: false, snapshot: false, old: false, aprilfools: false,
  } as Record<string, boolean>,

  selectedVersion: null as VersionMeta | null,
  versionInput: "",
  resolvedDirName: "",

  // Loader version data cache
  loaderData: {} as Record<string, string[]>,
  loaderLoading: {} as Record<string, boolean>,
  // Selected loader: only one of forge/neoforge/fabric at a time
  selectedMainLoader: null as string | null,
  selectedLoaderVersions: {} as Record<string, string>,
  expandedLoader: {} as Record<string, boolean>,

  // Fabric API state
  fabricApiData: {} as Record<string, FabricApiVer[]>,
  fabricApiLoading: false,
  fabricApiExpanded: false,

  dlSubmitting: false,

  isAprilFools,
  loaderSupported(ld: string, mcVersion: string): boolean {
    const min = loaderMinVersion[ld];
    if (min && !versionGte(mcVersion, min)) return false;
    return true;
  },
  showLoader(ld: string): boolean {
    const mc = this.selectedVersion?.id;
    if (mc && !this.loaderSupported(ld, mc)) return false;
    if (!isMainLoader(ld)) {
      if (!this.selectedMainLoader) return false;
      // OptiFabric 桥接仅支持至 1.20.x：Fabric/Quilt + 1.21+ 组合直接过滤
      // （安装时也会明确报错，这里在下载流程提前隐藏）
      if (ld === "optifine" && mc && versionGte(mc, "1.21")) {
        const main = this.selectedMainLoader;
        if (main === "fabric" || main === "quilt") return false;
      }
      return true;
    }
    if (!this.selectedMainLoader) return true;
    return ld === this.selectedMainLoader;
  },

  async fetchManifest() {
    if (manifestLoading) return;
    // 已有缓存：直接展示，不重新请求（loading 初始值已按缓存状态设定）
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

    // Fetch all loader versions in parallel
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
    // If main loader, clear other main loaders and their selections
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
      // OptiFine - just set
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
    if (this.loaderData[key]) return;
    if (this.loaderLoading[ld]) return;

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
      this.loaderData[key] = versions;
    } catch {
      this.loaderData[key] = [];
    } finally {
      this.loaderLoading[ld] = false;
    }
  },

  getLoaderVersions(ld: string, mcVersion: string): string[] {
    return this.loaderData[`${ld}_${mcVersion}`] ?? [];
  },

  // ── Fabric API ──
  async fetchFabricApi(mcVer: string) {
    if (this.fabricApiData[mcVer]) return;
    if (this.fabricApiLoading) return;
    this.fabricApiLoading = true;
    try {
      const versions = await invoke<FabricApiVer[]>("get_fabric_api_versions", { mcVersion: mcVer });
      this.fabricApiData[mcVer] = versions;
      if (versions.length > 0 && !this.selectedLoaderVersions["fabric-api"]) {
        this.selectedLoaderVersions["fabric-api"] = versions[0].version;
      }
    } catch {
      this.fabricApiData[mcVer] = [];
    } finally {
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
    // ponytail: re-auto-select latest if data exists
    const mc = this.selectedVersion?.id;
    if (mc) {
      const data = this.fabricApiData[mc];
      if (data && data.length > 0) {
        this.selectedLoaderVersions["fabric-api"] = data[0].version;
      }
    }
  },

  getFabricApiVersions(mcVer: string): FabricApiVer[] {
    return this.fabricApiData[mcVer] ?? [];
  },

  /** 选了 Fabric 但 Fabric API 列表未就绪（加载中/拉取失败/未选择）时禁止下载。 */
  fabricApiUnready(): boolean {
    if (this.selectedMainLoader !== "fabric") return false;
    const sv = this.selectedVersion;
    if (!sv) return false;
    if (this.selectedLoaderVersions["fabric-api"]) return false;
    const data = this.fabricApiData[sv.id];
    return this.fabricApiLoading || !data || data.length === 0;
  },

  async updateResolvedDirName() {
    const sv = this.selectedVersion;
    if (!sv) { this.resolvedDirName = ""; return; }
    const mainLd = this.selectedMainLoader;
    const mainVer = mainLd ? this.selectedLoaderVersions[mainLd] : undefined;
    const base = mainLd && mainVer ? `${sv.id}-${mainLd}-${mainVer}` : sv.id;
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
      const data = this.fabricApiData[sv.id] ?? [];
      const found = data.find(v => v.version === faVer);
      if (found) {
        faUrl = found.download_url;
        faVerStr = found.version;
      }
    }

    this.dlSubmitting = true;
    // 提交前重新解析目标目录：watcher 的异步更新有竞态（选版本后立即点下载时
    // resolve invoke 未返回，缓存旧值指向已存在的同名目录）——重名场景必须
    // 拿到最新 _n，否则 json/jar/fabric-api 全部落盘到旧目录（覆盖已有版本）
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

/** 原版/正式版图标（数据源在 versionMeta）。 */
export const vanillaIcon = CATEGORIES[0].icon;

export const displayLoaders = computed<LoaderItem[]>(() => {
  const list: LoaderItem[] = [];
  for (const ld of loaders) {
    list.push(ld);
    // Injects fabric-api right after fabric for strict ordering
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

// 版本/loader 选择变化 → 刷新目录名
watch(
  () => [vdState.selectedVersion, vdState.selectedLoaderVersions, vdState.selectedMainLoader],
  () => vdState.updateResolvedDirName(),
  { deep: true },
);

// Auto-fetch Fabric API when Fabric is selected

watch(() => vdState.selectedMainLoader, (ld) => {
  // 未选主 ModLoader 时隐藏 OptiFine：同时清掉残留选择，防止被静默提交
  if (!ld) delete vdState.selectedLoaderVersions["optifine"];
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
    vdState.fabricApiData = {};
    vdState.fetchFabricApi(mc);
  }
});
