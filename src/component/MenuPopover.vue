<script setup lang="ts">
// 共享弹出菜单壳：Teleport 到 body + 视口 fixed 定位 + @click.stop 阻断冒泡
// （window click 统一关闭菜单的监听仍由使用方注册）。
// 壳样式与菜单项通用样式（.menu-item / .menu-sep）收敛于此，替换原先
// AccountSection（skin-menu 系）与 GameDirList（vs-item-menu 系）两份重复实现；
// 两处壳样式仅 min-width 不同（164px / 120px），经 prop 传入。
withDefaults(
  defineProps<{
    /** 开关状态。 */
    visible: boolean;
    /** 视口定位坐标（px）。 */
    x: number;
    y: number;
    /** 壳最小宽度。 */
    minWidth?: string;
    /** 内容区最大高度（有值时启用内部滚动，账号列表菜单用）。 */
    maxHeight?: string;
    /** 追加到壳上的额外类名（如 acct-menu accounts，供外部按类查询 DOM）。 */
    menuClass?: string;
  }>(),
  { minWidth: "164px", maxHeight: "", menuClass: "" },
);
</script>

<template>
  <Teleport to="body">
    <div
      v-if="visible"
      :class="['menu-pop', menuClass]"
      :style="{ left: x + 'px', top: y + 'px', minWidth, ...(maxHeight ? { maxHeight, overflowY: 'auto' as const } : {}) }"
      @click.stop
    >
      <!-- slot 内容属于使用方作用域，通用菜单项样式须经 :slotted 命中 -->
      <slot />
    </div>
  </Teleport>
</template>

<style scoped>
/* 壳：Teleport 到 body 的 fixed 弹出层。left/top 由调用方内联传入，
   必须 fixed 定位才生效——缺 position 会退化为 body 底部 static 块，
   菜单渲染到窗口角落/屏幕外，表现为"点了没反应"。 */
.menu-pop {
  position: fixed;
  z-index: var(--z-menu); /* 内容/边缘加深(2000)之上，toast(3000)/tooltip(3100) 之下 */
  border-radius: var(--radius-sm);
  border: 1px solid var(--border-default);
  background: var(--surface-card);
  box-shadow: var(--shadow-lg);
  padding: 4px;
  box-sizing: border-box;
}

.menu-pop :slotted(.menu-item) {
  padding: 8px 12px;
  font-size: 13px;
  color: var(--text-primary);
  border-radius: var(--radius-sm);
  cursor: pointer;
  white-space: nowrap;
  transition: background 0.12s;
}

.menu-pop :slotted(.menu-item:hover) {
  background: var(--color-brand);
  color: var(--text-inverse);
}

.menu-pop :slotted(.menu-sep) {
  height: 1px;
  margin: 5px 8px;
  background: var(--border-default);
}
</style>
