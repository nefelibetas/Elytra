<script setup lang="ts">
// 皮肤衣柜：左 = 3D 全身预览（拖动旋转，能看到披风），右 = 衣柜（上皮肤 / 下披风）。
// 交互与状态全在 store/skinStudio（模块级单例），本页仅编排布局与缩略图样式。
import { onMounted, onUnmounted } from "vue";
import { Check, Download, RefreshCw, RotateCcw, Shirt, Upload, X } from "@lucide/vue";
import AlertDialog from "../component/common/AlertDialog.vue";
import EmptyState from "../component/common/EmptyState.vue";
import LoadingSpinner from "../component/common/LoadingSpinner.vue";
import { skinStudio } from "../store/skinStudio";

onMounted(() => void skinStudio.load());
onUnmounted(() => skinStudio.dispose());

/** 皮肤缩略图：64×64 纹理里截头部正面 (8,8,8,8) 区域放大
 *  （size 800% + position 14.286% = 8/56 → 精确对准该 UV 区）。 */
function skinThumb(b64: string) {
  return {
    backgroundImage: `url(data:image/png;base64,${b64})`,
    backgroundSize: "800% 800%",
    backgroundPosition: "14.286% 14.286%",
  };
}

/** 披风缩略图：64×32 纹理里截披风正面 (0,0,10,16) 区域。 */
function capeThumb(b64: string) {
  return {
    backgroundImage: `url(data:image/png;base64,${b64})`,
    backgroundSize: "640% 200%",
    backgroundPosition: "0 0",
  };
}
</script>

