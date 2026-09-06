<script setup lang="ts">
// 种子查看器窗口（label=seed-map）：预测/种子功能从地图编辑器剥离后的独立住所。
// 无存档依赖：输入种子直开 26.x 参数族主世界群系+水陆预测，任意坐标漫游。
// 与地图编辑器同画布组件（MapCanvas seedMode），编辑器本体回归纯真实存档地图。
import { computed, onMounted, ref } from "vue";
import { Map, Minus, X } from "@lucide/vue";
import { getCurrentWindow } from "@tauri-apps/api/window";
import MapCanvas from "../component/map/MapCanvas.vue";
import { parseSeedText } from "../utils/seed";

const appWindow = getCurrentWindow();
function minimize() { void appWindow.minimize(); }
function closeWindow() { void appWindow.close(); }
function onTitleBarMouseDown(e: MouseEvent) {
	const target = e.target as HTMLElement;
	if (target.closest("a, button")) return;
	void appWindow.startDragging();
}

/** 种子输入框 / 已应用种子（十进制字符串）。 */
const seedInput = ref("");
const seedView = ref<string | null>(null);
const topdownStyle = "topdown";
const seedReady = computed(() => seedView.value !== null);

function openSeedMap() {
	const seed = parseSeedText(seedInput.value);
	if (seed === null) return;
	seedView.value = seed;
	void appWindow.setTitle(`种子 ${seed} · 种子查看器`);
}

onMounted(() => {
	// 深链直开：seed-map?seed=…（输入框预填并自动开图）
	const query = new URLSearchParams(location.hash.split("?")[1] ?? "");
	const seed = query.get("seed") ?? "";
	if (seed) {
		seedInput.value = seed;
		openSeedMap();
	}
});
</script>

<template>
	<div class="sv">
		<!-- 自绘标题栏 -->
		<div class="sv-titlebar" @mousedown="onTitleBarMouseDown">
			<Map :size="17" class="sv-tb-icon" />
			<span class="sv-tb-title">种子查看器</span>
			<div class="sv-tb-actions">
				<button class="sv-tb-btn" title="最小化" @click="minimize"><Minus :size="16" /></button>
				<button class="sv-tb-btn sv-tb-close" title="关闭" @click="closeWindow"><X :size="16" /></button>
			</div>
		</div>

		<div class="sv-body">
			<div class="sv-toolbar">
				<input
					v-model="seedInput"
					class="sv-seed-input"
					placeholder="种子：数字（支持超 2^53）或任意字符串"
					spellcheck="false"
					@keyup.enter="openSeedMap"
				/>
				<button class="sv-seed-open" :disabled="!seedInput.trim()" @click="openSeedMap">
					查看
				</button>
				<span class="sv-note">26.x 参数族 · 主世界表层 · 群系 + 水陆预测</span>
			</div>

			<div v-if="seedReady" class="sv-map-holder">
				<MapCanvas
					world-path=""
					dimension=""
					:style="topdownStyle"
					:seed-mode="true"
					:seed="seedView ?? ''"
					:init-scale="0.25"
				/>
			</div>
			<div v-else class="sv-empty">
				<Map :size="44" :stroke-width="1.2" opacity="0.35" />
				<p>输入种子开始查看未生成世界的地图</p>
				<p class="sv-empty-sub">预测瓦片缓存于本机，漫游即取；真实存档地图请用「地图编辑器」</p>
			</div>
		</div>
	</div>
</template>

<style scoped>
.sv {
	display: flex;
	flex-direction: column;
	height: 100vh;
	background-color: var(--surface-page);
	color: var(--text-primary);
	font-size: 13px;
	overflow: hidden;
}

.sv-titlebar {
	display: flex;
	align-items: center;
	gap: 10px;
	height: 40px;
	padding: 0 6px 0 14px;
	background-color: var(--surface-card);
	border-bottom: 1px solid var(--surface-border, rgba(128, 128, 128, 0.25));
	user-select: none;
	flex-shrink: 0;
}

.sv-tb-icon { color: var(--color-brand); }
.sv-tb-title { font-size: 14px; font-weight: 700; flex: 1; }
.sv-tb-actions { display: flex; gap: 4px; }

.sv-tb-btn {
	display: flex;
	align-items: center;
	justify-content: center;
	width: 30px;
	height: 26px;
	border: none;
	border-radius: 6px;
	background: transparent;
	color: var(--text-secondary);
	cursor: pointer;
}

.sv-tb-btn:hover { background: var(--surface-hover); color: var(--text-primary); }
.sv-tb-close:hover { background: rgba(231, 76, 60, 0.85); color: #fff; }

.sv-body { flex: 1; display: flex; flex-direction: column; min-height: 0; }

.sv-toolbar {
	display: flex;
	align-items: center;
	gap: 10px;
	padding: 10px 14px;
	border-bottom: 1px solid var(--surface-border, rgba(128, 128, 128, 0.25));
	flex-shrink: 0;
}

.sv-seed-input {
	flex: 1;
	max-width: 460px;
	box-sizing: border-box;
	padding: 7px 10px;
	border-radius: 6px;
	font-size: 13px;
	background: var(--surface-input, var(--surface-card));
	border: 1px solid var(--surface-border, rgba(128, 128, 128, 0.35));
	color: var(--text-primary);
	outline: none;
}

.sv-seed-input:focus { border-color: var(--color-brand); }

.sv-seed-open {
	padding: 7px 18px;
	border-radius: 6px;
	font-size: 13px;
	border: 1px solid var(--surface-border, rgba(128, 128, 128, 0.35));
	background: color-mix(in srgb, var(--color-brand) 14%, transparent);
	color: var(--color-brand);
	cursor: pointer;
}

.sv-seed-open:disabled { opacity: 0.45; cursor: default; }

.sv-note { margin-left: auto; font-size: 12px; color: var(--text-tertiary); }

.sv-map-holder { flex: 1; position: relative; min-height: 0; }

.sv-empty {
	flex: 1;
	display: flex;
	flex-direction: column;
	align-items: center;
	justify-content: center;
	gap: 8px;
	color: var(--text-secondary);
}

.sv-empty-sub { font-size: 11px; color: var(--text-tertiary); }
</style>
