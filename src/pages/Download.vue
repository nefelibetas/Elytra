<script lang="ts">
// 模块级缓存：仅首次进入下载页拉取版本列表，之后进入直接展示缓存；
// 刷新按钮手动刷新当前 tab（vanilla 重新拉取）。
// （必须放在普通 <script> 块：<script setup> 的顶层变量每次组件重建都会重置）
interface VersionMeta {
  id: string;
  type: string;
  url: string;
  time: string;
  releaseTime: string;
  sha1: string;
  complianceLevel: number;
}
interface LatestVersion { release: string; snapshot: string; }
interface VersionManifest { latest: LatestVersion; versions: VersionMeta[]; }
let manifestLoaded = false;
let manifestLoading = false;
let cachedManifest: VersionManifest | null = null;
</script>

<script setup lang="ts">
import { ref, computed, watch, onMounted } from "vue";
import { useRouter } from "vue-router";
import { invoke } from "@tauri-apps/api/core";
import { dlState, ensureListen } from "../store/download";
import DownloadTab from "../component/DownloadTab.vue";
import VersionCard from "../component/VersionCard.vue";
import VersionItem from "../component/VersionItem.vue";
import grassIcon from "../assets/grass.png";
import commandIcon from "../assets/command.png";
import cobbleIcon from "../assets/cobble.png";
import emeraldIcon from "../assets/emerald.png";
import forgeIcon from "../assets/forge.png";
import neoForgeIcon from "../assets/neoForge.png";
import fabricIcon from "../assets/Fabric.png";
import optifineIcon from "../assets/optifined.png";
import quiltIcon from "../assets/quilt.png";

interface LoaderItem {
  key: string;
  name: string;
  icon: string;
}

type Tab = "vanilla" | "mod" | "modpack" | "datapack" | "resourcepack" | "shader";

const currentTab = ref<Tab>("vanilla");
const manifest = ref<VersionManifest | null>(cachedManifest);
// 初始加载态由缓存状态决定：已有缓存则首帧直接展示列表，不闪加载卡片
const loading = ref(!manifestLoaded);
const error = ref("");
const expanded = ref<Record<string, boolean>>({
  release: false, snapshot: false, old: false, aprilfools: false,
});

const selectedVersion = ref<VersionMeta | null>(null);
const versionInput = ref("");
const resolvedDirName = ref("");

const loaderIcon: Record<string, string> = {
  forge: forgeIcon, neoforge: neoForgeIcon, fabric: fabricIcon,
  optifine: optifineIcon, quilt: quiltIcon,
};

const loaders: LoaderItem[] = [
  { key: "forge", name: "Forge", icon: forgeIcon },
  { key: "neoforge", name: "NeoForge", icon: neoForgeIcon },
  { key: "fabric", name: "Fabric", icon: fabricIcon },
  { key: "quilt", name: "Quilt", icon: quiltIcon },
  { key: "optifine", name: "OptiFine", icon: optifineIcon },
  { key: "liteloader", name: "LiteLoader", icon: "" },
];

// Loader version data cache
const loaderData = ref<Record<string, string[]>>({});
const loaderLoading = ref<Record<string, boolean>>({});

// Selected loader: only one of forge/neoforge/fabric at a time
const selectedMainLoader = ref<string | null>(null);
const selectedLoaderVersions = ref<Record<string, string>>({});
const expandedLoader = ref<Record<string, boolean>>({});

async function updateResolvedDirName() {
  const sv = selectedVersion.value;
  if (!sv) { resolvedDirName.value = ""; return; }
  const mainLd = selectedMainLoader.value;
  const mainVer = mainLd ? selectedLoaderVersions.value[mainLd] : undefined;
  const base = mainLd && mainVer ? `${sv.id}-${mainLd}-${mainVer}` : sv.id;
  try {
    resolvedDirName.value = await invoke<string>("resolve_version_name", { baseName: base });
  } catch { resolvedDirName.value = base; }
}

watch([selectedVersion, selectedLoaderVersions, selectedMainLoader], updateResolvedDirName, { deep: true });

function isMainLoader(key: string) { return !["optifine", "liteloader"].includes(key); }

