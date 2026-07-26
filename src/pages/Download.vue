<script setup lang="ts">
import { ref, computed, onMounted } from "vue";
import { invoke } from "@tauri-apps/api/core";
import DownloadTab from "../component/DownloadTab.vue";
import VersionCard from "../component/VersionCard.vue";
import VersionItem from "../component/VersionItem.vue";
import grassIcon from "../assets/grass.png";
import commandIcon from "../assets/command.png";
import cobbleIcon from "../assets/cobble.png";
import emeraldIcon from "../assets/emerald.png";

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

interface VersionManifest {
  latest: LatestVersion;
  versions: VersionMeta[];
}

type Tab = "vanilla" | "mod" | "modpack" | "datapack" | "resourcepack" | "shader";

const currentTab = ref<Tab>("vanilla");
const manifest = ref<VersionManifest | null>(null);
const loading = ref(true);
const error = ref("");
const expanded = ref<Record<string, boolean>>({
  release: false, snapshot: false, old: false, aprilfools: false,
});

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

async function refreshData(tab: string) {
  if (tab === "vanilla") {
    expanded.value = { release: false, snapshot: false, old: false, aprilfools: false };
    loading.value = true;
    error.value = "";
    try { manifest.value = await invoke<VersionManifest>("get_version_list"); }
    catch (e) { error.value = String(e); }
    finally { loading.value = false; }
  }
}

onMounted(async () => {
  try { manifest.value = await invoke<VersionManifest>("get_version_list"); }
  catch (e) { error.value = String(e); }
  finally { loading.value = false; }
});
</script>

<template>
  <div class="dl">
    <nav class="dl-nav">
      <DownloadTab :active="currentTab === 'vanilla'" @click="currentTab = 'vanilla'" @refresh="refreshData('vanilla')">
        <template #icon>
          <svg viewBox="0 0 16 16" width="16" height="16" fill="currentColor">
            <path d="M2 2h5v5H2V2zm7 0h5v5H9V2zM2 9h5v5H2V9zm7 0h5v5H9V9z" />
          </svg>
        </template>
        原版游戏
      </DownloadTab>

      <div class="nav-divider"><span>社区资源</span></div>

      <DownloadTab :active="currentTab === 'mod'" @click="currentTab = 'mod'" @refresh="refreshData('mod')">
        <template #icon>
          <svg viewBox="0 0 16 16" width="16" height="16" fill="currentColor">
            <path d="M3 2a1 1 0 00-1 1v2a1 1 0 001 1h2a1 1 0 001-1V3a1 1 0 00-1-1H3zm8 0a1 1 0 00-1 1v2a1 1 0 001 1h2a1 1 0 001-1V3a1 1 0 00-1-1h-2zM3 10a1 1 0 00-1 1v2a1 1 0 001 1h2a1 1 0 001-1v-2a1 1 0 00-1-1H3zm8 0a1 1 0 00-1 1v2a1 1 0 001 1h2a1 1 0 001-1v-2a1 1 0 00-1-1h-2z" />
          </svg>
        </template>
        Mod
      </DownloadTab>

      <DownloadTab :active="currentTab === 'modpack'" @click="currentTab = 'modpack'" @refresh="refreshData('modpack')">
        <template #icon>
          <svg viewBox="0 0 16 16" width="16" height="16" fill="currentColor">
            <path d="M2 3a1 1 0 011-1h4a1 1 0 011 1v4a1 1 0 01-1 1H3a1 1 0 01-1-1V3zm6 0a1 1 0 011-1h4a1 1 0 011 1v4a1 1 0 01-1 1h-4a1 1 0 01-1-1V3zM2 9a1 1 0 011-1h4a1 1 0 011 1v4a1 1 0 01-1 1H3a1 1 0 01-1-1V9zm6 0a1 1 0 011-1h4a1 1 0 011 1v4a1 1 0 01-1 1h-4a1 1 0 01-1-1V9z" />
          </svg>
        </template>
        整合包
      </DownloadTab>

      <DownloadTab :active="currentTab === 'datapack'" @click="currentTab = 'datapack'" @refresh="refreshData('datapack')">
        <template #icon>
          <svg viewBox="0 0 16 16" width="16" height="16" fill="currentColor">
            <path d="M4 2h8a1 1 0 011 1v2a1 1 0 01-1 1H4a1 1 0 01-1-1V3a1 1 0 011-1zm0 4h8a1 1 0 011 1v6a1 1 0 01-1 1H4a1 1 0 01-1-1V7a1 1 0 011-1zm1 2v4h6V8H5z" />
          </svg>
        </template>
        数据包
      </DownloadTab>

      <DownloadTab :active="currentTab === 'resourcepack'" @click="currentTab = 'resourcepack'" @refresh="refreshData('resourcepack')">
        <template #icon>
          <svg viewBox="0 0 16 16" width="16" height="16" fill="currentColor">
            <circle cx="8" cy="8" r="6" fill="none" stroke="currentColor" stroke-width="1.2" />
            <circle cx="8" cy="8" r="2" />
          </svg>
        </template>
        资源包
      </DownloadTab>

      <DownloadTab :active="currentTab === 'shader'" @click="currentTab = 'shader'" @refresh="refreshData('shader')">
        <template #icon>
          <svg viewBox="0 0 16 16" width="16" height="16" fill="currentColor">
            <path d="M8 1a1 1 0 011 1v.5a1 1 0 01-2 0V2a1 1 0 011-1zM4.5 4.5a1 1 0 011.4 0l.4.3a1 1 0 01-1.4 1.4l-.4-.3a1 1 0 010-1.4zm7 0a1 1 0 010 1.4l-.4.3a1 1 0 11-1.4-1.4l.4-.3a1 1 0 011.4 0zM2 7a1 1 0 011 1v.5a1 1 0 01-2 0V8a1 1 0 011-1zm12 0a1 1 0 011 1v.5a1 1 0 01-2 0V8a1 1 0 011-1zm-6 2a1 1 0 011 1v.5a1 1 0 01-2 0V10a1 1 0 011-1z" />
          </svg>
        </template>
        光影包
      </DownloadTab>
    </nav>

    <div class="dl-body">
      <div v-if="loading" class="loading-wrap">
        <div class="loading-card">
          <div class="spinner"></div>
          <p>正在获取版本列表</p>
        </div>
      </div>
      <div v-else-if="currentTab === 'vanilla'" class="dl-page">
        <div v-if="error" class="state err">{{ error }}</div>
        <template v-else-if="manifest">
          <div class="card">
            <div class="card-title">最新版本</div>
            <div class="card-body">
              <VersionItem
                :icon-src="grassIcon"
                :version-id="manifest.latest.release"
                :subtitle="'最新正式版，发布于 ' + (latestRelease ? fmtDateTime(latestRelease.releaseTime) : '')"
              />
              <VersionItem
                :icon-src="commandIcon"
                :version-id="manifest.latest.snapshot"
                :subtitle="'最新预览版，发布于 ' + (latestSnapshot ? fmtDateTime(latestSnapshot.releaseTime) : '')"
              />
            </div>
          </div>

          <VersionCard title="正式版" :count="releases.length" :expanded="expanded.release" @toggle="toggle('release')">
            <VersionItem v-for="v in releases" :key="v.id" :icon-src="grassIcon" :version-id="v.id" :subtitle="fmtDateTime(v.releaseTime)" />
          </VersionCard>

          <VersionCard title="预览版" :count="snapshots.length" :expanded="expanded.snapshot" @toggle="toggle('snapshot')">
            <VersionItem v-for="v in snapshots" :key="v.id" :icon-src="commandIcon" :version-id="v.id" :subtitle="fmtDateTime(v.releaseTime)" />
          </VersionCard>

          <VersionCard title="远古版" :count="oldVersions.length" :expanded="expanded.old" @toggle="toggle('old')">
            <VersionItem v-for="v in oldVersions" :key="v.id" :icon-src="cobbleIcon" :version-id="v.id" :subtitle="fmtDateTime(v.releaseTime)" :badge="v.type" :badge-class="v.type" />
          </VersionCard>

          <VersionCard title="愚人节版" :count="aprilFools.length" :expanded="expanded.aprilfools" @toggle="toggle('aprilfools')">
            <VersionItem v-for="v in aprilFools" :key="v.id" :icon-src="emeraldIcon" :version-id="v.id" :subtitle="fmtDateTime(v.releaseTime)" />
          </VersionCard>
        </template>
      </div>
      <div v-else class="dl-page">
        <h2>{{ currentTab }}</h2>
      </div>
    </div>
  </div>