<template>
  <div class="studio">
    <!-- 左：3D 全身预览（按住拖动旋转；pitch 限 ±40°） -->
    <div class="stage-col">
      <div
        class="stage"
        :class="{ grabbing: skinStudio.dragging }"
        @pointerdown="skinStudio.beginDrag($event)"
        @pointermove="skinStudio.moveDrag($event)"
        @pointerup="skinStudio.endDrag($event)"
        @pointercancel="skinStudio.endDrag($event)"
      >
        <img
          v-if="skinStudio.frame"
          class="frame"
          :src="skinStudio.frame"
          alt="3D 皮肤预览"
          draggable="false"
        />
        <LoadingSpinner
          v-else-if="skinStudio.officialLoading || skinStudio.rendering"
          text="正在渲染…"
        />
        <EmptyState v-else>
          <p>暂无可预览的皮肤</p>
          <p>登录账号或从本地衣柜选一件</p>
        </EmptyState>

        <div class="stage-tip">按住拖动旋转 · 上下 ±40°</div>
      </div>

      <div class="stage-bar">
        <div class="stage-meta">
          <span class="meta-name">
            {{ skinStudio.selectedLocal ? skinStudio.selectedLocal.name : "账号当前皮肤" }}
          </span>
          <span class="meta-tag">{{ skinStudio.currentSlim ? "SLIM" : "CLASSIC" }}</span>
          <span v-if="skinStudio.selectedLocal" class="meta-tag local">本地</span>
        </div>
        <div class="stage-actions">
          <button class="tool" v-tooltip="'重置视角'" @click="skinStudio.resetView">
            <RotateCcw :size="14" /><span>重置视角</span>
          </button>
          <button class="tool" v-tooltip="'重新拉取皮肤与披风'" @click="skinStudio.refresh">
            <RefreshCw :size="14" /><span>刷新</span>
          </button>
          <button
            class="tool"
            :disabled="!skinStudio.premium"
            v-tooltip="'导出账号当前皮肤 PNG'"
            @click="skinStudio.saveSkinFile"
          >
            <Download :size="14" /><span>保存皮肤</span>
          </button>
          <button
            v-if="skinStudio.selectedLocal"
            class="tool primary"
            :disabled="!skinStudio.canApply"
            v-tooltip="'上传为账号当前皮肤'"
            @click="skinStudio.applySelected"
          >
            <Check :size="14" />
            <span>{{ skinStudio.applyBusy ? "应用中…" : "应用为当前皮肤" }}</span>
          </button>
        </div>
        <p v-if="!skinStudio.premium" class="hint">
          本地衣柜可用于预览；应用皮肤 / 管理披风需登录正版账号
        </p>
        <p v-if="skinStudio.officialError" class="err">{{ skinStudio.officialError }}</p>
      </div>
    </div>

    <!-- 右：衣柜（上半皮肤 / 下半披风） -->
    <div class="wardrobe">
      <section class="panel">
        <header class="panel-hd">
          <span class="panel-title"><Shirt :size="15" />皮肤</span>
          <div class="panel-hd-right">
            <span v-if="skinStudio.skins.length" class="panel-count">
              {{ skinStudio.skins.length }} 件收藏
            </span>
            <button class="tool sm" v-tooltip="'把本地 64×64 皮肤 PNG 收进衣柜'" @click="skinStudio.pickSkinFile">
              <Upload :size="13" /><span>上传本地皮肤</span>
            </button>
          </div>
        </header>
        <div class="panel-body">
          <div class="grid">
            <div
              class="card"
              :class="{ active: skinStudio.selectedKey === 'official' }"
              v-tooltip="'账号当前皮肤'"
              @click="skinStudio.selectSkin('official')"
            >
              <div v-if="skinStudio.officialSkin" class="thumb" :style="skinThumb(skinStudio.officialSkin)"></div>
              <div v-else class="thumb empty"></div>
              <div class="card-name">官方当前</div>
            </div>
            <div
              v-for="s in skinStudio.skins"
              :key="s.id"
              class="card"
              :class="{ active: skinStudio.selectedKey === s.id }"
              @click="skinStudio.selectSkin(s.id)"
            >
              <div v-if="s.skin" class="thumb" :style="skinThumb(s.skin)"></div>
              <div v-else class="thumb empty"></div>
              <div class="card-name" :title="s.name">
                <span class="nm">{{ s.name }}</span>
                <span class="vt">{{ s.variant }}</span>
              </div>
              <button class="card-del" v-tooltip="'从衣柜删除'" @click.stop="skinStudio.removeSkin(s)">
                <X :size="12" />
              </button>
            </div>
          </div>

          <LoadingSpinner v-if="skinStudio.skinsLoading" text="读取本地衣柜…" />
          <EmptyState v-else-if="!skinStudio.skins.length">
            <p>衣柜还是空的</p>
            <p>点「上传本地皮肤」把皮肤文件收进来</p>
          </EmptyState>
          <p v-if="skinStudio.skinsError" class="err">{{ skinStudio.skinsError }}</p>
        </div>
      </section>

      <section class="panel">
        <header class="panel-hd">
          <span class="panel-title"><Shirt :size="15" />披风</span>
          <div class="panel-hd-right">
            <span v-if="skinStudio.premium && skinStudio.capes.length" class="panel-count">
              {{ skinStudio.capes.filter(c => c.active).length ? "已装备" : "未装备" }}
            </span>
          </div>
        </header>
        <div class="panel-body">
          <template v-if="skinStudio.premium">
            <div class="grid">
              <div
                class="card"
                :class="{ active: skinStudio.selectedCapeId === '' }"
                v-tooltip="'不装备披风'"
                @click="skinStudio.useCape('')"
              >
                <div class="thumb cape none"><X :size="18" /></div>
                <div class="card-name">无披风</div>
                <span v-if="skinStudio.selectedCapeId === ''" class="card-tag ok">使用中</span>
              </div>
              <div
                v-for="c in skinStudio.capes"
                :key="c.id"
                class="card"
                :class="{ active: skinStudio.selectedCapeId === c.id }"
                @click="skinStudio.useCape(c.id)"
              >
                <div v-if="c.texture" class="thumb cape" :style="capeThumb(c.texture)"></div>
                <div v-else class="thumb cape empty"></div>
                <div class="card-name" :title="c.alias || c.id">
                  {{ c.alias || c.id.slice(0, 8) }}
                </div>
                <span v-if="c.active" class="card-tag ok">使用中</span>
              </div>
            </div>

            <LoadingSpinner v-if="skinStudio.capeLoading" text="读取披风列表…" />
            <EmptyState v-else-if="!skinStudio.capes.length">
              <p>该账号暂无可用披风</p>
            </EmptyState>
            <p v-if="skinStudio.capesError" class="err">{{ skinStudio.capesError }}</p>
          </template>
          <EmptyState v-else>
            <p>披风需要正版账号</p>
            <p>登录微软账号后可在此切换 / 移除披风</p>
          </EmptyState>
        </div>
      </section>
    </div>

    <!-- 上传皮肤前确认模型：CLASSIC（经典 Steve 宽）/ SLIM（纤细 Alex 窄） -->
    <AlertDialog v-model="skinStudio.skinModelDialog" title="选择皮肤模型">
      <div class="model-desc">
        皮肤将存入本地衣柜；模型不可中途更换（需重新上传），按所选模型预览与应用。
      </div>
      <template #actions>
        <button class="dialog-btn" @click="skinStudio.skinModelDialog = false">取消</button>
        <button class="dialog-btn danger" @click="skinStudio.confirmSaveSkin('CLASSIC')">经典模型（Steve）</button>
        <button class="dialog-btn" @click="skinStudio.confirmSaveSkin('SLIM')">纤细模型（Alex）</button>
      </template>
    </AlertDialog>
  </div>
