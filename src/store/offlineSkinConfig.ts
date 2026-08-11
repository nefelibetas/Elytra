// 离线皮肤配置 store（对齐 PCL2 五种）：类型选择/正版玩家皮肤/自定义文件。
import { ref, computed } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";
import { showToast } from "../utils/toast";
import { SKIN_TYPE_OPTIONS, type OfflineSkinType, type OfflineSkinModel } from "../utils/offlineSkin";
import { useConfigSection } from "../utils/useConfigSection";

const section = useConfigSection();
export const skinError = section.error;

export const skinOptions = SKIN_TYPE_OPTIONS;

export const skinType = ref<OfflineSkinType>("random");
export const skinPlayerName = ref("");
export const skinCustomPath = ref("");
export const skinCustomModel = ref<OfflineSkinModel>("steve");

/** 模型选择 dialog（选完皮肤文件后弹出）。 */
export const skinModelDialog = ref(false);

export async function loadOfflineSkin() {
  try {
    const cfg = await invoke<{
      skin_type: OfflineSkinType;
      player_name: string;
      custom_path: string;
      custom_model: OfflineSkinModel;
    }>("get_offline_skin");
    skinType.value = cfg.skin_type;
    skinPlayerName.value = cfg.player_name;
    skinCustomPath.value = cfg.custom_path;
    skinCustomModel.value = cfg.custom_model;
  } catch {
    // 无配置时保持默认
  }
}

export async function saveOfflineSkin() {
  await section.run(async () => {
    await invoke("set_offline_skin", {
      config: {
        skin_type: skinType.value,
        player_name: skinPlayerName.value.trim(),
        custom_path: skinCustomPath.value,
        custom_model: skinCustomModel.value,
      },
    });
  });
}

/** 当前选中选项的详细描述（选中后才展示）。 */
export const currentSkinDesc = computed(
  () => skinOptions.find(o => o.value === skinType.value)?.desc ?? "",
);

/** 点击皮肤类型胶囊：custom 且未选择文件时自动打开文件选择器。 */
export function onSkinTypeChange() {
  if (skinType.value === "custom" && !skinCustomPath.value) {
    saveOfflineSkin();
    pickCustomSkin();
    return;
  }
  saveOfflineSkin();
}

/** 打开文件选择器；选完后弹出「选择皮肤种类」模型 dialog。 */
export async function pickCustomSkin() {
  const path = await open({
    multiple: false,
    filters: [{ name: "PNG 图片", extensions: ["png"] }],
  });
  if (!path) return;
  skinCustomPath.value = path;
  skinModelDialog.value = true;
}

/** 选择模型（Steve / Alex / 启动器决定）→ 保存并关闭。 */
export function pickModel(model: OfflineSkinModel) {
  skinCustomModel.value = model;
  skinModelDialog.value = false;
  saveOfflineSkin();
}

/** 重置自定义皮肤：清空文件选择，自动切回随机。 */
export function resetCustomSkin() {
  skinCustomPath.value = "";
  skinType.value = "random";
  skinCustomModel.value = "steve";
  saveOfflineSkin();
}

/** 保存正版玩家的皮肤到用户指定位置。 */
export async function savePlayerSkin() {
  const name = skinPlayerName.value.trim();
  if (!name) {
    showToast("请先填写正版玩家用户名");
    return;
  }
  const path = await save({
    defaultPath: `${name}.png`,
    filters: [{ name: "PNG 图片", extensions: ["png"] }],
  });
  if (!path) return;
  await section.run(async () => {
    await invoke("save_player_skin", { playerName: name, path });
    showToast(`皮肤已保存：${name}`);
  });
}

/** 刷新：重新校验正版玩家皮肤信息。 */
export async function refreshPlayerSkin() {
  const name = skinPlayerName.value.trim();
  if (!name) {
    showToast("请先填写正版玩家用户名");
    return;
  }
  await section.run(async () => {
    const info = await invoke<{ uuid: string; texture_id: string }>(
      "fetch_player_skin",
      { playerName: name },
    );
    showToast(`皮肤信息已刷新：${name}`);
    void info;
  });
}
