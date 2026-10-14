<script setup lang="ts">
// 自动任务编辑器（双模式复用）：instance 版本设置页，实例文件存在即完全采用实例配置，否则跟随全局；
// global 设置页直接编辑全局配置（无实例上下文，"立即执行"不可用）。
import { computed, onMounted, ref } from "vue";
import { useRoute } from "vue-router";
import { Play, Plus, Trash2, Undo2 } from "@lucide/vue";
import {
  DEFAULT_KEEP, DEFAULT_TIMEOUT, actionKind, actionLabel, builtinCatalog, makeAction,
  getInstanceAutoTasks, saveInstanceAutoTasks, saveGlobalAutoTasks, getGlobalAutoTasks,
  runAutoTasksNow, autoTaskProgress, phases,
} from "../../store/config/autoTasks";
import type { AutoTasks } from "../../types/launcher/AutoTasks";
import type { AutoTaskPhase } from "../../types/launcher/AutoTaskPhase";
import type { AutoTaskStep } from "../../types/launcher/AutoTaskStep";
import { worlds, loadWorlds } from "../../store/task/versionDetail";
import { showToast } from "../../utils/toast";
import { errMsg } from "../../utils/error";

const props = withDefaults(defineProps<{ mode?: "instance" | "global" }>(), {
  mode: "instance",
});

const route = useRoute();
/** 游戏 id（版本目录名），来自路由 query。 */
const versionName = computed(() => String(route.query.name ?? ""));

const isInstance = computed(() => props.mode === "instance");

/** 全局模式恒可编辑；实例模式需切到"自定义"。 */
const editing = computed(() => !isInstance.value || mode.value === "custom");

const mode = ref<"global" | "custom">("global");
const globalAutoTasks = ref<AutoTasks | null>(null);
const draft = ref<AutoTasks>({ pre_launch: [], on_crash: [], post_exit: [], pre_on_failure: "abort" });
const loaded = ref(false);
const saving = ref(false);
const expandAdd = ref<AutoTaskPhase | null>(null);

/** 深拷贝，避免草稿污染全局快照（三端 WebView 保守写法）。 */
function clone<T>(value: T): T {
  return JSON.parse(JSON.stringify(value)) as T;
}

function stepsOf(phase: AutoTaskPhase): AutoTaskStep[] {
  return draft.value[phase];
}

onMounted(async () => {
  try {
    if (!isInstance.value) {
      draft.value = await getGlobalAutoTasks();
    } else {
      const state = await getInstanceAutoTasks(versionName.value);
      globalAutoTasks.value = state.global;
      if (state.instance) {
        mode.value = "custom";
        draft.value = clone(state.instance);
      } else {
        mode.value = "global";
        draft.value = clone(state.global);
      }
      await loadWorlds(versionName.value);
    }
    loaded.value = true;
  } catch (e) {
    showToast(errMsg(e));
  }
});

/** 切到"自定义"：预填当前全局值。 */
function switchToCustom() {
  if (mode.value === "custom") return;
  if (globalAutoTasks.value) draft.value = clone(globalAutoTasks.value);
  mode.value = "custom";
}

/** 切回"全局"：丢弃实例配置，保存后生效。 */
function switchToGlobal() {
  mode.value = "global";
}

async function save() {
  saving.value = true;
  try {
    if (!isInstance.value) {
      await saveGlobalAutoTasks(draft.value);
      showToast("已保存全局自动任务");
    } else {
      await saveInstanceAutoTasks(versionName.value, mode.value === "custom" ? draft.value : null);
      showToast(mode.value === "custom" ? "已保存实例自动任务" : "已改为使用全局自动任务");
    }
  } catch (e) {
    showToast(errMsg(e));
  } finally {
    saving.value = false;
  }
}

function addAction(phase: AutoTaskPhase, kind: string) {
  stepsOf(phase).push({ enabled: true, action: makeAction(kind) });
  expandAdd.value = null;
}