</template>

<style scoped>
.studio {
  display: flex;
  height: 100%;
  min-height: 0;
}

/* ===== 左：3D 预览 ===== */
.stage-col {
  flex: 0 0 42%;
  min-width: 300px;
  display: flex;
  flex-direction: column;
  min-height: 0;
  background-color: var(--surface-elevated);
  border-right: 1px solid var(--border-default);
}

.stage {
  position: relative;
  flex: 1;
  min-height: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  /* 底部留白给「按住拖动」提示，避免与模型脚部重叠 */
  padding: 16px 16px 30px;
  overflow: hidden;
  cursor: grab;
  /* 拖动旋转：禁掉浏览器手势/选中，pointer 事件才不会被滚屏吞掉 */
  touch-action: none;
  user-select: none;
  -webkit-user-select: none;
  background:
    radial-gradient(circle at 50% 28%, var(--color-brand-bg), transparent 68%),
    var(--surface-page);
}

.stage.grabbing {
  cursor: grabbing;
}

.frame {
  max-width: 100%;
  max-height: 100%;
  image-rendering: pixelated;
  pointer-events: none;
  -webkit-user-drag: none;
}

.stage-tip {
  position: absolute;
  left: 0;
  right: 0;
  bottom: 10px;
  text-align: center;
  font-size: 11.5px;
  color: var(--text-tertiary);
  pointer-events: none;
}

.stage-bar {
  flex: none;
  border-top: 1px solid var(--border-default);
  padding: 10px 16px 12px;
  display: flex;
  flex-direction: column;
  gap: 9px;
}

.stage-meta {
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
}

.meta-name {
  font-size: 13px;
  font-weight: 600;
  color: var(--text-primary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.meta-tag {
  flex: none;
  padding: 1px 6px;
  border-radius: 4px;
  font-size: 10px;
  font-weight: 600;
  letter-spacing: 0.4px;
  color: var(--text-secondary);
  background-color: var(--surface-hover);
}

.meta-tag.local {
  color: var(--color-brand);
  background-color: var(--color-brand-bg);
}

.stage-actions {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
}

.tool {
  display: flex;
  align-items: center;
  gap: 5px;
  padding: 5px 11px;
  border-radius: var(--radius-sm);
  border: 1px solid var(--border-default);
  background-color: transparent;
  color: var(--text-secondary);
  font-size: 12px;
  white-space: nowrap;
  cursor: pointer;
  transition: all 0.15s;
}

.tool:hover:not(:disabled) {
  border-color: var(--color-brand);
  color: var(--color-brand);
  background-color: var(--color-brand-bg);
}

.tool:disabled {
  opacity: 0.45;
  cursor: not-allowed;
}

.tool.primary {
  border-color: transparent;
  background: var(--color-brand-gradient);
  color: var(--text-inverse);
  font-weight: 500;
}

.tool.primary:hover:not(:disabled) {
  color: var(--text-inverse);
  box-shadow: 0 0 0 3px var(--color-brand-glow);
}

.tool.sm {
  padding: 4px 9px;
  font-size: 11.5px;
}

.err {
  margin: 0;
  font-size: 11.5px;
  color: var(--color-danger);
  white-space: pre-line;
  word-break: break-all;
}

.hint {
  margin: 0;
  font-size: 11.5px;
  color: var(--text-tertiary);
}

/* ===== 右：衣柜 ===== */
.wardrobe {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
}

.panel {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
}

.panel + .panel {
  border-top: 1px solid var(--border-default);
}

.panel-hd {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  padding: 11px 16px;
  border-bottom: 1px solid var(--border-default);
}

.panel-title {
  display: flex;
  align-items: center;
  gap: 7px;
  font-size: 13.5px;
  font-weight: 600;
  color: var(--text-primary);
}

.panel-title svg {
  color: var(--color-brand);
}

.panel-hd-right {
  display: flex;
  align-items: center;
  gap: 10px;
}

.panel-count {
  font-size: 11.5px;
  color: var(--text-tertiary);
}

.panel-body {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  padding: 14px 16px 18px;
}

.grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(96px, 1fr));
  gap: 12px;
}

