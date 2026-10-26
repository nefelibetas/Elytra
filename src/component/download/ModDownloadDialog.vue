<script setup lang="ts">
// 下载确认弹窗：主 Mod + 必装/可选前置；分类折叠、整体滚动、单分类渲染上限 200。
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from "vue";
import { useI18n } from "vue-i18n";
import { ChevronDown } from "@lucide/vue";
import { focusDialog, restoreFocus } from "../../utils/focusNav";
import { formatError } from "../../utils/error";
import type { ModDependencyItem, ModDependencyPlan } from "../../store/community/modDependencies";

const props = defineProps<{
  modelValue: boolean;
  loading: boolean;
  error: string;
  modName: string;
  modVersion: string;
  modLoader: string;
  plan: ModDependencyPlan | null;
}>();

const emit = defineEmits<{
  "update:modelValue": [value: boolean];
  confirm: [selectedOptional: string[]];
  mainOnly: [];
}>();

const { t } = useI18n();
const RENDER_CAP = 200;

const card = ref<HTMLElement | null>(null);
let restoreTo: HTMLElement | null = null;
const requiredOpen = ref(true);
const optionalOpen = ref(true);
const selected = ref<Set<string>>(new Set());

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
const canDownload = computed(() => !props.loading && !hasRequiredError.value);
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
    requiredOpen.value = true;
    optionalOpen.value = true;
    await nextTick();
    if (card.value) restoreTo = focusDialog(card.value);
  },
);

watch(
  () => props.plan,
  plan => {
    if (!plan) return;
    selected.value = new Set();
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
        </div>

        <div class="dialog-body">
          <div class="dep-main">
            <div class="dep-main-name" :title="modName">{{ modName }}</div>
            <div class="dep-main-meta">
              <span v-if="modLoader" class="dep-tag">{{ modLoader }}</span>
              <span v-if="modVersion" class="dep-version">{{ modVersion }}</span>
            </div>
            <div v-if="plan" class="dep-instance">
              {{ t("download.detail.modDepsInstance", { mc: plan.instance_mc, loader: plan.instance_loader }) }}
            </div>
          </div>

          <div v-if="loading" class="dep-state">{{ t("download.detail.modDepsLoading") }}</div>
          <div v-else-if="error" class="dep-state dep-error-text">{{ error }}</div>
          <template v-else-if="plan">
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
                      <span v-if="row.loader" class="dep-tag">{{ row.loader }}</span>
                    </div>
                    <span class="dep-row-side">
                      <span v-if="row.installed" class="dep-note">{{ t("download.detail.modDepsInstalled") }}</span>
                      <span v-else-if="row.error" class="dep-error-text">{{ errorText(row.error) }}</span>
                      <span v-else class="dep-version">{{ row.version_label }}</span>
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
                  <label
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
                      :disabled="!available(row)"
                      @change="toggleRoot(row)"
                    />
                    <span v-else class="dep-indent">↳</span>
                    <div class="dep-row-main">
                      <span class="dep-name" :title="row.name">{{ row.name }}</span>
                      <span v-if="row.loader" class="dep-tag">{{ row.loader }}</span>
                    </div>
                    <span class="dep-row-side">
                      <span v-if="row.installed" class="dep-note">{{ t("download.detail.modDepsInstalled") }}</span>
                      <span v-else-if="row.error" class="dep-error-text">{{ errorText(row.error) }}</span>
                      <span v-else class="dep-version">{{ row.version_label }}</span>
                    </span>
                  </label>
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
          <button class="dialog-btn" @click="emit('mainOnly')">{{ t("download.detail.modDepsMainOnly") }}</button>
          <button class="dialog-btn primary" :disabled="!canDownload" @click="confirm">
            {{ t("download.detail.modDepsDownload") }}
          </button>
        </div>
      </div>
    </div>
  </Transition>
</template>

<style scoped>
.dialog-overlay {
  position: fixed;
  inset: 0;
  z-index: var(--z-modal, 1100);
  display: flex;
  align-items: center;
  justify-content: center;
  background: var(--overlay);
  backdrop-filter: blur(2px);
}

@supports not (backdrop-filter: blur(1px)) {
  .dialog-overlay {
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
  max-height: calc(100vh - 48px);
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
  padding: 2px 0 10px;
}

.dep-main-name {
  font-size: 14px;
  font-weight: 600;
  color: var(--text-primary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.dep-main-meta {
  display: flex;
  align-items: center;
  gap: 6px;
  margin-top: 6px;
}

.dep-instance {
  margin-top: 6px;
  font-size: 12px;
  color: var(--text-tertiary);
}

.dep-state {
  padding: 18px 0;
  text-align: center;
  font-size: 13px;
  color: var(--text-secondary);
}

.dep-errors {
  margin-bottom: 8px;
}

.dep-error-text {
  color: var(--color-danger);
  font-size: 12.5px;
  line-height: 1.6;
  word-break: break-word;
}

.dep-section + .dep-section {
  margin-top: 6px;
}

.dep-section-head {
  display: flex;
  align-items: center;
  gap: 6px;
  width: 100%;
  padding: 7px 0;
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
  color: var(--text-tertiary);
  font-weight: 500;
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
}

.dep-row {
  display: flex;
  align-items: center;
  gap: 8px;
  min-height: 32px;
  padding: 4px 0;
  border-top: 1px solid var(--border-default);
  font-size: 12.5px;
}

.dep-row--child {
  padding-left: 20px;
}

.dep-row--muted {
  opacity: 0.62;
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
  align-items: center;
  gap: 6px;
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
  padding: 1px 5px;
  border: 1px solid var(--border-default);
  border-radius: var(--radius-sm);
  color: var(--text-tertiary);
  font-size: 10.5px;
  line-height: 1.4;
}

.dep-row-side {
  flex: none;
  max-width: 42%;
  text-align: right;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.dep-version {
  color: var(--text-secondary);
  font-size: 12px;
}

.dep-note {
  color: var(--text-tertiary);
  font-size: 11.5px;
}

.dep-more {
  padding: 8px 0 2px;
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