async function runNow(phase: AutoTaskPhase) {
  try {
    await runAutoTasksNow(versionName.value, phase);
    showToast("自动任务执行完成");
  } catch (e) {
    showToast(errMsg(e));
  }
}

/** 当前实例 + 阶段是否有进行中的自动任务（事件载荷匹配）。 */
function runningPhase(): AutoTaskPhase | null {
  const p = autoTaskProgress.value;
  if (!p || p.version_id !== versionName.value) return null;
  return p.phase;
}

/** 命令动作的命令对象（非命令动作返回占位，保证模板引用稳定）。 */
function commandAction(step: AutoTaskStep) {
  if (typeof step.action === "object" && "Command" in step.action) {
    return step.action.Command;
  }
  return { command: "", timeout_secs: DEFAULT_TIMEOUT };
}

function backupAction(step: AutoTaskStep) {
  if (typeof step.action === "object" && "BackupSaves" in step.action) {
    return step.action.BackupSaves;
  }
  return { scope: "All" as const, keep: DEFAULT_KEEP };
}

/** 备份范围是否已限定世界（决定"指定世界"分支）。 */
function scopedWorlds(step: AutoTaskStep): string[] {
  const scope = backupAction(step).scope;
  return scope === "All" ? [] : scope.Selected.worlds;
}

function toggleWorld(name: string, step: AutoTaskStep) {
  const backup = backupAction(step);
  const list = [...scopedWorlds(step)];
  const idx = list.indexOf(name);
  if (idx >= 0) list.splice(idx, 1);
  else list.push(name);
  backup.scope = list.length ? { Selected: { worlds: list } } : "All";
}
</script>

