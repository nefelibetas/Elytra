// 账号管理 UI store：编辑信息菜单（修改密码/修改玩家名）+ 账号列表/离线历史菜单。
// 皮肤与披风功能已内移到 store/skinStudio（皮肤衣柜页）。
// 依赖 tauri opener 插件（菜单项跳官网）。
import { nextTick, reactive } from "vue";
import { openUrl } from "@tauri-apps/plugin-opener";

const MENU_W = 176;
const EDIT_MENU_H = 96;

export const accountUi = reactive({
  // 编辑信息菜单（修改密码 / 修改玩家名 → 官网）
  editMenu: { visible: false, x: 0, y: 0 },
  // 账号列表菜单（正版历史切换）
  accountsMenu: { visible: false, x: 0, y: 0 },
  // 离线历史用户名展开面板（锚定输入框下方，非跟随鼠标）
  offlineMenu: { visible: false },
  /** 账号切换进行中（防止连点） */
  accountsBusy: false,

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
  closeOtherMenus(self: "editMenu" | "accountsMenu" | "offlineMenu") {
    if (self !== "editMenu") this.closeEditMenu();
    if (self !== "accountsMenu") this.closeAccountsMenu();
    if (self !== "offlineMenu") this.closeOfflineMenu();
  },

  /** window click 统一关闭菜单（组件 onMounted 注册监听）。 */
  closeMenus() {
    if (this.editMenu.visible) this.closeEditMenu();
    if (this.accountsMenu.visible) this.closeAccountsMenu();
    if (this.offlineMenu.visible) this.closeOfflineMenu();
  },
});
