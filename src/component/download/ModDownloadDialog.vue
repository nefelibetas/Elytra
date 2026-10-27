<script setup lang="ts">
// 下载确认弹窗：主 Mod + 必装/可选前置；分类折叠、整体滚动、单分类渲染上限 200。
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import { ChevronDown, CircleCheck, Info, LoaderCircle } from "@lucide/vue";
import { focusDialog, restoreFocus } from "../../utils/focusNav";
import { formatError } from "../../utils/error";
import DropSelect, { type SelOption } from "../common/DropSelect.vue";
import type {
  ModDependencyItem,
  ModDependencyPlan,
  ModDependencyVersion,
} from "../../store/community/modDependencies";

const props = defineProps<{
  modelValue: boolean;
  loading: boolean;
  updating: boolean;
  error: string;
  modName: string;
  modVersion: string;
  modFilename: string;
  modLoaders: string[];
  plan: ModDependencyPlan | null;
}>();

const emit = defineEmits<{
  "update:modelValue": [value: boolean];
  confirm: [selectedOptional: string[]];
  mainOnly: [];
  versionChange: [projectId: string, versionId: string];
}>();

const { t } = useI18n();
const RENDER_CAP = 200;

const card = ref<HTMLElement | null>(null);
let restoreTo: HTMLElement | null = null;
const requiredOpen = ref(true);
const optionalOpen = ref(true);
const selected = ref<Set<string>>(new Set());
let planSeen = false;

const items = computed<ModDependencyItem[]>(() => props.plan?.items ?? []);
// 必装树 = 不属于任何可选根；可选树 = 属某个可选根（含其 required 子依赖）
const requiredRows = computed(() => items.value.filter(item => item.root_optional === null));
const optionalRows = computed(() => items.value.filter(item => item.root_optional !== null));
const optionalRoots = computed(() =>
  optionalRows.value.filter(item => item.root_optional === item.project_id),
);
const planErrors = computed(() => props.plan?.errors ?? []);
const hasRequiredError = computed(
  () => planErrors.value.length > 0 || requiredRows.value.some(item => !!item.error),
);
const canDownload = computed(
  () => !props.loading && !props.updating && !hasRequiredError.value,
);
const requiredShown = computed(() => requiredRows.value.slice(0, RENDER_CAP));
const optionalShown = computed(() => optionalRows.value.slice(0, RENDER_CAP));

function errorText(value: string | null): string {
  return value ? formatError(value).summary : "";
}

function groupHasError(rootId: string): boolean {
  return optionalRows.value.some(item => item.root_optional === rootId && !!item.error);
}

function available(item: ModDependencyItem): boolean {
  if (item.root_optional === item.project_id && groupHasError(item.project_id)) return false;
  return !item.installed && !item.error && !!item.url && !!item.filename;
}

function selectedRoot(item: ModDependencyItem): boolean {
  return item.root_optional !== null && selected.value.has(item.root_optional);
}

function isRoot(item: ModDependencyItem): boolean {
  return item.root_optional === item.project_id;
}

function toggleRoot(item: ModDependencyItem) {
  if (!available(item)) return;
  const next = new Set(selected.value);
  if (next.has(item.project_id)) next.delete(item.project_id);
  else next.add(item.project_id);
  selected.value = next;
}

function selectAll() {
  selected.value = new Set(optionalRoots.value.filter(available).map(item => item.project_id));
}

function clearAll() {
  selected.value = new Set();
}

function versionLabel(version: ModDependencyVersion): string {
  return version.version_type === "beta" || version.version_type === "alpha"
    ? `${version.version_label} · ${version.version_type}`
    : version.version_label;
}

function versionOptions(item: ModDependencyItem): SelOption[] {
  return item.candidates.map(version => ({
    value: version.version_id,
    label: versionLabel(version),
  }));
}

function versionHint(item: ModDependencyItem): string {
  if (item.candidate_total > item.candidates.length) {
    return t("download.detail.modDepsVersionTruncated", {
      shown: item.candidates.length,
      total: item.candidate_total,
    });
  }
  return "";
}

