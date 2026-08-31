import { createRouter, createWebHashHistory } from "vue-router";
// 顶级 Tab（启动/资源中心/任务中心/设置）静态导入：首屏即加载、切换零延迟、
// 导航高亮稳定；其 store 依赖链不含 router（无循环）。
// 深层页面（资源详情/版本选择/版本设置）经 store → router 依赖，保留懒加载
// 以解除 router→页面→store→router 的静态环。
import Launch from "../pages/Launch.vue";
import Download from "../pages/Download.vue";
import Settings from "../pages/Settings.vue";
import Tools from "../pages/Tools.vue";
import DownloadDetail from "../pages/DownloadDetail.vue";
const ResourceDetail = () => import("../pages/ResourceDetail.vue");
const VersionSelect = () => import("../pages/VersionSelect.vue");
const VersionSetup = () => import("../pages/VersionSetup.vue");
const CustomThemeEditor = () => import("../component/CustomThemeEditor.vue");

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
  { path: "/tools", name: "tools", component: Tools },
  { path: "/download-detail", name: "download-detail", component: DownloadDetail },
  { path: ROUTE_RESOURCE_DETAIL, name: "resource-detail", component: ResourceDetail },
  { path: "/version-select", name: "version-select", component: VersionSelect },
  { path: "/version-setup", name: "version-setup", component: VersionSetup },
  { path: "/custom-theme", name: "custom-theme", component: CustomThemeEditor },
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
  tools: 0,
  "download-detail": 0,
  "resource-detail": 1,
  "version-select": 1,
  "custom-theme": 1,
  "version-setup": 2,
};

/** 顶级 tab 顺序（同级切换按此决定左右方向）。 */
const tabOrder = ["launch", "download", "download-detail", "tools", "settings"];

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