// Fetch loader versions from API
async function fetchLoaderVersions(ld: string, mcVersion: string) {
  const key = `${ld}_${mcVersion}`;
  if (loaderData.value[key]) return;
  if (loaderLoading.value[ld]) return;

  loaderLoading.value[ld] = true;
  const cmdName = ({
    forge: "get_forge_versions",
    fabric: "get_fabric_versions",
    optifine: "get_optifine_versions",
    neoforge: "get_neoforge_versions",
    quilt: "get_quilt_versions",
    liteloader: "get_liteloader_versions",
  } as Record<string, string>)[ld];
  try {
    const versions = await invoke<string[]>(cmdName, { mcVersion });
    loaderData.value[key] = versions;
  } catch {
    loaderData.value[key] = [];
  } finally {
    loaderLoading.value[ld] = false;
  }
}

function getLoaderVersions(ld: string, mcVersion: string): string[] {
  return loaderData.value[`${ld}_${mcVersion}`] ?? [];
}

function isAprilFools(v: VersionMeta): boolean {
  return v.releaseTime?.substring(5, 10) === "04-01";
}

const latestRelease = computed(() =>
  manifest.value?.versions.find(v => v.id === manifest.value?.latest.release) ?? null
);
const latestSnapshot = computed(() =>
  manifest.value?.versions.find(v => v.id === manifest.value?.latest.snapshot) ?? null
);
const releases = computed(() =>
  manifest.value?.versions.filter(v => v.type === "release") ?? []
);
const snapshots = computed(() =>
  manifest.value?.versions.filter(v => v.type === "snapshot" && !isAprilFools(v)) ?? []
);
const oldVersions = computed(() =>
  manifest.value?.versions.filter(v => v.type.startsWith("old_")) ?? []
);
const aprilFools = computed(() =>
  manifest.value?.versions.filter(v => v.type === "snapshot" && isAprilFools(v)) ?? []
);

function toggle(key: string) { expanded.value[key] = !expanded.value[key]; }

function fmtDateTime(iso: string): string {
  if (!iso) return "";
  return iso.substring(0, 10) + " " + iso.substring(11, 19);
}

async function selectVersion(v: VersionMeta) {
  selectedVersion.value = v;
  versionInput.value = v.id;
  expandedLoader.value = {};
  selectedMainLoader.value = null;
  selectedLoaderVersions.value = {};

  // Fetch all loader versions in parallel
  const mcVer = v.id;
  await Promise.all([
    fetchLoaderVersions("forge", mcVer),
    fetchLoaderVersions("neoforge", mcVer),
    fetchLoaderVersions("fabric", mcVer),
    fetchLoaderVersions("quilt", mcVer),
    fetchLoaderVersions("optifine", mcVer),
    fetchLoaderVersions("liteloader", mcVer),
  ]);
}

function backToList() {
  selectedVersion.value = null;
}

function toggleLoader(key: string) {
  expandedLoader.value[key] = !expandedLoader.value[key];
}

function selectLoaderVer(loader: string, ver: string) {
  // If main loader, clear other main loaders and their selections
  if (isMainLoader(loader)) {
    selectedMainLoader.value = loader;
    // Clear other main loaders
    for (const ld of loaders) {
      if (isMainLoader(ld.key) && ld.key !== loader) {
        selectedLoaderVersions.value[ld.key] = "";
        delete selectedLoaderVersions.value[ld.key];
      }
    }
    selectedLoaderVersions.value[loader] = ver;
  } else {
    // OptiFine - just set
    selectedLoaderVersions.value[loader] = ver;
  }
  versionInput.value = computedVersionName.value;
  expandedLoader.value[loader] = false;
}

function clearLoaderVer(loader: string) {
  if (isMainLoader(loader)) selectedMainLoader.value = null;
  delete selectedLoaderVersions.value[loader];
  versionInput.value = selectedVersion.value?.id ?? "";
}

const computedVersionName = computed(() => {
  const base = selectedVersion.value?.id ?? "";
  const mainLd = selectedMainLoader.value;
  const mainVer = mainLd ? selectedLoaderVersions.value[mainLd] : "";
  if (mainVer) {
    const label = loaders.find(l => l.key === mainLd)?.name ?? mainLd;
    return `${base}-${label} ${mainVer}`;
  }
  return base;
});

const router = useRouter();

