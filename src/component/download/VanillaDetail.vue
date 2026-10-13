<script setup lang="ts">
import { ArrowLeft, X, ChevronDown } from "@lucide/vue";
import {
  vdState, displayLoaders, loaderIcon, vanillaIcon,
} from "../../store/task/versionDownload";
import type { VersionMeta } from "../../store/task/versionDownload";
import commandIcon from "../../assets/command.png";
import cobbleIcon from "../../assets/cobble.png";

function versionIcon(v: { type: string }): string {
  return v.type === "release" ? vanillaIcon : v.type === "snapshot" ? commandIcon : cobbleIcon;
}

function startDownload() {
  vdState.startDownload();
}

// Transition 离场期组件可能多存活一帧且 selectedVersion 已为 null，用空对象兜底避免模板报错。
const EMPTY_VERSION: VersionMeta = {
  id: "", type: "", url: "", time: "", releaseTime: "", sha1: "", complianceLevel: 0,
};
const sv = () => vdState.selectedVersion ?? EMPTY_VERSION;
</script>

<template>
  <div class="det-card">
    <div class="det-hdr">
      <button class="det-back" @click="vdState.backToList" v-tooltip="'返回'">
        <ArrowLeft :size="16" />
      </button>
      <img :src="vdState.selectedMainLoader && loaderIcon[vdState.selectedMainLoader] ? loaderIcon[vdState.selectedMainLoader] : versionIcon(sv())" width="28" height="28" class="det-icon" alt="" />
      <input class="det-input" v-model="vdState.versionInput" autocomplete="off" />
      <span v-if="vdState.resolvedDirName" class="det-dir">目录: {{ vdState.resolvedDirName }}</span>
    </div>
  </div>

  <TransitionGroup name="ld">
    <template v-for="ld in displayLoaders" :key="ld.key">
      <div v-if="ld.key !== 'fabric-api'" v-show="vdState.showLoader(ld.key)" class="det-card det-ld">
        <button class="det-ld-hdr" :class="{ clickable: vdState.getLoaderVersions(ld.key, sv().id).length }"
          @click="vdState.getLoaderVersions(ld.key, sv().id).length && vdState.toggleLoader(ld.key)">
          <img v-if="loaderIcon[ld.key]" :src="loaderIcon[ld.key]" width="32" height="32" class="det-ld-icon" alt="" />
          <span class="det-ld-info">
            <span class="det-ld-title">{{ ld.name }}</span>
            <span class="det-ld-sub">
              <template v-if="vdState.selectedLoaderVersions[ld.key]">{{ ld.name }} {{ vdState.selectedLoaderVersions[ld.key] }}</template>
              <template v-else-if="vdState.loaderLoading[ld.key]">正在加载...</template>
              <template v-else-if="vdState.getLoaderVersions(ld.key, sv().id).length">可以添加</template>
              <template v-else>无</template>
              <!-- Fabric/Quilt loader 版本无关（meta 不按 MC 版本过滤）：任何版本兼容性均未验证，需手动确认 -->
              <span v-if="ld.key === 'fabric' || ld.key === 'quilt'" class="det-ld-warn" v-tooltip="'该加载器为版本无关构建，meta 不按 MC 版本过滤；最新版也可能与当前 MC 不兼容，安装前请确认'">兼容性需确认</span>
            </span>
          </span>
          <span v-if="vdState.selectedLoaderVersions[ld.key] || vdState.getLoaderVersions(ld.key, sv().id).length" class="det-ld-actions">
            <button v-if="vdState.selectedLoaderVersions[ld.key]" class="det-ld-remove" @click.stop="vdState.clearLoaderVer(ld.key)" v-tooltip="'移除'">
              <X :size="10" />
            </button>
            <ChevronDown v-if="vdState.getLoaderVersions(ld.key, sv().id).length" class="det-ld-arrow" :class="{ open: vdState.expandedLoader[ld.key] }" :size="12" />
          </span>
        </button>
        <Transition name="expand">
          <div v-if="vdState.expandedLoader[ld.key] && vdState.getLoaderVersions(ld.key, sv().id).length" class="det-ld-body">
            <div v-for="ver in vdState.getLoaderVersions(ld.key, sv().id)" :key="ver"
                class="det-ld-ver"
                @click="vdState.selectLoaderVer(ld.key, ver)">
              <img v-if="loaderIcon[ld.key]" :src="loaderIcon[ld.key]" width="24" height="24" class="det-ld-ver-icon" alt="" />
              <span class="det-ld-ver-text">{{ ver }}</span>
            </div>
          </div>
        </Transition>
      </div>
      <div v-else class="det-card det-ld">
        <button class="det-ld-hdr" :class="{ clickable: vdState.getFabricApiVersions(sv().id).length }"
          @click="vdState.getFabricApiVersions(sv().id).length && vdState.toggleFabricApi()">
          <img :src="loaderIcon.fabric" width="32" height="32" class="det-ld-icon" alt="" />
          <span class="det-ld-info">
            <span class="det-ld-title">Fabric API</span>
            <span class="det-ld-sub">
              <template v-if="vdState.selectedLoaderVersions['fabric-api']">Fabric API {{ vdState.selectedLoaderVersions['fabric-api'] }}</template>
              <template v-else-if="vdState.fabricApiLoading">正在加载...</template>
              <template v-else-if="vdState.getFabricApiVersions(sv().id).length">可以添加</template>
              <template v-else>无</template>
            </span>
          </span>
          <span v-if="vdState.selectedLoaderVersions['fabric-api'] || vdState.getFabricApiVersions(sv().id).length" class="det-ld-actions">
            <button v-if="vdState.selectedLoaderVersions['fabric-api']" class="det-ld-remove" @click.stop="vdState.clearFabricApi()" v-tooltip="'移除'">
              <X :size="10" />
            </button>
            <ChevronDown v-if="vdState.getFabricApiVersions(sv().id).length" class="det-ld-arrow" :class="{ open: vdState.fabricApiExpanded }" :size="12" />
          </span>
        </button>
        <Transition name="expand">
          <div v-if="vdState.fabricApiExpanded && vdState.getFabricApiVersions(sv().id).length" class="det-ld-body">
            <div v-for="ver in vdState.getFabricApiVersions(sv().id)" :key="ver.version"
                class="det-ld-ver"
                @click="vdState.selectFabricApi(ver)">
              <img :src="loaderIcon.fabric" width="24" height="24" class="det-ld-ver-icon" alt="" />
              <span class="det-ld-ver-text">{{ ver.version }}</span>
            </div>
          </div>
        </Transition>
      </div>
    </template>
  </TransitionGroup>

  <div class="dl-spacer"></div>
  <div class="dl-footer">
    <button v-if="!vdState.dlSubmitting" class="dl-btn" :disabled="vdState.fabricApiUnready()" @click="startDownload">
      开始下载
    </button>
    <p v-else class="dl-done">已加入下载队列</p>
  </div>