function versionTitle(item: ModDependencyItem): string {
  const chosen = item.candidates.find(version => version.version_id === item.version_id);
  const label = chosen?.version_label ?? item.version_label;
  const hint = versionHint(item);
  return hint ? `${label}\n${hint}` : label;
}

function changeVersion(item: ModDependencyItem, versionId: string) {
  if (versionId && versionId !== item.version_id) {
    emit("versionChange", item.project_id, versionId);
  }
}

function close() {
  emit("update:modelValue", false);
}

function confirm() {
  emit("confirm", Array.from(selected.value));
}

function onKeydown(event: KeyboardEvent) {
  if (event.key === "Escape" && props.modelValue) close();
}

watch(
  () => props.modelValue,
  async open => {
    if (!open) {
      restoreFocus(restoreTo);
      restoreTo = null;
      return;
    }
    selected.value = new Set();
    planSeen = false;
    requiredOpen.value = true;
    optionalOpen.value = true;
    await nextTick();
    if (card.value) restoreTo = focusDialog(card.value);
  },
);

watch(
  () => props.plan,
  plan => {
    if (!plan || planSeen) return;
    planSeen = true;
    const roots = plan.items.filter(
      item => item.relation === "optional" && item.root_optional === item.project_id,
    );
    optionalOpen.value = roots.length <= 30;
  },
);

onMounted(() => document.addEventListener("keydown", onKeydown));
onUnmounted(() => document.removeEventListener("keydown", onKeydown));
</script>