function versionIcon(v: VersionMeta): string {
  if (v.type === "release") return grassIcon;
  if (v.type === "snapshot") return commandIcon;
  return cobbleIcon;
}

const dlSubmitting = ref(false);

const loaderMinVersion: Record<string, string> = {
  forge: "1.1", neoforge: "1.20.2", fabric: "1.14", quilt: "1.14.4",
  optifine: "1.2.5", liteloader: "1.5.2",
};
const loaderMaxVersion: Record<string, string> = {
  liteloader: "1.12.2",
};

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

function versionGt(ver: string, other: string): boolean {
  const a = versionTuple(ver);
  const b = versionTuple(other);
  for (let i = 0; i < Math.max(a.length, b.length); i++) {
    const va = a[i] ?? 0;
    const vb = b[i] ?? 0;
    if (va !== vb) return va > vb;
  }
  return false;
}

function loaderSupported(ld: string, mcVersion: string): boolean {
  const min = loaderMinVersion[ld];
  const max = loaderMaxVersion[ld];
  if (min && !versionGte(mcVersion, min)) return false;
  if (max && versionGt(mcVersion, max)) return false;
  return true;
}

function showLoader(ld: string): boolean {
  const mc = selectedVersion.value?.id;
  if (mc && !loaderSupported(ld, mc)) return false;
  if (!isMainLoader(ld)) return true;
  if (!selectedMainLoader.value) return true;
  return ld === selectedMainLoader.value;
}

async function startDownload() {
  const sv = selectedVersion.value;
  if (!sv) return;
  ensureListen();

  const mainLd = selectedMainLoader.value;
  const mainVer = mainLd ? selectedLoaderVersions.value[mainLd] : undefined;
  const ofVer = selectedLoaderVersions.value["optifine"];

  dlSubmitting.value = true;
  await dlState.startTask(
    sv.id, sv.url, sv.id, router.currentRoute.value.fullPath,
    mainLd ?? undefined, mainVer, ofVer, resolvedDirName.value || undefined,
  );
  dlSubmitting.value = false;
  selectedVersion.value = null;
}

async function refreshData(tab: string) {
  if (tab === "vanilla") {
    selectedVersion.value = null;
    expanded.value = { release: false, snapshot: false, old: false, aprilfools: false };
    await fetchManifest();
  }
}

async function fetchManifest() {
  if (manifestLoading) return;
  manifestLoading = true;
  loading.value = true;
  error.value = "";
  try {
    manifest.value = await invoke<VersionManifest>("get_version_list");
    cachedManifest = manifest.value;
    manifestLoaded = true;
  } catch (e) {
    error.value = String(e);
  } finally {
    loading.value = false;
    manifestLoading = false;
  }
}

onMounted(() => {
  // 已有缓存：直接展示，不重新请求（loading 初始值已按缓存状态设定）
  if (manifestLoaded) return;
  fetchManifest();
});
</script>

