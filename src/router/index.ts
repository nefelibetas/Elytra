import { createRouter, createWebHashHistory } from "vue-router";
import Launch from "../pages/Launch.vue";
import Download from "../pages/Download.vue";
import Settings from "../pages/Settings.vue";

const routes = [
  { path: "/", name: "launch", component: Launch },
  { path: "/download", name: "download", component: Download },
  { path: "/settings", name: "settings", component: Settings },
];

const router = createRouter({
  history: createWebHashHistory(),
  routes,
});

export default router;
