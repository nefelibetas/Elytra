import { createApp } from "vue";
import App from "./App.vue";
import router from "./router";
import { applyTheme } from "./store/app/theme";
import tooltip from "./utils/tooltip";
import "./styles/transitions.css";

// 主题在挂载前应用，避免首帧闪白（暗色模式必须）
applyTheme();

const app = createApp(App);
// 全局 v-tooltip 指令（替换 UA title tooltip）
app.directive("tooltip", tooltip);
app.use(router).mount("#app");