<template>
  <div class="dl">
    <nav class="dl-nav">
      <DownloadTab :active="currentTab === 'vanilla'" @click="currentTab = 'vanilla'" @refresh="refreshData('vanilla')">
        <template #icon>
          <svg viewBox="0 0 16 16" width="16" height="16" fill="currentColor"><path d="M2 2h5v5H2V2zm7 0h5v5H9V2zM2 9h5v5H2V9zm7 0h5v5H9V9z" /></svg>
        </template>
        原版游戏
      </DownloadTab>
      <div class="nav-divider"><span>社区资源</span></div>
      <DownloadTab :active="currentTab === 'mod'" @click="currentTab = 'mod'" @refresh="refreshData('mod')">
        <template #icon><svg viewBox="0 0 16 16" width="16" height="16" fill="currentColor"><path d="M3 2a1 1 0 00-1 1v2a1 1 0 001 1h2a1 1 0 001-1V3a1 1 0 00-1-1H3zm8 0a1 1 0 00-1 1v2a1 1 0 001 1h2a1 1 0 001-1V3a1 1 0 00-1-1h-2zM3 10a1 1 0 00-1 1v2a1 1 0 001 1h2a1 1 0 001-1v-2a1 1 0 00-1-1H3zm8 0a1 1 0 00-1 1v2a1 1 0 001 1h2a1 1 0 001-1v-2a1 1 0 00-1-1h-2z" /></svg></template>Mod
      </DownloadTab>
      <DownloadTab :active="currentTab === 'modpack'" @click="currentTab = 'modpack'" @refresh="refreshData('modpack')">
        <template #icon><svg viewBox="0 0 16 16" width="16" height="16" fill="currentColor"><path d="M2 3a1 1 0 011-1h4a1 1 0 011 1v4a1 1 0 01-1 1H3a1 1 0 01-1-1V3zm6 0a1 1 0 011-1h4a1 1 0 011 1v4a1 1 0 01-1 1h-4a1 1 0 01-1-1V3zM2 9a1 1 0 011-1h4a1 1 0 011 1v4a1 1 0 01-1 1H3a1 1 0 01-1-1V9zm6 0a1 1 0 011-1h4a1 1 0 011 1v4a1 1 0 01-1 1h-4a1 1 0 01-1-1V9z" /></svg></template>整合包
      </DownloadTab>
      <DownloadTab :active="currentTab === 'datapack'" @click="currentTab = 'datapack'" @refresh="refreshData('datapack')">
        <template #icon><svg viewBox="0 0 16 16" width="16" height="16" fill="currentColor"><path d="M4 2h8a1 1 0 011 1v2a1 1 0 01-1 1H4a1 1 0 01-1-1V3a1 1 0 011-1zm0 4h8a1 1 0 011 1v6a1 1 0 01-1 1H4a1 1 0 01-1-1V7a1 1 0 011-1zm1 2v4h6V8H5z" /></svg></template>数据包
      </DownloadTab>
      <DownloadTab :active="currentTab === 'resourcepack'" @click="currentTab = 'resourcepack'" @refresh="refreshData('resourcepack')">
        <template #icon><svg viewBox="0 0 16 16" width="16" height="16" fill="currentColor"><circle cx="8" cy="8" r="6" fill="none" stroke="currentColor" stroke-width="1.2" /><circle cx="8" cy="8" r="2" /></svg></template>资源包
      </DownloadTab>
      <DownloadTab :active="currentTab === 'shader'" @click="currentTab = 'shader'" @refresh="refreshData('shader')">
        <template #icon><svg viewBox="0 0 16 16" width="16" height="16" fill="currentColor"><path d="M8 1a1 1 0 011 1v.5a1 1 0 01-2 0V2a1 1 0 011-1zM4.5 4.5a1 1 0 011.4 0l.4.3a1 1 0 01-1.4 1.4l-.4-.3a1 1 0 010-1.4zm7 0a1 1 0 010 1.4l-.4.3a1 1 0 11-1.4-1.4l.4-.3a1 1 0 011.4 0zM2 7a1 1 0 011 1v.5a1 1 0 01-2 0V8a1 1 0 011-1zm12 0a1 1 0 011 1v.5a1 1 0 01-2 0V8a1 1 0 011-1zm-6 2a1 1 0 011 1v.5a1 1 0 01-2 0V10a1 1 0 011-1z" /></svg></template>光影包
      </DownloadTab>
    </nav>

    <div class="dl-body">
      <Transition name="page" mode="out-in">
        <div v-if="loading" key="loading" class="dl-page">
          <div class="loading-card" style="margin: auto;"><div class="spinner"></div><p>正在获取版本列表</p></div>
        </div>

        <div v-else-if="currentTab === 'vanilla' && !selectedVersion" key="list" class="dl-page">
          <div v-if="error" class="state err">{{ error }}</div>
          <template v-else-if="manifest">
            <div class="card">
              <div class="card-title">最新版本</div>
              <div class="card-body">
                <VersionItem :icon-src="grassIcon" :version-id="manifest.latest.release" :subtitle="'最新正式版，发布于 ' + (latestRelease ? fmtDateTime(latestRelease.releaseTime) : '')" @select="selectVersion(latestRelease!)" />
                <VersionItem :icon-src="commandIcon" :version-id="manifest.latest.snapshot" :subtitle="'最新预览版，发布于 ' + (latestSnapshot ? fmtDateTime(latestSnapshot.releaseTime) : '')" @select="selectVersion(latestSnapshot!)" />
              </div>
            </div>
            <VersionCard title="正式版" :count="releases.length" :expanded="expanded.release" @toggle="toggle('release')">
              <VersionItem v-for="v in releases" :key="v.id" :icon-src="grassIcon" :version-id="v.id" :subtitle="fmtDateTime(v.releaseTime)" @select="selectVersion(v)" />
            </VersionCard>
            <VersionCard title="预览版" :count="snapshots.length" :expanded="expanded.snapshot" @toggle="toggle('snapshot')">
              <VersionItem v-for="v in snapshots" :key="v.id" :icon-src="commandIcon" :version-id="v.id" :subtitle="fmtDateTime(v.releaseTime)" @select="selectVersion(v)" />
            </VersionCard>
            <VersionCard title="远古版" :count="oldVersions.length" :expanded="expanded.old" @toggle="toggle('old')">
              <VersionItem v-for="v in oldVersions" :key="v.id" :icon-src="cobbleIcon" :version-id="v.id" :subtitle="fmtDateTime(v.releaseTime)" :badge="v.type" :badge-class="v.type" @select="selectVersion(v)" />
            </VersionCard>
            <VersionCard title="愚人节版" :count="aprilFools.length" :expanded="expanded.aprilfools" @toggle="toggle('aprilfools')">
              <VersionItem v-for="v in aprilFools" :key="v.id" :icon-src="emeraldIcon" :version-id="v.id" :subtitle="fmtDateTime(v.releaseTime)" @select="selectVersion(v)" />
            </VersionCard>
          </template>
        </div>

        <div v-else-if="currentTab === 'vanilla' && selectedVersion" key="detail" class="dl-page">
          <div class="det-card">
            <div class="det-hdr">
              <button class="det-back" @click="backToList" title="返回">
                <svg viewBox="0 0 16 16" width="16" height="16" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M10 2L4 8l6 6" /></svg>
              </button>
              <img :src="selectedMainLoader && loaderIcon[selectedMainLoader] ? loaderIcon[selectedMainLoader] : versionIcon(selectedVersion)" width="28" height="28" class="det-icon" alt="" />
              <input class="det-input" v-model="versionInput" />
              <span v-if="resolvedDirName" class="det-dir">目录: {{ resolvedDirName }}</span>
            </div>
          </div>

          <TransitionGroup name="ld">
            <div v-for="ld in loaders" :key="ld.key" v-show="showLoader(ld.key)" class="det-card det-ld">
              <button class="det-ld-hdr" :class="{ clickable: getLoaderVersions(ld.key, selectedVersion.id).length }"
                @click="getLoaderVersions(ld.key, selectedVersion.id).length && toggleLoader(ld.key)">
                <img v-if="loaderIcon[ld.key]" :src="loaderIcon[ld.key]" width="32" height="32" class="det-ld-icon" alt="" />
                <span class="det-ld-info">
                  <span class="det-ld-title">{{ ld.name }}</span>
                  <span class="det-ld-sub">
                    <template v-if="selectedLoaderVersions[ld.key]">{{ ld.name }} {{ selectedLoaderVersions[ld.key] }}</template>
                    <template v-else-if="loaderLoading[ld.key]">正在加载...</template>
                    <template v-else-if="getLoaderVersions(ld.key, selectedVersion.id).length">可以添加</template>
                    <template v-else>无</template>
                  </span>
                </span>
                <span v-if="selectedLoaderVersions[ld.key] || getLoaderVersions(ld.key, selectedVersion.id).length" class="det-ld-actions">
                  <button v-if="selectedLoaderVersions[ld.key]" class="det-ld-remove" @click.stop="clearLoaderVer(ld.key)" title="移除">
                    <svg viewBox="0 0 12 12" width="10" height="10" fill="currentColor">
                      <polygon points="1,1 5,5 9,1 11,3 7,7 11,11 9,13 5,9 1,13 -1,11 3,7 -1,3" />
                    </svg>
                  </button>
                  <svg v-if="getLoaderVersions(ld.key, selectedVersion.id).length" class="det-ld-arrow" :class="{ open: expandedLoader[ld.key] }" viewBox="0 0 12 12" width="12" height="12" fill="currentColor">
                    <polygon points="1,3 11,3 6,10" />
                  </svg>
                </span>
              </button>
              <Transition name="expand">
                <div v-if="expandedLoader[ld.key] && getLoaderVersions(ld.key, selectedVersion.id).length" class="det-ld-body">
                  <div v-for="ver in getLoaderVersions(ld.key, selectedVersion.id)" :key="ver"
                      class="det-ld-ver"
                      @click="selectLoaderVer(ld.key, ver)">
                    <img v-if="loaderIcon[ld.key]" :src="loaderIcon[ld.key]" width="24" height="24" class="det-ld-ver-icon" alt="" />
                    <span class="det-ld-ver-text">{{ ver }}</span>
                  </div>
                </div>
              </Transition>
            </div>
          </TransitionGroup>

          <div class="dl-spacer"></div>
          <div class="dl-footer">
            <button v-if="!dlSubmitting" class="dl-btn" @click="startDownload">
              开始下载
            </button>
            <p v-else class="dl-done">已加入下载队列</p>
          </div>
        </div>

        <div v-else key="other" class="dl-page">
          <h2>{{ currentTab }}</h2>
        </div>
      </Transition>
    </div>
  </div>
