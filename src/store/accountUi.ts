// 账号管理 UI store：编辑信息菜单 / 皮肤披风菜单 / 皮肤操作 / 披风选择 dialog。
// 依赖 auth（账号）+ avatar（头像刷新）+ alert（错误弹窗）+ tauri dialog/opener 插件。
import { nextTick, reactive } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import { open, save } from "@tauri-apps/plugin-dialog";
import { msAccount } from "./auth";
import { renderPremiumAvatar } from "./avatar";
import { showError } from "./alert";
import { showToast } from "../utils/toast";

export interface CapeInfo {
  id: string;
  alias: string;
  url: string;
  active: boolean;
}

const MENU_W = 176;
const MENU_H = 224;
const EDIT_MENU_H = 96;

/** 确保披风列表只预取一次（失败允许下次重试） */
let capesFetched = false;

export const accountUi = reactive({
  // 编辑信息菜单（修改密码 / 修改玩家名 → 官网）
  editMenu: { visible: false, x: 0, y: 0 },
  // 皮肤/披风菜单
  skinMenu: { visible: false, x: 0, y: 0 },
  // 账号列表菜单（正版历史切换）
  accountsMenu: { visible: false, x: 0, y: 0 },
  // 离线历史用户名展开面板（锚定输入框下方，非跟随鼠标）
  offlineMenu: { visible: false },
  /** 账号切换进行中（防止连点） */
  accountsBusy: false,

  // 披风选择 dialog
  capes: [] as CapeInfo[],
  capeLoading: false,
  capeDialogVisible: false,
  /** 当前选中披风 id，"" = 无披风 */
  selectedCapeId: "",

  // 皮肤模型选择 dialog（上传前确认 CLASSIC/SLIM）
  skinModelDialog: false,
  pendingSkinPath: "",

  openEditMenu(e: MouseEvent) {
    if (this.editMenu.visible) {
      this.closeEditMenu();
      return;
    }
    this.closeOtherMenus("editMenu");
    this.editMenu = {
      visible: true,
      x: Math.min(e.clientX + 6, window.innerWidth - MENU_W - 6),
      y: Math.min(e.clientY + 6, window.innerHeight - EDIT_MENU_H - 6),
    };
  },

  closeEditMenu() {
    this.editMenu.visible = false;
  },

  /** 修改密码：微软账号官方改密页。 */
  openPasswordPage() {
    this.closeEditMenu();
    openUrl("https://account.live.com/password/change");
  },

  /** 修改玩家名：Minecraft 官方角色管理页（含改名入口）。 */
  openNameChangePage() {
    this.closeEditMenu();
    openUrl("https://www.minecraft.net/msa");
  },

  /** 点击衣服图标：菜单出现在鼠标右下方（toggle；靠近窗口边缘时内收）。
   * 同时后台预取披风列表，保证打开「修改披风」时数据已就绪。 */
  openSkinMenu(e: MouseEvent) {
    if (this.skinMenu.visible) {
      this.closeSkinMenu();
      return;
    }
    this.closeOtherMenus("skinMenu");
    this.skinMenu = {
      visible: true,
      x: Math.min(e.clientX + 6, window.innerWidth - MENU_W - 6),
      y: Math.min(e.clientY + 6, window.innerHeight - MENU_H - 6),
    };
    this.ensureCapes();
  },

  closeSkinMenu() {
    this.skinMenu.visible = false;
  },

  /** 账号列表菜单（toggle；跟随鼠标，渲染后按实际尺寸校准不超视口）。 */
  openAccountsMenu(e: MouseEvent) {
    if (this.accountsMenu.visible) {
      this.closeAccountsMenu();
      return;
    }
    this.closeOtherMenus("accountsMenu");
    this.accountsMenu = {
      visible: true,
      x: e.clientX + 6,
      y: e.clientY + 6,
    };
    void this.clampAfterTick();
  },

  closeAccountsMenu() {
    this.accountsMenu.visible = false;
  },

  /** 离线历史用户名面板（toggle；锚定输入框下方展开，无需鼠标定位）。 */
  openOfflineMenu() {
    if (this.offlineMenu.visible) {
      this.closeOfflineMenu();
      return;
    }
    this.closeOtherMenus("offlineMenu");
    this.offlineMenu.visible = true;
  },

  closeOfflineMenu() {
    this.offlineMenu.visible = false;
  },

  /** 渲染后按实际尺寸校准菜单位置（内容量可变，估算会溢出/偏差；
   * nextTick 后 Teleport 菜单已挂载，用 getBoundingClientRect 内收）。 */
  async clampAfterTick() {
    await nextTick();
    if (!this.accountsMenu.visible) return;
    const el = document.querySelector(".acct-menu.accounts");
    if (!el) return;
    const r = el.getBoundingClientRect();
    if (r.right > window.innerWidth - 6) this.accountsMenu.x = Math.max(0, window.innerWidth - r.width - 6);
    if (r.bottom > window.innerHeight - 6) this.accountsMenu.y = Math.max(0, window.innerHeight - r.height - 6);
  },

  /** 互斥：关闭除 self 外的全部菜单（按钮 @click.stop 阻止冒泡，window 收不到，
   * 开新菜单时必须显式关旧菜单，否则叠加出现两个菜单）。 */
  closeOtherMenus(self: "skinMenu" | "editMenu" | "accountsMenu" | "offlineMenu") {
    if (self !== "skinMenu") this.closeSkinMenu();
    if (self !== "editMenu") this.closeEditMenu();
    if (self !== "accountsMenu") this.closeAccountsMenu();
    if (self !== "offlineMenu") this.closeOfflineMenu();
  },

  /** window click 统一关闭菜单（组件 onMounted 注册监听）。 */
  closeMenus() {
    if (this.skinMenu.visible) this.closeSkinMenu();
    if (this.editMenu.visible) this.closeEditMenu();
    if (this.accountsMenu.visible) this.closeAccountsMenu();
    if (this.offlineMenu.visible) this.closeOfflineMenu();
  },

  /** 1. 修改皮肤：本地选择 PNG → 上传官方账号 → 刷新头像。 */
  async uploadSkin() {
    this.closeSkinMenu();
    const acct = msAccount.value;
    if (!acct) return;
    const path = await open({
      multiple: false,
      filters: [{ name: "PNG 图片", extensions: ["png"] }],
    });
    if (!path) return;
    this.pendingSkinPath = path;
    this.skinModelDialog = true;
  },

  /** 1b. 确认模型后上传（CLASSIC = 经典 Steve 宽模型 / SLIM = 纤细 Alex 窄模型）。 */
  async confirmUploadSkin(variant: "CLASSIC" | "SLIM") {
    this.skinModelDialog = false;
    const acct = msAccount.value;
    if (!acct || !this.pendingSkinPath) return;
    const path = this.pendingSkinPath;
    this.pendingSkinPath = "";
    try {
      await invoke("upload_skin_file", {
        accessToken: acct.access_token,
        filePath: path,
        variant,
      });
      showToast("皮肤上传成功，正在刷新头像…");
      // 等 Mojang 处理生效后按新 texture ID 渲染
      await new Promise(r => setTimeout(r, 3000));
      await renderPremiumAvatar(acct.uuid, true); // 皮肤已变，强制重新拉取
    } catch (e) {
      showError(`皮肤上传失败：${String(e)}`, true);
    }
  },

  /** 2. 刷新皮肤：重新获取当前皮肤并更新头像。 */
  async refreshSkin() {
    this.closeSkinMenu();
    await renderPremiumAvatar(msAccount.value?.uuid ?? "", true); // 主动刷新，忽略缓存
    showToast("皮肤已刷新");
  },

  /** 3. 保存皮肤文件：文件选择器选目标位置，下载当前皮肤写入。 */
  async saveSkin() {
    this.closeSkinMenu();
    const acct = msAccount.value;
    if (!acct) return;
    const path = await save({
      defaultPath: `${acct.name}.png`,
      filters: [{ name: "PNG 图片", extensions: ["png"] }],
    });
    if (!path) return;
    try {
      await invoke("save_skin", { uuid: acct.uuid, path });
      showToast("皮肤已保存");
    } catch (e) {
      showError(`保存皮肤失败：${String(e)}`, true);
    }
  },

  /** 拉取官方披风列表，返回是否成功。失败时保留旧缓存（下次可重试）。 */
  async loadCapes(): Promise<boolean> {
    const acct = msAccount.value;
    if (!acct) return false;
    this.capeLoading = true;
    try {
      this.capes = await invoke<CapeInfo[]>("fetch_capes", {
        accessToken: acct.access_token,
      });
      return true;
    } catch (e) {
      showError(`获取披风列表失败：${String(e)}`, true);
      return false;
    } finally {
      this.capeLoading = false;
    }
  },

  /** 确保列表已加载过：菜单打开时后台预取，打开 dialog 时数据已就绪、无抖动。 */
  async ensureCapes() {
    if (capesFetched) return;
    if (await this.loadCapes()) capesFetched = true;
  },

  /** 5. 修改披风：打开选择 dialog（列表已预取，默认选中当前披风）。 */
  async openCapeDialog() {
    this.closeSkinMenu();
    await this.ensureCapes();
    this.capeDialogVisible = true;
    this.selectedCapeId = this.capes.find(c => c.active)?.id ?? "";
  },

  closeCapeDialog() {
    this.capeDialogVisible = false;
  },

  /** 确定：无披风 → 移除；否则切换。成功后 toast 提示。 */
  async confirmCape() {
    const acct = msAccount.value;
    if (!acct) return;
    const cape = this.capes.find(c => c.id === this.selectedCapeId);
    try {
      if (cape) {
        await invoke("set_active_cape", {
          accessToken: acct.access_token,
          capeId: cape.id,
        });
        showToast(`已切换到披风「${cape.alias || cape.id.slice(0, 8)}」`);
      } else {
        await invoke("disable_cape", { accessToken: acct.access_token });
        showToast("已移除披风");
      }
      this.closeCapeDialog();
      await this.loadCapes(); // 刷新缓存
    } catch (e) {
      showError(`切换披风失败：${String(e)}`, true);
    }
  },

  /** 6. 刷新披风列表：重新拉取官方列表。 */
  async refreshCape() {
    this.closeSkinMenu();
    await this.loadCapes();
    if (!this.capeLoading) {
      showToast(`披风列表已刷新（${this.capes.length} 个可用）`);
    }
  },
});
