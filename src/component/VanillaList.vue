<script setup lang="ts">
// 原版游戏版本列表页：最新版本 + 正式版/预览版/远古版/愚人节版分组
import VersionCard from "./VersionCard.vue";
import VersionItem from "./VersionItem.vue";
import {
  vdState, latestRelease, latestSnapshot, releases, snapshots, oldVersions, aprilFools, vanillaIcon,
} from "../store/versionDownload";
import { fmtDateTime } from "../utils/format";
import commandIcon from "../assets/command.png";
import emeraldIcon from "../assets/emerald.png";
import cobbleIcon from "../assets/cobble.png";
</script>

<template>
  <div v-if="vdState.loading" class="loading-card" style="margin: auto;">
    <div class="spinner"></div><p>正在获取版本列表</p>
  </div>
  <div v-else-if="vdState.error" class="state err">{{ vdState.error }}</div>
  <template v-else-if="vdState.manifest">
    <div class="card">
      <div class="card-title">最新版本</div>
      <div class="card-body">
        <VersionItem :icon-src="vanillaIcon" :version-id="vdState.manifest.latest.release" :subtitle="'最新正式版，发布于 ' + (latestRelease ? fmtDateTime(latestRelease.releaseTime) : '')" @select="vdState.selectVersion(latestRelease!)" />
        <VersionItem :icon-src="commandIcon" :version-id="vdState.manifest.latest.snapshot" :subtitle="'最新预览版，发布于 ' + (latestSnapshot ? fmtDateTime(latestSnapshot.releaseTime) : '')" @select="vdState.selectVersion(latestSnapshot!)" />
      </div>
    </div>
    <VersionCard title="正式版" :count="releases.length" :expanded="vdState.expanded.release" @toggle="vdState.toggle('release')">
      <VersionItem v-for="v in releases" :key="v.id" :icon-src="vanillaIcon" :version-id="v.id" :subtitle="fmtDateTime(v.releaseTime)" @select="vdState.selectVersion(v)" />
    </VersionCard>
    <VersionCard title="预览版" :count="snapshots.length" :expanded="vdState.expanded.snapshot" @toggle="vdState.toggle('snapshot')">
      <VersionItem v-for="v in snapshots" :key="v.id" :icon-src="commandIcon" :version-id="v.id" :subtitle="fmtDateTime(v.releaseTime)" @select="vdState.selectVersion(v)" />
    </VersionCard>
    <VersionCard title="远古版" :count="oldVersions.length" :expanded="vdState.expanded.old" @toggle="vdState.toggle('old')">
      <VersionItem v-for="v in oldVersions" :key="v.id" :icon-src="cobbleIcon" :version-id="v.id" :subtitle="fmtDateTime(v.releaseTime)" :badge="v.type" :badge-class="v.type" @select="vdState.selectVersion(v)" />
    </VersionCard>
    <VersionCard title="愚人节版" :count="aprilFools.length" :expanded="vdState.expanded.aprilfools" @toggle="vdState.toggle('aprilfools')">
      <VersionItem v-for="v in aprilFools" :key="v.id" :icon-src="emeraldIcon" :version-id="v.id" :subtitle="fmtDateTime(v.releaseTime)" @select="vdState.selectVersion(v)" />
    </VersionCard>
  </template>
</template>

<style scoped>
.state.err { color: var(--color-danger); font-size: 14px; text-align: center; padding: 40px; }
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
.card-body {
  overflow: hidden;
  /* 与 VersionCard 一致：底部圆角贴合 .card 圆角，最后一项 hover 背景不露直角 */
  border-radius: 0 0 var(--radius-md) var(--radius-md);
}
.card-title {
  display: flex; align-items: center; gap: 8px;
  padding: 12px 16px; font-size: 14px; font-weight: 600;
  color: var(--text-primary); user-select: none; outline: none;
}
</style>