.card {
  position: relative;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 7px;
  padding: 10px 8px 8px;
  border: 1px solid var(--border-default);
  border-radius: var(--radius-md);
  background-color: var(--surface-card);
  cursor: pointer;
  transition: border-color 0.15s, background-color 0.15s, box-shadow 0.15s;
}

.card:hover {
  border-color: var(--color-brand);
  background-color: var(--color-brand-bg);
}

.card.active {
  border-color: var(--color-brand);
  box-shadow: 0 0 0 3px var(--color-brand-glow);
}

.thumb {
  width: 64px;
  height: 64px;
  border-radius: 4px;
  background-color: var(--surface-hover);
  background-repeat: no-repeat;
  image-rendering: pixelated;
}

/* 披风贴图正面为 10×16 体素 → 缩略图保持同比例（40×64） */
.thumb.cape {
  width: 40px;
  height: 64px;
}

.thumb.empty {
  background-image: none;
}

.thumb.cape.none {
  display: flex;
  align-items: center;
  justify-content: center;
  color: var(--text-tertiary);
}

.card-name {
  max-width: 100%;
  display: flex;
  align-items: center;
  gap: 4px;
  font-size: 12px;
  color: var(--text-secondary);
}

/* 名称超出省略，模型标签固定不压缩 */
.card-name .nm {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.card-name .vt {
  flex: none;
  padding: 0 4px;
  border: 1px solid var(--border-default);
  border-radius: 3px;
  font-size: 9px;
  font-weight: 700;
  letter-spacing: 0.3px;
  color: var(--text-tertiary);
}

.card.active .card-name {
  color: var(--color-brand);
  font-weight: 600;
}

.card-tag {
  position: absolute;
  top: 5px;
  left: 5px;
  padding: 1px 5px;
  border-radius: 4px;
  font-size: 9.5px;
  font-weight: 700;
  letter-spacing: 0.4px;
  color: var(--text-tertiary);
  background-color: var(--surface-hover);
}

.card-tag.ok {
  color: var(--text-inverse);
  background-color: var(--color-brand);
}

.card-del {
  position: absolute;
  top: 4px;
  right: 4px;
  display: flex;
  align-items: center;
  justify-content: center;
  width: 20px;
  height: 20px;
  border: none;
  border-radius: var(--radius-sm);
  background: var(--surface-card);
  color: var(--text-tertiary);
  cursor: pointer;
  opacity: 0;
  visibility: hidden; /* 隐藏态彻底移出命中测试（WebView2 下 pointer-events 过渡不可靠） */
  transition: opacity 0.12s, visibility 0.12s, color 0.12s, background-color 0.12s;
}

.card:hover .card-del {
  opacity: 1;
  visibility: visible;
}

.card-del:hover {
  color: var(--color-danger);
  background-color: var(--color-danger-bg);
}

.model-desc {
  font-size: 12.5px;
  color: var(--text-secondary);
  line-height: 1.6;
}
</style>