</template>

<style scoped>
.dl { display: flex; height: 100%; }

.dl-nav {
  flex: 0 0 180px;
  min-width: 160px;
  background-color: var(--surface-elevated);
  border-right: 1px solid var(--border-default);
  display: flex;
  flex-direction: column;
  padding: 16px 12px;
  gap: 2px;
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
  flex: 1; position: relative;
  padding: 24px; overflow-y: auto; scrollbar-gutter: stable;
}

.dl-page { display: flex; flex-direction: column; gap: 16px; }

.state.err { color: #e74c3c; font-size: 14px; text-align: center; padding: 40px; }

.loading-wrap {
  position: absolute;
  inset: 0;
  display: flex;
  align-items: center;
  justify-content: center;
}

.loading-card {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 16px;
  padding: 40px 56px;
  background-color: var(--surface-card);
  border-radius: var(--radius-lg);
  box-shadow: var(--shadow-md);
}

.loading-card p { font-size: 14px; color: var(--text-secondary); }

.spinner {
  width: 36px; height: 36px;
  border: 3px solid var(--border-default);
  border-top-color: var(--color-brand);
  border-radius: 50%;
  animation: spin 0.7s linear infinite;
}

@keyframes spin { to { transform: rotate(360deg); } }

/* latest card (non-expandable) */
.card {
  background-color: var(--surface-card);
  border: 1px solid var(--border-default);
  border-radius: var(--radius-md);
}

.card-title {
  display: flex; align-items: center; gap: 8px;
  padding: 12px 16px;
  font-size: 14px; font-weight: 600;
  color: var(--text-primary);
  user-select: none;
  outline: none;
}


</style>