</template>

<style scoped>
.dl { display: flex; height: 100%; }
.dl-nav {
  flex: 0 0 180px; min-width: 160px;
  background-color: var(--surface-elevated);
  border-right: 1px solid var(--border-default);
  display: flex; flex-direction: column;
  padding: 16px 12px; gap: 2px;
}
.nav-divider {
  display: flex; align-items: center;
  padding: 12px 12px 6px; margin-top: 8px; user-select: none;
}
.nav-divider span {
  font-size: 11px; font-weight: 500;
  color: var(--text-tertiary);
  text-transform: uppercase; letter-spacing: 0.8px;
}
.dl-body {
  flex: 1; position: relative; padding: 24px; overflow-y: auto; overflow-x: hidden; scrollbar-gutter: stable;
  background: var(--surface-page);
}
.dl-page {
  display: flex; flex-direction: column; gap: 16px; min-height: 100%;
  background: var(--surface-page);
}
.state.err { color: #e74c3c; font-size: 14px; text-align: center; padding: 40px; }
.loading-card {
  display: flex; flex-direction: column; align-items: center; gap: 16px;
  padding: 40px 56px; background-color: var(--surface-card);
  border-radius: var(--radius-lg); box-shadow: var(--shadow-md);
}
.loading-card p { font-size: 14px; color: var(--text-secondary); }
.spinner {
  width: 36px; height: 36px;
  border: 3px solid var(--border-default); border-top-color: var(--color-brand);
  border-radius: 50%; animation: spin 0.7s linear infinite;
}
@keyframes spin { to { transform: rotate(360deg); } }

.card { background-color: var(--surface-card); border: 1px solid var(--border-default); border-radius: var(--radius-md); }
.card-title {
  display: flex; align-items: center; gap: 8px;
  padding: 12px 16px; font-size: 14px; font-weight: 600;
  color: var(--text-primary); user-select: none; outline: none;
}

.det-card { background-color: var(--surface-card); border: 1px solid var(--border-default); border-radius: var(--radius-md); overflow: hidden; }
.det-hdr { display: flex; align-items: center; gap: 12px; padding: 10px 16px; }
.det-back {
  width: 32px; height: 32px;
  display: flex; align-items: center; justify-content: center;
  border: none; background: transparent; color: var(--text-secondary);
  cursor: pointer; border-radius: var(--radius-sm); transition: all 0.15s; flex-shrink: 0;
}
.det-back:hover { background-color: var(--surface-hover); color: var(--text-primary); }
.det-icon { flex-shrink: 0; border-radius: 3px; width: 28px; height: 28px; }
.det-input {
  flex: 1; height: 32px; padding: 0 10px;
  border: 1px solid var(--border-default); border-radius: var(--radius-sm);
  background: var(--surface-page); color: var(--text-primary);
  font-size: 13px; font-weight: 600; outline: none; cursor: text; transition: border-color 0.15s;
}
.det-dir {
  flex: 0 0 auto; font-size: 11px; color: var(--text-tertiary);
  white-space: nowrap; user-select: none;
}
.det-input:focus { border-color: var(--color-brand); }

.det-ld-hdr {
  width: 100%; display: flex; align-items: center; gap: 12px;
  padding: 10px 16px; border: none; background: transparent;
  color: var(--text-primary); font-size: 13px;
  cursor: default; transition: background 0.1s; outline: none; user-select: none;
}
.det-ld-hdr.clickable { cursor: pointer; }
.det-ld-hdr.clickable:hover { background-color: var(--surface-hover); }
.det-ld-icon { flex-shrink: 0; border-radius: 3px; width: 32px; height: 32px; }
.det-ld-info { flex: 1; display: flex; flex-direction: column; gap: 2px; min-width: 0; text-align: left; }
.det-ld-title { font-weight: 600; color: var(--text-primary); font-size: 13px; }
.det-ld-sub { font-size: 11px; color: var(--text-tertiary); }
.det-ld-actions { display: flex; align-items: center; gap: 2px; margin-left: auto; }
.det-ld-remove {
  width: 24px; height: 24px;
  display: flex; align-items: center; justify-content: center;
  border: none; background: transparent;
  color: var(--text-tertiary);
  cursor: pointer; border-radius: var(--radius-sm);
  transition: all 0.15s; flex-shrink: 0;
}
.det-ld-remove:hover { color: #e74c3c; background-color: rgba(231, 76, 60, 0.1); }
.det-ld-arrow { flex-shrink: 0; color: var(--text-tertiary); transition: transform 0.2s; }
.det-ld-arrow.open { transform: rotate(180deg); }
.det-ld-ver {
  display: flex; align-items: center; gap: 12px; padding: 8px 16px;
  font-size: 13px; color: var(--text-primary); cursor: pointer;
  transition: background 0.1s; user-select: none;
}
.det-ld-ver:hover { background-color: var(--surface-hover); }
.det-ld-ver-icon { flex-shrink: 0; border-radius: 3px; width: 24px; height: 24px; }
.det-ld-ver-text { font-weight: 500; }
.det-ld-none { padding: 12px 16px; font-size: 13px; color: var(--text-tertiary); text-align: center; }

.dl-spacer { flex: 1; min-height: 12px; }
.dl-footer {
  display: flex; flex-direction: column; align-items: center; gap: 8px;
  position: sticky; bottom: 0; z-index: 5; flex-shrink: 0;
}
.dl-btn {
  padding: 10px 40px;
  border: none; border-radius: var(--radius-lg);
  background-color: var(--color-brand);
  color: #fff; font-size: 14px; font-weight: 600;
  cursor: pointer; transition: all 0.15s;
  align-self: center; display: inline-flex; align-items: center; gap: 8px;
}
.dl-btn:disabled { opacity: 0.6; cursor: not-allowed; }
.dl-btn:hover:not(:disabled) { background-color: var(--color-brand-hover, #4f46e5); box-shadow: var(--shadow-md); }
.dl-btn:active:not(:disabled) { transform: scale(0.97); }
.dl-spin {
  width: 14px; height: 14px;
  border: 2px solid rgba(255,255,255,0.4); border-top-color: #fff;
  border-radius: 50%; animation: dl-spin 0.6s linear infinite;
}
@keyframes dl-spin { to { transform: rotate(360deg); } }
.dl-done { font-size: 14px; font-weight: 600; color: #059669; text-align: center; padding: 10px 40px; }
.dl-err { font-size: 13px; color: #e74c3c; text-align: center; max-width: 100%; word-break: break-all; padding: 10px 16px; }

/* fade */
.fade-enter-active, .fade-leave-active { transition: opacity 0.25s ease; }
.fade-enter-from, .fade-leave-to { opacity: 0; }

/* page transition - fast opacity fade */
.page-enter-active, .page-leave-active { transition: opacity 0.12s ease; }
.page-enter-from { opacity: 0; }
.page-leave-to { opacity: 0; }

/* expand */
.det-ld-body { border-top: 1px solid var(--border-default); overflow: hidden; }
.expand-enter-active, .expand-leave-active { transition: max-height 0.3s ease, opacity 0.25s ease; max-height: 1000px; }
.expand-enter-from, .expand-leave-to { max-height: 0; opacity: 0; }

/* loader cards */
.ld-enter-active, .ld-leave-active { transition: all 0.25s ease; }
.ld-enter-from { opacity: 0; transform: translateY(-8px); }
.ld-leave-to { opacity: 0; transform: translateY(-8px); }
.ld-move { transition: transform 0.25s ease; }
</style>