</template>

<style scoped>
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
.det-ld-sub { font-size: 11px; color: var(--text-tertiary); display: flex; align-items: center; gap: 6px; }
.det-ld-warn {
  font-size: 10px;
  color: var(--color-warning);
  font-weight: 500;
  flex-shrink: 0;
}
.det-ld-actions { display: flex; align-items: center; gap: 2px; margin-left: auto; }
.det-ld-remove {
  width: 24px; height: 24px;
  display: flex; align-items: center; justify-content: center;
  border: none; background: transparent;
  color: var(--text-tertiary);
  cursor: pointer; border-radius: var(--radius-sm);
  transition: all 0.15s; flex-shrink: 0;
}
.det-ld-remove:hover { color: var(--color-danger); background-color: var(--color-danger-bg); }
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

.dl-spacer { flex: 1; min-height: 12px; }
.dl-footer {
  display: flex; flex-direction: column; align-items: center; gap: 8px;
  position: sticky; bottom: 0; z-index: 5; flex-shrink: 0;
}
.dl-btn {
  padding: 10px 40px;
  border: none; border-radius: var(--radius-lg);
  background-color: var(--color-brand);
  color: var(--text-inverse); font-size: 14px; font-weight: 600;
  cursor: pointer; transition: all 0.15s;
  align-self: center; display: inline-flex; align-items: center; gap: 8px;
}
.dl-btn:disabled { opacity: 0.6; cursor: not-allowed; }
.dl-btn:hover:not(:disabled) { background-color: var(--color-brand-hover); box-shadow: var(--shadow-md); }
.dl-btn:active:not(:disabled) { transform: scale(0.97); }
.dl-done { font-size: 14px; font-weight: 600; color: var(--color-success); text-align: center; padding: 10px 40px; }

/* expand 目标高度由共享骨架（styles/transitions.css）提供；须与 max-height 320px 一致，超高时内部滚动 */
.det-ld-body { border-top: 1px solid var(--border-default); max-height: 320px; overflow-y: auto; --expand-h: 320px; }

/* loader cards */
.ld-enter-active, .ld-leave-active { transition: all 0.25s ease; }
.ld-enter-from { opacity: 0; transform: translateY(-8px); }
.ld-leave-to { opacity: 0; transform: translateY(-8px); }
.ld-move { transition: transform 0.25s ease; }
</style>
