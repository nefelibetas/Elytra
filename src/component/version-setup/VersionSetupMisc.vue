<script setup lang="ts">
// 版本设置-其他 tab：版本图标管理（自定义图标 / 还原为默认图标）。
// 图标优先级：用户自定义（version-icon.png）> 整合包项目图（pack-icon.png）> loader 图标。
// "还原为默认"仅删除自定义图——整合包 logo 是安装时的样子，不属于自定义。
import { computed, onMounted } from "vue";
import { useRoute } from "vue-router";
import { ImagePlus, Undo2 } from "@lucide/vue";
import {
  versionIcons, iconSources,
  loadVersionIcon, loadIconSource, setVersionIcon, resetVersionIcon,
  type VersionIconSource,
} from "../../store/versionIcon";
import { open } from "@tauri-apps/plugin-dialog";
import { showToast } from "../../utils/toast";
import { errMsg } from "../../utils/error";

const route = useRoute();
/** 游戏 id（版本目录名），由路由 query 传入。 */
const versionName = computed(() => String(route.query.name ?? ""));

const currentIcon = computed(() => versionIcons[versionName.value]);
const source = computed<VersionIconSource>(() => iconSources[versionName.value] ?? "default");

const SOURCE_LABELS: Record<VersionIconSource, string> = {
  custom: "当前使用：自定义图标",
  pack: "当前使用：整合包项目图标",
  default: "当前使用：默认加载器图标",
};

onMounted(() => {
  void loadVersionIcon(versionName.value);
  void loadIconSource(versionName.value);
});

/** 自定义图标：本地选图 → 后端魔数校验 + 复制为版本图标，成功后缓存自动刷新。 */
async function pickCustomIcon() {
  const path = await open({
    multiple: false,
    filters: [{ name: "图片", extensions: ["png", "jpg", "jpeg", "webp", "gif"] }],
  });
  if (!path) return;
  try {
    await setVersionIcon(versionName.value, String(path));
    showToast("图标已更新");
  } catch (e) {
    showToast(errMsg(e));
  }
}

/** 还原为默认：删除自定义图；无自定义时按钮禁用（幂等保护）。 */
async function restoreDefault() {
  try {
    await resetVersionIcon(versionName.value);
    showToast("已还原为默认图标");
  } catch (e) {
    showToast(errMsg(e));
  }
}
</script>

<template>
  <div class="vs-page">
    <!-- 当前图标预览 -->
    <div class="setup-card">
      <div class="setup-card-title">版本图标</div>
      <div class="vs-icon-row">
        <img v-if="currentIcon" :src="currentIcon" class="vs-icon-preview" alt="版本图标" />
        <div v-else class="vs-icon-preview vs-icon-empty">无</div>
        <span class="setup-desc">{{ SOURCE_LABELS[source] }}</span>
      </div>
    </div>

    <!-- 自定义图标 -->
    <div class="setup-card">
      <div class="setup-card-title">自定义图标</div>
      <div class="setup-desc">
        为该版本选择一张本地图片作为显示图标（支持 png / jpg / webp / gif，1MB 以内），
        版本列表与主页卡片将展示它。
      </div>
      <div class="setup-btn-row">
        <button class="setup-btn" @click="pickCustomIcon">
          <ImagePlus :size="15" />
          <span>选择图片</span>
        </button>
      </div>
    </div>

    <!-- 还原为默认图标 -->
    <div class="setup-card">
      <div class="setup-card-title">还原为默认图标</div>
      <div class="setup-desc">
        移除自定义图标。整合包安装自带的封面图不受影响；
        两者都没有时显示默认加载器图标。
      </div>
      <div class="setup-btn-row">
        <button
          class="setup-btn"
          :disabled="source !== 'custom'"
          :class="{ 'setup-btn-disabled': source !== 'custom' }"
          @click="restoreDefault"
        >
          <Undo2 :size="15" />
          <span>还原为默认图标</span>
        </button>
      </div>
    </div>
  </div>
</template>

<style scoped>
/* 其他页：占满右侧剩余空间，内容超出自身滚动 */
.vs-page {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  overflow-x: hidden;
}

.setup-card {
  background: var(--surface-card);
  border: 1px solid var(--border-default);
  border-radius: var(--radius-md);
  max-width: 560px;
}

.setup-card + .setup-card {
  margin-top: 12px;
}

.setup-card-title {
  font-size: 12px;
  font-weight: 600;
  color: var(--text-tertiary);
  padding: 12px 16px 4px;
  letter-spacing: 0.5px;
}

.setup-desc {
  font-size: 12.5px;
  line-height: 1.7;
  color: var(--text-secondary);
  padding: 4px 16px 0;
}

.setup-btn-row {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
  padding: 10px 16px 14px;
  flex-wrap: wrap;
}

.setup-btn {
  display: flex;
  align-items: center;
  gap: 7px;
  padding: 7px 14px;
  border: 1px solid var(--border-strong);
  border-radius: var(--radius-sm);
  background: transparent;
  color: var(--text-secondary);
  font-size: 12.5px;
  cursor: pointer;
  transition: all 0.15s;
}

.setup-btn:hover:not(:disabled) {
  border-color: var(--color-brand);
  color: var(--color-brand);
}

.setup-btn:disabled {
  opacity: 0.45;
  cursor: not-allowed;
}

/* 当前图标预览行 */
.vs-icon-row {
  display: flex;
  align-items: center;
  gap: 14px;
  padding: 10px 16px 14px;
}

.vs-icon-preview {
  width: 48px;
  height: 48px;
  border-radius: var(--radius-sm);
  object-fit: contain;
  background: var(--surface-page);
  border: 1px solid var(--border-default);
  flex-shrink: 0;
}

.vs-icon-empty {
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 11px;
  color: var(--text-tertiary);
}
</style>