<template>
  <Transition name="dialog">
    <div v-if="modelValue" class="dialog-overlay" @click.self="close">
      <div
        ref="card"
        class="dialog-card mod-dep-card"
        role="dialog"
        aria-modal="true"
        :aria-label="t('download.detail.modDepsTitle', { name: modName })"
        tabindex="-1"
      >
        <div class="dialog-header">
          <span class="dialog-title">{{ t("download.detail.modDepsTitle", { name: modName }) }}</span>
          <span
            v-if="updating"
            class="dep-updating-indicator"
            role="status"
            :aria-label="t('download.detail.modDepsUpdating')"
            :title="t('download.detail.modDepsUpdating')"
          >
            <LoaderCircle :size="14" class="dep-spin" aria-hidden="true" />
          </span>
        </div>

        <div class="dialog-body">
          <div class="dep-main">
            <div class="dep-main-head">
              <span class="dep-main-name" :title="modName">{{ modName }}</span>
              <span v-if="modLoaders.length" class="dep-main-tags">
                <span
                  v-for="loader in modLoaders"
                  :key="loader"
                  class="dep-tag"
                  :class="`dep-tag--${loader}`"
                >{{ loader }}</span>
              </span>
            </div>
            <div class="dep-meta">
              <div class="dep-meta-row">
                <span class="dep-meta-label">{{ t("download.detail.modDepsInstanceLabel") }}：</span>
                <span class="dep-meta-value">
                  {{ plan ? `${plan.instance_mc} / ${plan.instance_loader}` : "—" }}
                </span>
              </div>
              <div class="dep-meta-row">
                <span class="dep-meta-label">{{ t("download.detail.modDepsVersionLabel") }}：</span>
                <span class="dep-meta-value">{{ modVersion || "—" }}</span>
              </div>
              <div class="dep-meta-row">
                <span class="dep-meta-label">{{ t("download.detail.modDepsFilenameLabel") }}：</span>
                <span class="dep-meta-value" :title="modFilename">{{ modFilename || "—" }}</span>
              </div>
            </div>
          </div>

          <div v-if="loading" class="dep-state">{{ t("download.detail.modDepsLoading") }}</div>
          <div v-else-if="error" class="dep-state dep-error-text">{{ error }}</div>
          <template v-else-if="plan">
            <div
              v-if="requiredRows.length || optionalRows.length"
              class="dep-version-hint"
            >{{ t("download.detail.modDepsVersionHint") }}</div>
            <div v-if="planErrors.length" class="dep-errors">
              <div v-for="message in planErrors" :key="message" class="dep-error-text">{{ errorText(message) }}</div>
            </div>

            <section v-if="requiredRows.length" class="dep-section">
              <button
                class="dep-section-head"
                :aria-expanded="requiredOpen"
                @click="requiredOpen = !requiredOpen"
              >
                <ChevronDown :size="14" class="dep-caret" :class="{ collapsed: !requiredOpen }" />
                <span>{{ t("download.detail.modDepsRequired") }}</span>
                <span class="dep-count">{{ requiredRows.length }}</span>
              </button>
              <Transition name="expand">
                <div v-if="requiredOpen" class="dep-list">
                  <div
                    v-for="row in requiredShown"
                    :key="row.project_id"
                    class="dep-row"
                    :class="{ 'dep-row--error': row.error, 'dep-row--muted': row.installed }"
                  >
                    <div class="dep-row-main">
                      <span class="dep-name" :title="row.name">{{ row.name }}</span>
                      <span
                        v-if="row.loader"
                        class="dep-tag"
                        :class="`dep-tag--${row.loader}`"
                      >{{ row.loader }}</span>
                      <span
                        v-if="row.installed"
                        class="dep-status dep-status--installed"
                        role="img"
                        :title="t('download.detail.modDepsInstalled')"
                        :aria-label="t('download.detail.modDepsInstalled')"
                      >
                        <CircleCheck :size="14" aria-hidden="true" />
                      </span>
                      <span
                        v-else-if="row.installed_version_label"
                        class="dep-status dep-status--other"
                        role="img"
                        :title="t('download.detail.modDepsInstalledOther', { version: row.installed_version_label })"
                        :aria-label="t('download.detail.modDepsInstalledOther', { version: row.installed_version_label })"
                      >
                        <Info :size="14" aria-hidden="true" />
                      </span>
                      <span v-if="row.error" class="dep-row-error">{{ errorText(row.error) }}</span>
                    </div>
                    <span class="dep-row-side">
                      <DropSelect
                        v-if="row.candidates.length"
                        class="dep-version-select"
                        :model-value="row.version_id"
                        :options="versionOptions(row)"
                        :disabled="updating"
                        width="100%"
                        max-height="220px"
                        :title="row.error ? errorText(row.error) : versionTitle(row)"
                        @update:model-value="changeVersion(row, $event)"
                      />
                      <span v-else class="dep-version">{{ row.version_label || "—" }}</span>

                    </span>
                  </div>
                  <div v-if="requiredRows.length > RENDER_CAP" class="dep-more">
                    {{ t("download.detail.modDepsHidden", { shown: RENDER_CAP, total: requiredRows.length }) }}
                  </div>
                </div>
              </Transition>
            </section>

            <section v-if="optionalRows.length" class="dep-section">
              <div class="dep-section-head dep-section-head--split">
                <button
                  class="dep-section-toggle"
                  :aria-expanded="optionalOpen"
                  @click="optionalOpen = !optionalOpen"
                >
                  <ChevronDown :size="14" class="dep-caret" :class="{ collapsed: !optionalOpen }" />
                  <span>{{ t("download.detail.modDepsOptional") }}</span>
                  <span class="dep-count">{{ optionalRoots.length }}</span>
                </button>
                <div class="dep-actions">
                  <button class="dep-link" @click="selectAll">{{ t("download.detail.modDepsSelectAll") }}</button>
                  <button class="dep-link" @click="clearAll">{{ t("download.detail.modDepsClear") }}</button>
                </div>
              </div>
              <Transition name="expand">
                <div v-if="optionalOpen" class="dep-list">
                  <div
                    v-for="row in optionalShown"
                    :key="row.project_id"
                    class="dep-row"
                    :class="{
                      'dep-row--child': !isRoot(row),
                      'dep-row--error': row.error,
                      'dep-row--muted': row.installed,
                    }"
                  >
                    <input
                      v-if="isRoot(row)"
                      type="checkbox"
                      class="dep-check"
                      :checked="selectedRoot(row)"
                      :disabled="updating || !available(row)"
                      :aria-label="row.name"
                      @change="toggleRoot(row)"
                    />
                    <span v-else class="dep-indent">↳</span>
                    <div class="dep-row-main">
                      <span class="dep-name" :title="row.name">{{ row.name }}</span>
                      <span
                        v-if="row.loader"
                        class="dep-tag"
                        :class="`dep-tag--${row.loader}`"
                      >{{ row.loader }}</span>
                      <span
                        v-if="row.installed"
                        class="dep-status dep-status--installed"
                        role="img"
                        :title="t('download.detail.modDepsInstalled')"
                        :aria-label="t('download.detail.modDepsInstalled')"
                      >
                        <CircleCheck :size="14" aria-hidden="true" />
                      </span>
                      <span
                        v-else-if="row.installed_version_label"
                        class="dep-status dep-status--other"
                        role="img"
                        :title="t('download.detail.modDepsInstalledOther', { version: row.installed_version_label })"
                        :aria-label="t('download.detail.modDepsInstalledOther', { version: row.installed_version_label })"
                      >
                        <Info :size="14" aria-hidden="true" />
                      </span>
                      <span v-if="row.error" class="dep-row-error">{{ errorText(row.error) }}</span>
                    </div>
                    <span class="dep-row-side">
                      <DropSelect
                        v-if="row.candidates.length"
                        class="dep-version-select"
                        :model-value="row.version_id"
                        :options="versionOptions(row)"
                        :disabled="updating"
                        width="100%"
                        max-height="220px"
                        :title="row.error ? errorText(row.error) : versionTitle(row)"
                        @update:model-value="changeVersion(row, $event)"
                      />
                      <span v-else class="dep-version">{{ row.version_label || "—" }}</span>

                    </span>
                  </div>
                  <div v-if="optionalRows.length > RENDER_CAP" class="dep-more">
                    {{ t("download.detail.modDepsHidden", { shown: RENDER_CAP, total: optionalRows.length }) }}
                  </div>
                </div>
              </Transition>
            </section>

            <div v-if="!requiredRows.length && !optionalRows.length" class="dep-state">
              {{ t("download.detail.modDepsEmpty") }}
            </div>
          </template>
        </div>

        <div class="dialog-footer">
          <button class="dialog-btn" @click="close">{{ t("download.detail.modDepsCancel") }}</button>
          <button class="dialog-btn" :disabled="updating" @click="emit('mainOnly')">{{ t("download.detail.modDepsMainOnly") }}</button>
          <button class="dialog-btn primary" :disabled="!canDownload" @click="confirm">
            {{ t("download.detail.modDepsDownload") }}
          </button>
        </div>
      </div>
    </div>
  </Transition>
