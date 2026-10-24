<script setup lang="ts">
// 设置 → 工具：Grid 卡片列表；选中工具后在右侧原位展开，页内返回列表。
import type { Component } from "vue";
import { computed, defineAsyncComponent, onBeforeUnmount, onMounted, ref } from "vue";
import { useI18n } from "vue-i18n";
import {
  Server, Type, Sword, PawPrint, Terminal, Paintbrush, Download, Map, MapPin, ArrowLeft,
} from "@lucide/vue";
import ServerStatusTool from "../tools/ServerStatusTool.vue";
import MotdGeneratorTool from "../tools/MotdGeneratorTool.vue";
import GiveGeneratorTool from "../tools/GiveGeneratorTool.vue";
import EntityNbtTool from "../tools/EntityNbtTool.vue";
import CommandGeneratorTool from "../tools/CommandGeneratorTool.vue";
import SkinPainterTool from "../tools/SkinPainterTool.vue";
import DownloadTool from "../tools/DownloadTool.vue";

const SeedMapTool = defineAsyncComponent(() => import("../tools/SeedMapTool.vue"));
const MapFolderTool = defineAsyncComponent(() => import("../tools/MapFolderTool.vue"));

const { t } = useI18n();

const selected = ref<string | null>(null);

interface ToolCard {
  key: string;
  titleKey: string;
  descKey: string;
  icon: Component;
  component: Component;
}

const tools: ToolCard[] = [
  { key: "status", titleKey: "tools.tabs.status", descKey: "tools.status.subtitle", icon: Server, component: ServerStatusTool },
  { key: "motd", titleKey: "tools.tabs.motd", descKey: "tools.motd.subtitle", icon: Type, component: MotdGeneratorTool },
  { key: "give", titleKey: "tools.tabs.give", descKey: "tools.give.subtitle", icon: Sword, component: GiveGeneratorTool },
  { key: "entity", titleKey: "tools.tabs.entity", descKey: "tools.entity.subtitle", icon: PawPrint, component: EntityNbtTool },
  { key: "command", titleKey: "tools.tabs.command", descKey: "tools.command.subtitle", icon: Terminal, component: CommandGeneratorTool },
  { key: "skinpaint", titleKey: "tools.tabs.skinpaint", descKey: "tools.skinpainter.subtitle", icon: Paintbrush, component: SkinPainterTool },
  { key: "seedmap", titleKey: "tools.tabs.seedmap", descKey: "tools.seedmap.subtitle", icon: Map, component: SeedMapTool },
  { key: "worldpreview", titleKey: "tools.tabs.worldpreview", descKey: "tools.worldpreview.subtitle", icon: MapPin, component: MapFolderTool },
  { key: "download", titleKey: "tools.tabs.download", descKey: "tools.download.subtitle", icon: Download, component: DownloadTool },
];

const active = computed(() => tools.find(tool => tool.key === selected.value) ?? null);
const navDirection = ref<"forward" | "back">("forward");

function openTool(key: string) {
  navDirection.value = "forward";
  selected.value = key;
}

function backToList() {
  navDirection.value = "back";
  selected.value = null;
}

/** 工具详情内 ESC 回 Grid；有弹窗时让弹窗先处理。 */
function onEsc(e: KeyboardEvent) {
  if (!selected.value || e.key !== "Escape" || document.querySelector(".dialog-overlay")) return;
  e.stopPropagation();
  backToList();
}

onMounted(() => document.addEventListener("keydown", onEsc, true));
onBeforeUnmount(() => document.removeEventListener("keydown", onEsc, true));
</script>

<template>
  <div class="stg-tools">
    <Transition
      :name="navDirection === 'forward' ? 'tool-forward' : 'tool-back'"
      mode="out-in"
    >
      <div v-if="!active" key="list" class="stg-tools-list">
        <header class="stg-tools-head">
          <h2>{{ t("tools.tabs.title") }}</h2>
        </header>

        <div class="stg-tools-grid">
          <button
            v-for="tool in tools"
            :key="tool.key"
            type="button"
            class="stg-tool-card"
            @click="openTool(tool.key)"
          >
            <span class="stg-tool-icon">
              <component :is="tool.icon" :size="20" />
            </span>
            <span class="stg-tool-title">{{ t(tool.titleKey) }}</span>
            <span class="stg-tool-desc">{{ t(tool.descKey) }}</span>
          </button>
        </div>
      </div>

      <div v-else key="detail" class="stg-tool-detail">
        <button type="button" class="stg-tool-back" @click="backToList">
          <ArrowLeft :size="14" />
          <span>{{ t("settings.tools.back") }}</span>
        </button>
        <component :is="active.component" :key="active.key" />
      </div>
    </Transition>
  </div>
</template>

<style scoped>
.stg-tools {
  display: flex;
  flex-direction: column;
  gap: 16px;
  height: 100%;
  min-height: 0;
}

.stg-tools-list {
  display: flex;
  flex-direction: column;
  gap: 16px;
  flex: 1;
  min-height: 0;
}

.tool-forward-enter-active,
.tool-forward-leave-active,
.tool-back-enter-active,
.tool-back-leave-active {
  transition: opacity 0.22s ease, transform 0.22s cubic-bezier(0.2, 0.8, 0.2, 1);
}

.tool-forward-enter-from {
  opacity: 0;
  transform: translateX(24px) scale(0.985);
}

.tool-forward-leave-to {
  opacity: 0;
  transform: translateX(-16px) scale(0.99);
}

.tool-back-enter-from {
  opacity: 0;
  transform: translateX(-24px) scale(0.985);
}

.tool-back-leave-to {
  opacity: 0;
  transform: translateX(16px) scale(0.99);
}

.stg-tools-head h2 {
  margin: 0 0 4px;
  font-size: 17px;
  font-weight: 600;
  color: var(--text-primary);
}

.stg-tools-head p {
  margin: 0;
  font-size: 12.5px;
  color: var(--text-tertiary);
}

.stg-tools-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(200px, 1fr));
  gap: 12px;
  align-content: start;
}

.stg-tool-card {
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  gap: 6px;
  min-height: 100px;
  padding: 16px;
  text-align: left;
  background: var(--surface-elevated);
  border: 1px solid var(--border-default);
  border-radius: var(--radius-lg);
  cursor: pointer;
  transition: border-color 0.15s, transform 0.15s, box-shadow 0.15s;
}

.stg-tool-card:hover {
  border-color: var(--color-brand);
  transform: translateY(-1px);
  box-shadow: var(--shadow-md);
}

.stg-tool-icon {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 32px;
  height: 32px;
  margin-bottom: 2px;
  border-radius: var(--radius-sm);
  background: var(--color-brand-bg);
  color: var(--color-brand);
}

.stg-tool-title {
  font-size: 14px;
  font-weight: 600;
  color: var(--text-primary);
}

.stg-tool-desc {
  font-size: 12px;
  line-height: 1.5;
  color: var(--text-tertiary);
}

.stg-tool-detail {
  display: flex;
  flex-direction: column;
  gap: 12px;
  flex: 1;
  min-height: 0;
}

.stg-tool-back {
  position: sticky;
  top: 0;
  z-index: 2;
  align-self: flex-start;
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 4px 0;
  border: none;
  background: var(--surface-page);
  color: var(--text-secondary);
  font-size: 13px;
  cursor: pointer;
}

.stg-tool-back:hover {
  color: var(--color-brand);
}
</style>
