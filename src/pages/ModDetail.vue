<script setup lang="ts">
// Mod 详情页：Mod 信息卡片 + 操作按钮 + 占位大卡片（后续扩展）。
import { computed } from "vue";
import { openUrl } from "@tauri-apps/plugin-opener";
import { showToast } from "../utils/toast";
import { modDetail } from "../store/modDetail";
import { dlTranslateMode } from "../store/downloaderConfig";
import { fmtCount } from "../utils/format";
import IconImage from "../component/IconImage.vue";

const item = computed(() => modDetail.item);

const sourceLabel = computed(() =>
  item.value?.source === "modrinth" ? "Modrinth" : "CurseForge",
);

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
</script>

<template>
  <div v-if="item" class="md">
    <!-- 卡片一：Mod 信息（样式与外部 item 类似；描述行与图标左对齐全宽；按钮在卡片内底部） -->
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

    <!-- 卡片二：占位（后续扩展） -->
    <div class="md-card md-placeholder"></div>
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

.md-placeholder {
  flex: 1;
  min-height: 120px;
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