</template>

<style scoped>
/* 遮罩从标题栏下方开始：模态确认不阻塞标题栏导航与窗口控制。 */
.dialog-overlay {
  position: fixed;
  top: var(--titlebar-h, 48px);
  right: 0;
  bottom: 0;
  left: 0;
  z-index: var(--z-modal, 1100);
  display: flex;
  align-items: center;
  justify-content: center;
  background: transparent;
}

/* 遮罩模糊放在伪元素：backdrop-filter 会为 fixed 子元素创建包含块，导致版本下拉定位偏移。 */
.dialog-overlay::before {
  content: "";
  position: absolute;
  inset: 0;
  z-index: -1;
  background: var(--overlay, rgba(15, 23, 42, 0.5));
  backdrop-filter: blur(2px);
  pointer-events: none;
}

@supports not (backdrop-filter: blur(1px)) {
  .dialog-overlay::before {
    background: rgba(15, 23, 42, 0.68);
  }
}

.dialog-card {
  display: flex;
  flex-direction: column;
  background: var(--surface-card);
  border-radius: var(--radius-lg);
  box-shadow: var(--shadow-lg);
  overflow: hidden;
  user-select: none;
}

.mod-dep-card {
  width: min(560px, 92vw);
  max-height: calc(100vh - var(--titlebar-h, 48px) - 32px);
}

.dialog-header {
  position: relative;
  flex: none;
  padding: 14px 20px 12px;
}

.dialog-header::after {
  content: "";
  position: absolute;
  left: 20px;
  right: 20px;
  bottom: 0;
  height: 1px;
  background: var(--text-tertiary);
}

