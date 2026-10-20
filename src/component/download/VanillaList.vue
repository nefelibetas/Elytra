<script setup lang="ts">
import { useI18n } from "vue-i18n";
import VersionCard from "../download/VersionCard.vue";
import VersionItem from "../download/VersionItem.vue";
import {
  vdState, latestRelease, latestSnapshot, releases, snapshots, oldVersions, aprilFools, vanillaIcon,
} from "../../store/task/versionDownload";
import { fmtDateTime } from "../../utils/format";
import LoadingSpinner from "../common/LoadingSpinner.vue";
import ErrorState from "../common/ErrorState.vue";
import commandIcon from "../../assets/command.png";
import emeraldIcon from "../../assets/emerald.png";
import cobbleIcon from "../../assets/cobble.png";

const { t } = useI18n();
</script>

<template>
  <LoadingSpinner v-if="vdState.loading" :text="t('download.vanilla.loadingList')" />
  <ErrorState v-else-if="vdState.error" :text="vdState.error" />
  <template v-else-if="vdState.manifest">
    <div class="card">
      <div class="card-title">{{ t("download.vanilla.latestTitle") }}</div>
      <div class="card-body">
        <VersionItem :icon-src="vanillaIcon" :version-id="vdState.manifest.latest.release" :subtitle="t('download.vanilla.latestRelease', { date: latestRelease ? fmtDateTime(latestRelease.releaseTime) : '' })" @select="latestRelease && vdState.selectVersion(latestRelease)" />
        <VersionItem :icon-src="commandIcon" :version-id="vdState.manifest.latest.snapshot" :subtitle="t('download.vanilla.latestSnapshot', { date: latestSnapshot ? fmtDateTime(latestSnapshot.releaseTime) : '' })" @select="latestSnapshot && vdState.selectVersion(latestSnapshot)" />
      </div>
    </div>
    <VersionCard :title="t('download.vanilla.release')" :count="releases.length" :expanded="vdState.expanded.release" @toggle="vdState.toggle('release')">
      <VersionItem v-for="v in releases" :key="v.id" :icon-src="vanillaIcon" :version-id="v.id" :subtitle="fmtDateTime(v.releaseTime)" @select="vdState.selectVersion(v)" />
    </VersionCard>
    <VersionCard :title="t('download.vanilla.snapshot')" :count="snapshots.length" :expanded="vdState.expanded.snapshot" @toggle="vdState.toggle('snapshot')">
      <VersionItem v-for="v in snapshots" :key="v.id" :icon-src="commandIcon" :version-id="v.id" :subtitle="fmtDateTime(v.releaseTime)" @select="vdState.selectVersion(v)" />
    </VersionCard>
    <VersionCard :title="t('download.vanilla.old')" :count="oldVersions.length" :expanded="vdState.expanded.old" @toggle="vdState.toggle('old')">
      <VersionItem v-for="v in oldVersions" :key="v.id" :icon-src="cobbleIcon" :version-id="v.id" :subtitle="fmtDateTime(v.releaseTime)" :badge="v.type" :badge-class="v.type" @select="vdState.selectVersion(v)" />
    </VersionCard>
    <VersionCard :title="t('download.vanilla.aprilFools')" :count="aprilFools.length" :expanded="vdState.expanded.aprilfools" @toggle="vdState.toggle('aprilfools')">
      <VersionItem v-for="v in aprilFools" :key="v.id" :icon-src="emeraldIcon" :version-id="v.id" :subtitle="fmtDateTime(v.releaseTime)" @select="vdState.selectVersion(v)" />
    </VersionCard>
  </template>
</template>

<style scoped>


.card { background-color: var(--surface-card); border: 1px solid var(--border-default); border-radius: var(--radius-md); }
.card-body {
  overflow: hidden;
  /* 与 VersionCard 一致：底部圆角贴合 .card，末项 hover 背景不露直角 */
  border-radius: 0 0 var(--radius-md) var(--radius-md);
}
.card-title {
  display: flex; align-items: center; gap: 8px;
  padding: 12px 16px; font-size: 14px; font-weight: 600;
  color: var(--text-primary); user-select: none; outline: none;
}
</style>
