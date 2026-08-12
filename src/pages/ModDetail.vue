<script setup lang="ts">
// 项目详情页（Mod / 光影包 / 资源包 / 数据包 / 整合包共用）：
// 信息卡片 + 版本文件列表 + 下载（modpack 无落盘目录映射，仅展示版本并提供来源页跳转）。
import { ref, computed, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import { showToast } from "../utils/toast";
import { modDetail } from "../store/modDetail";
import { dlTranslateMode } from "../store/downloaderConfig";
import { dlState } from "../store/download";
import { fetchVersions, type ModVersion } from "../store/modSearch";
import { selectedVersion } from "../store/selectedVersion";
import { fmtCount, fmtSize, fmtModDate } from "../utils/format";
import IconImage from "../component/IconImage.vue";

const item = computed(() => modDetail.item);

const sourceLabel = computed(() =>
  item.value?.source === "modrinth" ? "Modrinth" : "CurseForge",
);

// ── 版本列表 ──────────────────────────────

const versions = ref<ModVersion[]>([]);
const loadingVersions = ref(false);
const versionError = ref("");

async function loadVersions() {
  const it = item.value;
  if (!it) return;
  loadingVersions.value = true;
  versionError.value = "";
  try {
    versions.value = await fetchVersions(it.source, it.id);
  } catch (e) {
    versionError.value = String(e);
  } finally {
    loadingVersions.value = false;
  }
}

watch(item, loadVersions, { immediate: true });

// ── 操作 ──────────────────────────────

/** 来源页面链接（item.page_url）。 */
function gotoSource() {
  const it = item.value;
  if (!it?.page_url) return;
  openUrl(it.page_url).catch(() => showToast("打开链接失败"));
}

/** MC百科：有内置条目 id → 直达条目页；未收录 → 名称搜索页降级。 */
function gotoMcmod() {
  const it = item.value;
  if (!it) return;
  const url = it.mcmod_id
    ? `https://www.mcmod.cn/class/${it.mcmod_id}.html`
    : `https://search.mcmod.cn/s?key=${encodeURIComponent(it.title_zh ?? it.title)}&mold=0`;
  openUrl(url).catch(() => showToast("打开链接失败"));
}

/** 复制名称：有译名时复制 "翻译名 | 原名"（与标题显示样式一致），否则原名。 */
async function copyName() {
  const it = item.value;
  if (!it) return;
  const text = it.title_zh ? `${it.title_zh} | ${it.title}` : it.title;
  try {
    await navigator.clipboard.writeText(text);
    showToast("已复制名称");
  } catch {
    showToast("复制失败");
  }
}

/** 来源 tab → 下载落盘目录 kind（整合包无目录映射，返回 null）。 */
function kindFromTab(): "mod" | "shader" | "resourcepack" | "datapack" | null {
  const map: Record<string, "mod" | "shader" | "resourcepack" | "datapack" | null> = {
    mod: "mod",
    shader: "shader",
    resourcepack: "resourcepack",
    datapack: "datapack",
    modpack: null,
    vanilla: null,
  };
  return map[modDetail.backTab] ?? null;
}

/** kind → 实际落盘目录（镜像后端 DownloadFileKind::kind_dir）。 */
const kindDirLabel: Record<string, string> = {
  mod: "mods",
  shader: "shaderpacks",
  resourcepack: "resourcepacks",
  datapack: "datapacks",
};

/** 提交单文件下载到下载管理；目标版本目录 = 启动页当前选择的版本。 */
async function downloadVersion(v: ModVersion) {
  if (!item.value) return;
  const kind = kindFromTab();
  if (!kind) return;
  if (!selectedVersion.value) {
    showToast("请先在启动页选择要安装的游戏版本");
    return;
  }
  try {
    const taskId = await invoke<string>("download_single_file", {
      task: {
        version_dir_name: selectedVersion.value,
        kind,
        url: v.download_url,
        filename: v.filename,
      },
    });
    dlState.addTask({
      taskId,
      name: v.filename,
      status: "pending",
      progress: 0,
      speedBps: 0,
      completedFiles: 0,
      totalFiles: 0,
      completedBytes: 0,
      totalBytes: 0,
      currentPhase: "等待中",
      steps: [],
      previousRoute: "/download",
    });
    // 竞态校正：极快终态任务（error/done）可能已被移除，清理残留卡片
    const exists = await invoke<boolean>("task_exists", { taskId });
    if (!exists) dlState.removeTask(taskId);
    showToast("已加入下载任务");
  } catch (e) {
    showToast(`下载提交失败：${e}`);
  }
}
</script>

<template>
  <div v-if="item" class="md">
    <!-- 卡片一：项目信息（样式与外部 item 类似；描述行与图标左对齐全宽；按钮在卡片内底部） -->
    <div class="md-card">
      <div class="md-row">
        <IconImage :src="item.icon_url" :size="40" />
        <span class="md-info">
          <span class="md-title-row">
            <span class="md-title">{{ item.title_zh ?? item.title }}</span>
            <span v-if="dlTranslateMode === 'zh' && item.title_zh" class="md-title-en"> | {{ item.title }}</span>
            <span v-for="ld in item.loaders" :key="ld" class="md-ld">{{ ld }}</span>
            <span class="md-badge" :class="item.source">{{ sourceLabel }}</span>
            <span class="md-sub">{{ fmtCount(item.downloads) }} 次下载</span>
          </span>
        </span>
      </div>
      <div class="md-desc">{{ item.translated ?? item.description }}</div>

      <!-- 操作按钮（卡片内底部） -->
      <div class="md-actions">
        <button class="md-btn" @click="gotoSource">转到{{ sourceLabel }}</button>
        <button class="md-btn" @click="gotoMcmod">转到 MC百科</button>
        <button class="md-btn" @click="copyName">复制名称</button>
      </div>
    </div>

    <!-- 卡片二：版本文件列表 -->
    <div class="md-card md-versions">
      <div class="mv-head">
        <span class="mv-title">版本文件</span>
        <span class="mv-hint">
          {{ kindFromTab() ? `下载到已选版本 ${selectedVersion || "(未选择)"} 的 ${kindDirLabel[kindFromTab()!]} 目录` : "整合包请前往来源页下载" }}
        </span>
      </div>

      <div v-if="loadingVersions" class="mv-state">
        <div class="spinner"></div>
        <p class="mv-state-text">正在加载版本列表...</p>
      </div>
      <div v-else-if="versionError" class="mv-state err">
        <p>{{ versionError }}</p>
        <button class="md-btn mv-retry" @click="loadVersions">重试</button>
      </div>
      <div v-else-if="versions.length === 0" class="mv-state">暂无可用版本</div>
      <div v-else class="mv-list">
        <div v-for="v in versions" :key="v.version_id" class="mv-item">
          <div class="mv-info">
            <span class="mv-name" :title="v.filename">{{ v.filename }}</span>
            <span class="mv-tags">
              <span v-for="gv in v.game_versions.slice(0, 3)" :key="gv" class="mv-tag gv">{{ gv }}</span>
              <span v-for="ld in v.loaders" :key="ld" class="mv-tag ld">{{ ld }}</span>
            </span>
          </div>
          <span class="mv-meta">
            <span class="mv-date">{{ fmtModDate(v.date_published) }}</span>
            <span class="mv-size">{{ fmtSize(v.file_size) }}</span>
          </span>
          <button v-if="kindFromTab()" class="mv-btn" @click="downloadVersion(v)">下载</button>
        </div>
      </div>
    </div>
  </div>
  <div v-else class="md-empty">未找到资源信息</div>
</template>

<style scoped>
.md {
  height: 100%;
  display: flex;
  flex-direction: column;
  gap: 12px;
  padding: 20px 24px;
  overflow-y: auto;
  overflow-x: hidden;
  background: var(--surface-page);
  scrollbar-gutter: stable;
}

.md-card {
  background: var(--surface-card);
  border: 1px solid var(--border-default);
  border-radius: var(--radius-md);
  padding: 14px 16px;
}

.md-row {
  display: flex;
  align-items: center;
  gap: 12px;
}

.md-info {
  flex: 1;
  min-width: 0;
}

.md-title-row {
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
}

.md-title {
  font-weight: 600;
  font-size: 14px;
  color: var(--text-primary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

/* 原名：与译名样式不一致（弱化、更小、常规字重） */
.md-title-en {
  font-size: 11px;
  font-weight: 400;
  color: var(--text-tertiary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

/* 支持的 ModLoader 徽章（与外部 item 同款） */
.md-ld {
  flex-shrink: 0;
  font-size: 10px;
  font-weight: 600;
  padding: 0 5px;
  border-radius: 6px;
  background: var(--color-brand-bg);
  color: var(--color-brand);
}

.md-badge {
  flex-shrink: 0;
  font-size: 10px;
  padding: 1px 6px;
  border-radius: 8px;
  color: #fff;
  font-weight: 600;
}
.md-badge.modrinth { background: #1bd96a; }
.md-badge.curseforge { background: #f16436; }

.md-sub {
  flex-shrink: 0;
  margin-left: auto;
  font-size: 11px;
  color: var(--text-tertiary);
  white-space: nowrap;
}

/* 描述：另起一行，左边缘与图标左对齐（全宽） */
.md-desc {
  margin-top: 12px;
  padding-top: 12px;
  border-top: 1px solid var(--border-subtle, var(--border-default));
  font-size: 12px;
  color: var(--text-secondary);
  line-height: 1.6;
}

/* 操作按钮：卡片内底部（描述下方，分隔线上） */
.md-actions {
  display: flex;
  gap: 8px;
  margin-top: 12px;
  padding-top: 12px;
  border-top: 1px solid var(--border-subtle, var(--border-default));
}

.md-btn {
  flex: 1;
  padding: 8px 0;
  border: 1px solid var(--border-strong);
  border-radius: var(--radius-md);
  background: var(--surface-card);
  color: var(--text-secondary);
  font-size: 12.5px;
  font-weight: 500;
  cursor: pointer;
  transition: all 0.15s;
}

.md-btn:hover {
  border-color: var(--color-brand);
  color: var(--color-brand);
  background: var(--color-brand-bg);
}

/* ── 版本列表 ── */

.md-versions {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
  padding: 12px 16px;
}

.mv-head {
  display: flex;
  align-items: baseline;
  gap: 10px;
  padding-bottom: 8px;
  border-bottom: 1px solid var(--border-subtle, var(--border-default));
}

.mv-title {
  font-size: 13px;
  font-weight: 600;
  color: var(--text-primary);
}

.mv-hint {
  font-size: 11px;
  color: var(--text-tertiary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.mv-state {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 10px;
  padding: 30px 0;
  font-size: 12.5px;
  color: var(--text-tertiary);
}
.mv-state.err { color: #e74c3c; }
.mv-state-text { font-size: 12.5px; color: var(--text-tertiary); }
.mv-retry { flex: 0 0 auto; padding: 5px 20px; font-size: 12px; }

.spinner {
  width: 32px; height: 32px;
  border: 3px solid var(--border-default); border-top-color: var(--color-brand);
  border-radius: 50%; animation: spin 0.7s linear infinite;
}
@keyframes spin { to { transform: rotate(360deg); } }

.mv-list {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  margin: 8px -8px -4px;
}

.mv-item {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 8px 10px;
  border-radius: var(--radius-sm);
  transition: background 0.1s;
}
.mv-item:hover { background-color: var(--surface-hover); }

.mv-info {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 3px;
}

.mv-name {
  font-size: 12px;
  font-weight: 500;
  color: var(--text-primary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.mv-tags {
  display: flex;
  align-items: center;
  gap: 4px;
  min-width: 0;
  flex-wrap: wrap;
}

.mv-tag {
  flex-shrink: 0;
  font-size: 10px;
  font-weight: 600;
  padding: 0 5px;
  border-radius: 6px;
}
.mv-tag.gv { background: var(--surface-hover); color: var(--text-secondary); }
.mv-tag.ld { background: var(--color-brand-bg); color: var(--color-brand); }

.mv-meta {
  flex-shrink: 0;
  display: flex;
  flex-direction: column;
  align-items: flex-end;
  gap: 2px;
}
.mv-date { font-size: 10.5px; color: var(--text-tertiary); }
.mv-size { font-size: 11px; color: var(--text-secondary); }

.mv-btn {
  flex-shrink: 0;
  padding: 5px 14px;
  border: 1px solid var(--border-strong);
  border-radius: var(--radius-md);
  background: var(--surface-card);
  color: var(--text-secondary);
  font-size: 12px;
  font-weight: 500;
  cursor: pointer;
  transition: all 0.15s;
}
.mv-btn:hover {
  border-color: var(--color-brand);
  color: var(--color-brand);
  background: var(--color-brand-bg);
}

.md-empty {
  height: 100%;
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 13px;
  color: var(--text-tertiary);
  background: var(--surface-page);
}
</style>