.dialog-title {
  font-size: 16px;
  font-weight: 700;
  color: var(--color-brand);
}

.dialog-body {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  padding: 14px 20px;
}

.dialog-footer {
  position: relative;
  flex: none;
  display: flex;
  justify-content: flex-end;
  align-items: center;
  gap: 8px;
  padding: 12px 20px;
}

.dialog-footer::before {
  content: "";
  position: absolute;
  left: 20px;
  right: 20px;
  top: 0;
  height: 1px;
  background: var(--border-default);
}

.dep-main {
  padding: 10px 12px;
  margin-bottom: 14px;
  background: var(--surface-page);
  border: 1px solid var(--border-default);
  border-radius: var(--radius-md);
}

.dep-main-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
  min-width: 0;
}

.dep-main-name {
  flex: 1;
  min-width: 0;
  font-size: 14px;
  font-weight: 600;
  color: var(--text-primary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.dep-main-tags {
  flex: none;
  display: flex;
  flex-wrap: wrap;
  justify-content: flex-end;
  gap: 4px;
}

.dep-meta {
  display: flex;
  flex-direction: column;
  gap: 4px;
  margin-top: 8px;
}

.dep-meta-row {
  display: flex;
  align-items: baseline;
  gap: 6px;
  min-width: 0;
  font-size: 12px;
  line-height: 1.5;
}

.dep-meta-label {
  flex: none;
  color: var(--text-tertiary);
}

.dep-meta-value {
  min-width: 0;
  color: var(--text-primary);
  font-weight: 500;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.dep-state {
  padding: 18px 0;
  text-align: center;
  font-size: 13px;
  color: var(--text-secondary);
}

.dep-updating-indicator {
  position: absolute;
  top: 50%;
  right: 20px;
  display: inline-flex;
  color: var(--text-tertiary);
  transform: translateY(-50%);
}

.dep-spin {
  animation: dep-spin 0.8s linear infinite;
}

@keyframes dep-spin {
  to { transform: rotate(360deg); }
}

.dep-version-hint {
  margin-bottom: 10px;
  color: var(--text-tertiary);
  font-size: 11.5px;
  line-height: 1.5;
}

.dep-errors {
  margin-bottom: 10px;
  padding: 8px 10px;
  border: 1px solid var(--color-danger);
  border-radius: var(--radius-md);
  background: var(--surface-page);
}

.dep-error-text {
  color: var(--color-danger);
  font-size: 12.5px;
  line-height: 1.6;
  word-break: break-word;
}

.dep-section + .dep-section {
  margin-top: 12px;
}

.dep-section-head {
  display: flex;
  align-items: center;
  gap: 6px;
  width: 100%;
  padding: 0 2px 7px;
  border: 0;
  background: transparent;
  color: var(--text-secondary);
  font-size: 12.5px;
  font-weight: 600;
  cursor: pointer;
  text-align: left;
}

.dep-section-head--split {
  justify-content: space-between;
}

.dep-section-toggle {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 0;
  border: 0;
  background: transparent;
  color: inherit;
  font: inherit;
  cursor: pointer;
}

.dep-caret {
  flex: none;
  transition: transform 0.15s ease;
}

.dep-caret.collapsed {
  transform: rotate(-90deg);
}

.dep-count {
  padding: 0 6px;
  border-radius: 999px;
  background: var(--surface-hover);
  color: var(--text-tertiary);
  font-size: 11px;
  font-weight: 500;
  line-height: 16px;
}

.dep-actions {
  display: flex;
  gap: 10px;
}

.dep-link {
  padding: 0;
  border: 0;
  background: transparent;
  color: var(--color-brand);
  font-size: 12px;
  cursor: pointer;
}

.dep-link:hover {
  text-decoration: underline;
}

.dep-list {
  overflow: hidden;
  border: 1px solid var(--border-default);
  border-radius: var(--radius-md);
  background: var(--surface-page);
}

.dep-row {
  display: flex;
  align-items: center;
  gap: 8px;
  min-height: 34px;
  padding: 8px 10px;
  font-size: 12.5px;
}

.dep-row + .dep-row {
  border-top: 1px solid var(--border-default);
}

.dep-row--child {
  padding-left: 34px;
}

.dep-row--child .dep-name {
  color: var(--text-secondary);
}

.dep-row--muted {
  background: var(--surface-hover);
}

.dep-row--error .dep-name {
  color: var(--color-danger);
}

.dep-check {
  flex: none;
  margin: 0;
  accent-color: var(--color-brand);
}

.dep-indent {
  flex: none;
  width: 13px;
  color: var(--text-tertiary);
}

.dep-row-main {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 6px;
}

.dep-row-error {
  flex-basis: 100%;
  min-width: 0;
  color: var(--color-danger);
  font-size: 11.5px;
  line-height: 1.5;
  word-break: break-word;
}

.dep-name {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: var(--text-primary);
}

.dep-tag {
  flex: none;
  padding: 0 5px;
  border-radius: 6px;
  background: var(--color-brand-bg);
  color: var(--color-brand);
  font-size: 10px;
  font-weight: 600;
  line-height: 16px;
}

.dep-tag--fabric { color: var(--ld-fabric); }
.dep-tag--forge { color: var(--ld-forge); }
.dep-tag--neoforge { color: var(--ld-neoforge); }
.dep-tag--quilt { color: var(--ld-quilt); }
.dep-tag--optifine { color: var(--ld-optifine); }

/* 与外部列表/详情页同款：低版本 WebKitGTK 无 color-mix 时保留纯文字色徽章 */
@supports (color-mix(in srgb, red 12%, transparent)) {
  .dep-tag--fabric { background: color-mix(in srgb, var(--ld-fabric) 12%, transparent); }
  .dep-tag--forge { background: color-mix(in srgb, var(--ld-forge) 12%, transparent); }
  .dep-tag--neoforge { background: color-mix(in srgb, var(--ld-neoforge) 12%, transparent); }
  .dep-tag--quilt { background: color-mix(in srgb, var(--ld-quilt) 12%, transparent); }
  .dep-tag--optifine { background: color-mix(in srgb, var(--ld-optifine) 12%, transparent); }
}

.dep-row-side {
  flex: 0 0 230px;
  display: flex;
  align-items: center;
  justify-content: flex-end;
  max-width: 58%;
  min-width: 0;
}

.dep-version-select {
  width: 100%;
  min-width: 0;
  max-width: 230px;
}

.dep-version-select :deep(.ds-btn) {
  height: 26px;
  padding: 0 8px;
  font-size: 12px;
}

.dep-version {
  flex: none;
  min-width: 0;
  overflow: hidden;
  color: var(--text-secondary);
  font-size: 12px;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.dep-status {
  flex: none;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 16px;
  height: 16px;
  cursor: help;
}

.dep-status--installed {
  color: var(--color-success);
}

.dep-status--other {
  color: var(--text-tertiary);
}

.dep-more {
  padding: 8px 10px;
  border-top: 1px solid var(--border-default);
  color: var(--text-tertiary);
  font-size: 11.5px;
}

.dialog-btn {
  padding: 6px 14px;
  border: 1px solid var(--border-strong);
  border-radius: var(--radius-sm);
  background: transparent;
  color: var(--text-primary);
  font-size: 13px;
  cursor: pointer;
  transition: all 0.15s;
}

.dialog-btn:hover {
  border-color: var(--color-brand);
  color: var(--color-brand);
}

.dialog-btn.primary {
  background: var(--color-brand);
  color: var(--text-inverse);
}

.dialog-btn.primary:hover {
  opacity: 0.88;
}

.dialog-btn:disabled {
  opacity: 0.45;
  cursor: not-allowed;
}

.dialog-enter-active,
.dialog-leave-active {
  transition: opacity 0.18s ease;
}

.dialog-enter-from,
.dialog-leave-to {
  opacity: 0;
}

.dialog-enter-active .dialog-card,
.dialog-leave-active .dialog-card {
  transition: transform 0.18s ease;
}

.dialog-enter-from .dialog-card,
.dialog-leave-to .dialog-card {
  transform: translateY(8px) scale(0.98);
}
</style>