<template>
  <div :class="isInstance ? 'vs-page' : 'auto-tasks-global'">
    <div class="setup-card">
      <div class="setup-card-hdr">
        <div class="setup-card-titles">
          <div class="setup-card-title">自动任务</div>
          <div class="setup-desc">
            启动前 / 崩溃时 / 退出后自动执行的动作；内置动作跨平台，自定义命令走系统命令行
          </div>
        </div>
        <button class="vsh-save" :disabled="saving || !loaded" @click="save">保存</button>
      </div>

      <div v-if="isInstance" class="vsh-mode">
        <button
          class="vsh-mode-btn"
          :class="{ active: mode === 'global' }"
          @click="switchToGlobal"
        >
          使用全局自动任务
        </button>
        <button
          class="vsh-mode-btn"
          :class="{ active: mode === 'custom' }"
          @click="switchToCustom"
        >
          使用自定义自动任务
        </button>
        <span class="vsh-mode-hint">
          {{
            mode === "custom"
              ? "该实例完全采用下方配置；全局改动不再影响它"
              : "该实例跟随全局配置（在设置-启动选项里改）"
          }}
        </span>
      </div>

      <!-- 三阶段 -->
      <div v-if="editing" class="vsh-phases">
        <div v-for="p in phases" :key="p.key" class="vsh-phase">
          <div class="vsh-phase-hdr">
            <div class="vsh-phase-title">{{ p.label }}</div>
            <div class="vsh-phase-desc">{{ p.desc }}</div>
            <button
              v-if="isInstance"
              class="vsh-run"
              :title="`立即执行一次（${p.label}）`"
              @click="runNow(p.key)"
            >
              <Play :size="12" /> 立即执行
            </button>
          </div>

          <div v-if="runningPhase() === p.key && autoTaskProgress" class="vsh-progress">
            {{ autoTaskProgress.action }} · {{ autoTaskProgress.message }}
            ({{ autoTaskProgress.step_index + 1 }}/{{ autoTaskProgress.step_total }})
          </div>

          <div v-if="!stepsOf(p.key).length" class="vsh-empty">尚未配置动作</div>

          <div v-for="(step, i) in stepsOf(p.key)" :key="i" class="vsh-step">
            <div class="vsh-step-hdr">
              <label class="vsh-step-enable">
                <input v-model="step.enabled" type="checkbox" />
                <span class="vsh-step-kind" :class="`kind-${actionKind(step.action)}`">
                  {{ actionKind(step.action) === "command" ? "命令" : "内置" }}
                </span>
                <span class="vsh-step-label">{{ actionLabel(step.action) }}</span>
              </label>
              <button class="vsh-del" title="删除" @click="stepsOf(p.key).splice(i, 1)">
                <Trash2 :size="12" />
              </button>
            </div>

            <!-- 自定义命令编辑 -->
            <div v-if="actionKind(step.action) === 'command'" class="vsh-edit">
              <input
                v-model="commandAction(step).command"
                class="vsh-input"
                type="text"
                placeholder="如：echo hello > D:\backup.txt（Windows cmd /C，其余 sh -c）"
                spellcheck="false"
                autocomplete="off"
              />
              <label class="vsh-timeout">
                超时
                <input v-model.number="commandAction(step).timeout_secs" type="number" min="1" max="3600" />
                秒
              </label>
            </div>

            <!-- 备份存档编辑 -->
            <div v-else-if="actionKind(step.action) === 'backup_saves'" class="vsh-edit vsh-backup">
              <div class="vsh-backup-row">
                <label class="vsh-radio">
                  <input
                    type="radio"
                    :checked="backupAction(step).scope === 'All'"
                    @change="backupAction(step).scope = 'All'"
                  />
                  全部世界
                </label>
                <label class="vsh-radio">
                  <input
                    type="radio"
                    :checked="backupAction(step).scope !== 'All'"
                    @change="toggleWorld(worlds[0]?.name ?? '', step)"
                  />
                  指定世界
                </label>
                <label class="vsh-timeout">
                  保留
                  <input v-model.number="backupAction(step).keep" type="number" min="1" max="50" />
                  份
                </label>
              </div>
              <div v-if="backupAction(step).scope !== 'All'" class="vsh-worlds">
                <div v-if="!worlds.length" class="vsh-empty">未发现存档</div>
                <label v-for="w in worlds" :key="w.name" class="vsh-world">
                  <input
                    type="checkbox"
                    :checked="scopedWorlds(step).includes(w.name)"
                    @change="toggleWorld(w.name, step)"
                  />
                  {{ w.name }}
                </label>
              </div>
            </div>
          </div>

          <div class="vsh-add">
            <button class="vsh-add-btn" @click="expandAdd = expandAdd === p.key ? null : p.key">
              <Plus :size="12" /> 添加动作
            </button>
            <div v-if="expandAdd === p.key" class="vsh-add-menu">
              <button
                v-for="b in builtinCatalog"
                :key="b.kind"
                class="vsh-add-item"
                :title="b.desc"
                @click="addAction(p.key, b.kind)"
              >
                {{ b.label }}
              </button>
              <button class="vsh-add-item cmd" @click="addAction(p.key, 'command')">
                自定义命令
              </button>
            </div>
          </div>
        </div>

        <div class="vsh-prefailure">
          <span class="vsh-prefailure-label">启动前动作失败时</span>
          <label class="vsh-radio">
            <input
              v-model="draft.pre_on_failure"
              type="radio"
              value="abort"
            />
            终止启动
          </label>
          <label class="vsh-radio">
            <input
              v-model="draft.pre_on_failure"
              type="radio"
              value="continue"
            />
            继续启动
          </label>
        </div>
      </div>

      <!-- 实例模式且未自定义：全局只读预览 -->
      <div v-else-if="isInstance" class="vsh-phases">
        <div v-for="p in phases" :key="p.key" class="vsh-phase">
          <div class="vsh-phase-hdr">
            <div class="vsh-phase-title">{{ p.label }}</div>
            <div class="vsh-phase-desc">{{ p.desc }}</div>
            <button v-if="isInstance" class="vsh-run" @click="runNow(p.key)">
              <Play :size="12" /> 立即执行
            </button>
          </div>
          <div v-if="!globalAutoTasks || !globalAutoTasks[p.key].length" class="vsh-empty">全局未配置动作</div>
          <div v-for="(step, i) in globalAutoTasks?.[p.key] ?? []" :key="i" class="vsh-step readonly">
            <span class="vsh-step-kind" :class="`kind-${actionKind(step.action)}`">
              {{ actionKind(step.action) === "command" ? "命令" : "内置" }}
            </span>
            <span class="vsh-step-label">{{ actionLabel(step.action) }}</span>
            <span v-if="!step.enabled" class="vsh-step-off">（已停用）</span>
          </div>
        </div>
        <div class="vsh-note">
          <Undo2 :size="12" />
          全局自动任务在「设置 → 启动设置」中编辑；本页点「使用自定义自动任务」可为该实例单独配置。
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
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
  padding: 14px 16px;
}
.setup-card-hdr {
  display: flex;
  align-items: flex-end;
  justify-content: space-between;
  gap: 12px;
  margin-bottom: 12px;
}
.setup-card-titles {
  min-width: 0;
}
.setup-card-title {
  font-size: 12px;
  font-weight: 600;
  color: var(--text-tertiary);
  letter-spacing: 0.5px;
}
.setup-desc {
  margin-top: 2px;
  font-size: 12px;
  color: var(--text-tertiary);
}
.vsh-save {
  flex: none;
  height: 28px;
  padding: 0 14px;
  font-size: 12.5px;
  color: var(--text-on-brand, #fff);
  background: var(--color-brand);
  border: none;
  border-radius: 6px;
  cursor: pointer;
}
.vsh-save:disabled {
  opacity: 0.5;
  cursor: default;
}
.vsh-mode {
  display: flex;
  align-items: center;
  gap: 8px;
  padding-bottom: 12px;
  margin-bottom: 12px;
  border-bottom: 1px solid var(--border-default);
}
.vsh-mode-btn {
  height: 26px;
  padding: 0 12px;
  font-size: 12px;
  color: var(--text-secondary);
  background: var(--surface-elevated);
  border: 1px solid var(--border-default);
  border-radius: 6px;
  cursor: pointer;
}
.vsh-mode-btn.active {
  color: var(--color-brand);
  border-color: var(--color-brand);
}
.vsh-mode-hint {
  margin-left: auto;
  font-size: 11.5px;
  color: var(--text-tertiary);
}
.vsh-phases {
  display: flex;
  flex-direction: column;
  gap: 16px;
}
.vsh-phase-hdr {
  display: flex;
  align-items: baseline;
  gap: 8px;
  margin-bottom: 6px;
}
.vsh-phase-title {
  font-size: 12.5px;
  font-weight: 600;
  color: var(--text-primary);
}
.vsh-phase-desc {
  font-size: 11.5px;
  color: var(--text-tertiary);
}
.vsh-run {
  margin-left: auto;
  display: inline-flex;
  align-items: center;
  gap: 4px;
  height: 24px;
  padding: 0 10px;
  font-size: 11.5px;
  color: var(--text-secondary);
  background: var(--surface-elevated);
  border: 1px solid var(--border-default);
  border-radius: 6px;
  cursor: pointer;
}
.vsh-progress {
  margin-bottom: 6px;
  padding: 6px 8px;
  font-size: 11.5px;
  color: var(--color-brand);
  background: var(--surface-elevated);
  border-radius: 6px;
}
.vsh-empty {
  font-size: 11.5px;
  color: var(--text-tertiary);
  padding: 4px 0;
}
.vsh-step {
  padding: 8px 10px;
  margin-bottom: 6px;
  background: var(--surface-elevated);
  border: 1px solid var(--border-default);
  border-radius: 6px;
}
.vsh-step.readonly {
  display: flex;
  align-items: center;
  gap: 8px;
}
.vsh-step-hdr {
  display: flex;
  align-items: center;
  gap: 8px;
}
.vsh-step-enable {
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
  cursor: pointer;
}
.vsh-step-kind {
  flex: none;
  font-size: 10.5px;
  padding: 1px 6px;
  border-radius: 4px;
  border: 1px solid var(--border-default);
  color: var(--text-tertiary);
}
.vsh-step-kind.kind-command {
  color: var(--text-secondary);
}
.vsh-step-label {
  font-size: 12px;
  color: var(--text-primary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.vsh-step-off {
  font-size: 11px;
  color: var(--text-tertiary);
}
.vsh-del {
  margin-left: auto;
  flex: none;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 24px;
  height: 24px;
  color: var(--text-tertiary);
  background: transparent;
  border: none;
  border-radius: 6px;
  cursor: pointer;
}
.vsh-del:hover {
  color: var(--error, #e5484d);
}
.vsh-edit {
  display: flex;
  align-items: center;
  gap: 10px;
  margin-top: 8px;
}
.vsh-edit.vsh-backup {
  flex-direction: column;
  align-items: stretch;
  gap: 6px;
}
.vsh-input {
  flex: 1;
  min-width: 0;
  height: 28px;
  padding: 0 8px;
  font-size: 12px;
  color: var(--text-primary);
  background: var(--surface-card);
  border: 1px solid var(--border-default);
  border-radius: 6px;
  outline: none;
}
.vsh-input:focus {
  border-color: var(--color-brand);
}
.vsh-timeout {
  flex: none;
  display: inline-flex;
  align-items: center;
  gap: 4px;
  font-size: 11.5px;
  color: var(--text-tertiary);
}
.vsh-timeout input {
  width: 56px;
  height: 24px;
  padding: 0 4px;
  font-size: 11.5px;
  color: var(--text-primary);
  background: var(--surface-card);
  border: 1px solid var(--border-default);
  border-radius: 4px;
}
.vsh-backup-row {
  display: flex;
  align-items: center;
  gap: 12px;
}
.vsh-radio {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  font-size: 11.5px;
  color: var(--text-secondary);
  cursor: pointer;
}
.vsh-worlds {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  padding: 6px 8px;
  background: var(--surface-card);
  border-radius: 6px;
}
.vsh-world {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  font-size: 11.5px;
  color: var(--text-secondary);
  cursor: pointer;
}
.vsh-add {
  position: relative;
}
.vsh-add-btn {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  height: 24px;
  padding: 0 10px;
  font-size: 11.5px;
  color: var(--text-secondary);
  background: transparent;
  border: 1px dashed var(--border-default);
  border-radius: 6px;
  cursor: pointer;
}
.vsh-add-menu {
  position: absolute;
  z-index: 5;
  top: 28px;
  left: 0;
  display: flex;
  flex-direction: column;
  min-width: 160px;
  padding: 4px;
  background: var(--surface-card);
  border: 1px solid var(--border-default);
  border-radius: 6px;
  box-shadow: 0 6px 20px rgba(0, 0, 0, 0.18);
}
.vsh-add-item {
  padding: 6px 8px;
  font-size: 12px;
  text-align: left;
  color: var(--text-primary);
  background: transparent;
  border: none;
  border-radius: 4px;
  cursor: pointer;
}
.vsh-add-item:hover {
  background: var(--surface-elevated);
}
.vsh-prefailure {
  display: flex;
  align-items: center;
  gap: 12px;
  padding-top: 12px;
  border-top: 1px solid var(--border-default);
}
.vsh-prefailure-label {
  font-size: 12px;
  color: var(--text-secondary);
}
.vsh-note {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 11.5px;
  color: var(--text-tertiary);
}
</style>
