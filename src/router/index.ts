import { createRouter, createWebHashHistory } from "vue-router";
import Launch from "../pages/Launch.vue";
import Download from "../pages/Download.vue";
import Settings from "../pages/Settings.vue";
import DownloadDetail from "../pages/DownloadDetail.vue";
import ModDetail from "../pages/ModDetail.vue";
import VersionSelect from "../pages/VersionSelect.vue";
import VersionSetup from "../pages/VersionSetup.vue";

declare module "vue-router" {
  interface RouteMeta {
    /** 页面切换动画名（slide-left / slide-right / route） */
    transition?: string;
  }
}

const routes = [
  { path: "/", name: "launch", component: Launch },
  { path: "/download", name: "download", component: Download },
  { path: "/settings", name: "settings", component: Settings },
  { path: "/download-detail", name: "download-detail", component: DownloadDetail },
  { path: "/mod-detail", name: "mod-detail", component: ModDetail },
  { path: "/version-select", name: "version-select", component: VersionSelect },
  { path: "/version-setup", name: "version-setup", component: VersionSetup },
];

const router = createRouter({
  history: createWebHashHistory(),
  routes,
});

/** 页面层级：0 = 顶级 tab，1 = 一级详情，2 = 二级详情（动画方向按层级进出）。 */
const routeLevel: Record<string, number> = {
  launch: 0,
  download: 0,
  settings: 0,
  "download-detail": 0,
  "mod-detail": 1,
  "version-select": 1,
  "version-setup": 2,
};

/** 顶级 tab 顺序（同级切换按此决定左右方向）。 */
const tabOrder = ["launch", "download", "download-detail", "settings"];

router.beforeEach((to, from) => {
  const toLevel = routeLevel[to.name as string] ?? 0;
  const fromLevel = routeLevel[from.name as string] ?? 0;
  if (toLevel > fromLevel) {
    to.meta.transition = "slide-right"; // 进入更深层级
  } else if (toLevel < fromLevel) {
    to.meta.transition = "slide-left"; // 返回上级
  } else {
    // 同级：tab 顺序（右移左滑，左移右滑）
    const fi = tabOrder.indexOf(from.name as string);
    const ti = tabOrder.indexOf(to.name as string);
    to.meta.transition = fi >= 0 && ti >= 0 && ti > fi ? "slide-left" : "slide-right";
  }
});

export default router;
