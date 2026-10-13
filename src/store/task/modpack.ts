// 整合包安装入口（薄封装 install_modpack）：资源中心整合包页与版本选择页游戏
// 目录菜单的「导入本地」共用。后端按来源分流——local 直读用户选定文件，
// online 走多源下载并缓存到 config_dir()/modpacks。
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { errMsg } from "../../utils/error";
import { showToast } from "../../utils/toast";
import { registerTaskName } from "./download";

/** 整合包来源（镜像 Rust `game::modpack::PackSource`，serde tag = "type"）。 */
export type ModpackSource =
  | { type: "local"; path: string }
  | { type: "online"; url: string; sha1?: string; size?: number };

/** 安装请求（镜像 Rust `game::modpack::ModpackRequest`）。 */
export interface ModpackInstallRequest {
  name: string;
  source: ModpackSource;
  /** 平台项目图标 URL（在线安装透传；本地导入无平台元数据） */
  icon_url?: string | null;
}

/** 提交整合包安装任务（解析 → 游戏本体 → 内容），返回任务 id。 */
export async function installModpack(request: ModpackInstallRequest): Promise<string> {
  const taskId = await invoke<string>("install_modpack", { request });
  registerTaskName(taskId, request.name);
  return taskId;
}

/** 本地导入：选 .mrpack/.zip → 提交安装（装入当前激活游戏目录）。
 *  返回包名；用户取消或提交失败返回 null。 */
export async function importLocalModpack(): Promise<string | null> {
  try {
    const path = await open({
      multiple: false,
      filters: [{ name: "整合包", extensions: ["mrpack", "zip"] }],
    });
    if (!path) return null;
    // 展示名取文件名去扩展名（后端计划/任务卡均用该名称）
    const fileName = String(path).split(/[\\/]/).pop() ?? "整合包";
    const name = fileName.replace(/\.(mrpack|zip)$/i, "") || "整合包";
    await installModpack({ name, source: { type: "local", path: String(path) } });
    showToast("已提交整合包安装（准备 → 游戏本体 → 内容）");
    return name;
  } catch (e) {
    showToast(errMsg(e));
    return null;
  }
}
