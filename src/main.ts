import { createApp } from "vue";
import App from "./App.vue";
import { i18n, initI18n } from "./i18n";
import router from "./router";
import { applyTheme } from "./store/app/theme";
import tooltip from "./utils/tooltip";
import "./styles/transitions.css";

// 挂载前应用主题与语言，避免首帧闪白/闪中文
applyTheme();
initI18n();

const app = createApp(App);
// 全局 v-tooltip 指令（替换 UA title tooltip）
app.directive("tooltip", tooltip);
app.use(i18n);
app.use(router).mount("#app");
