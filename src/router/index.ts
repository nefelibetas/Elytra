import { createRouter, createWebHashHistory } from "vue-router";
import Launch from "../pages/Launch.vue";
import Download from "../pages/Download.vue";
import Settings from "../pages/Settings.vue";
import DownloadDetail from "../pages/DownloadDetail.vue";
import VersionSelect from "../pages/VersionSelect.vue";
import VersionSetup from "../pages/VersionSetup.vue";

const routes = [
  { path: "/", name: "launch", component: Launch },
  { path: "/download", name: "download", component: Download },
  { path: "/settings", name: "settings", component: Settings },
  { path: "/download-detail", name: "download-detail", component: DownloadDetail },
  { path: "/version-select", name: "version-select", component: VersionSelect },
  { path: "/version-setup", name: "version-setup", component: VersionSetup },
];

const router = createRouter({
  history: createWebHashHistory(),
  routes,
});

const tabOrder = ["/", "/download", "/settings"];

router.beforeEach((to, from) => {
  const meta = to.meta as Record<string, unknown>;
  // 版本选择/版本设置：进入/退出均带滑入动画
  if (to.path === "/version-select" || to.path === "/version-setup") {
    meta.transition = "slide-right";
    return;
  }
  if (from.path === "/version-select" || from.path === "/version-setup") {
    meta.transition = "slide-left";
    return;
  }
  if (from.path === "/download-detail" && to.path !== "/download-detail") {
    meta.transition = "slide-left";
    return;
  }
  const fi = tabOrder.indexOf(from.path);
  const ti = tabOrder.indexOf(to.path);
  if (fi >= 0 && ti >= 0) {
    meta.transition = ti > fi ? "slide-left" : "slide-right";
    return;
  }
  if (to.path === "/download-detail") {
    meta.transition = "slide-right";
    return;
  }
  meta.transition = "route";
});

export default router;
