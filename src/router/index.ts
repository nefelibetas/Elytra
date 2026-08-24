import { createRouter, createWebHashHistory } from "vue-router";
// 页面懒加载：按路由代码分割 + 解除 router→页面→store→router 的静态环
const Launch = () => import("../pages/Launch.vue");
const Download = () => import("../pages/Download.vue");
const Settings = () => import("../pages/Settings.vue");
const DownloadDetail = () => import("../pages/DownloadDetail.vue");
const ResourceDetail = () => import("../pages/ResourceDetail.vue");
const VersionSelect = () => import("../pages/VersionSelect.vue");
const VersionSetup = () => import("../pages/VersionSetup.vue");

declare module "vue-router" {
  interface RouteMeta {
    /** 页面切换动画名（slide-left / slide-right / route） */
    transition?: string;
  }
}

/** 路由路径常量（store/组件跳转共用，避免散落重复字符串）。 */
export const ROUTE_DOWNLOAD = "/download";
export const ROUTE_RESOURCE_DETAIL = "/resource-detail";

const routes = [
  { path: "/", name: "launch", component: Launch },
  { path: ROUTE_DOWNLOAD, name: "download", component: Download },
  { path: "/settings", name: "settings", component: Settings },
  { path: "/download-detail", name: "download-detail", component: DownloadDetail },
  { path: ROUTE_RESOURCE_DETAIL, name: "resource-detail", component: ResourceDetail },
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
  "resource-detail": 1,
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
