<script setup lang="ts">
/**
 * 统一 checkbox / radio 开关（appearance:none 完全自定义）。
 * 消除 UA 默认部件跨平台差异：Win WebView2 / macOS WKWebView / Linux WebKitGTK 原生渲染不一致。
 * 勾选态用 label + :checked + 相邻选择器实现（不依赖 :has()，兼容 WebKitGTK 2.40）。
 */
const props = withDefaults(
  defineProps<{
    /** checkbox：boolean；radio：当前选中值（字符串） */
    modelValue?: boolean | string;
    type?: "checkbox" | "radio";
    /** radio 的选中值（v-model 为字符串时用 value 匹配） */
    value?: string;
    disabled?: boolean;
    /** 14 = 工具页紧凑 / 15 = 设置页 / 16 = 对话框 */
    size?: 14 | 15 | 16;
    /** 文字变灰（配合 disabled 联动场景） */
    dim?: boolean;
  }>(),
  { type: "checkbox", size: 15 },
);

const emit = defineEmits<{
  "update:modelValue": [v: boolean | string];
  change: [v: boolean | string];
}>();

function onChange(e: Event) {
  const el = e.target as HTMLInputElement;
  if (props.type === "radio") {
    // radio：选中时 v-model 值 = 自身 value（与原生 radio 语义一致；radio 不可取消选中）
    const v = el.checked ? props.value : props.modelValue;
    emit("update:modelValue", v ?? "");
    emit("change", v ?? "");
  } else {
    emit("update:modelValue", el.checked);
    emit("change", el.checked);
  }
}
</script>

<template>
  <label class="ck" :class="{ disabled, dim }" :style="{ '--ck-size': size + 'px' }">
    <input
      :type="type"
      :checked="type === 'radio' ? modelValue === value : !!modelValue"
      :value="value"
      :disabled="disabled"
      @change="onChange"
    />
    <span class="ck-box"></span>
    <span class="ck-label"><slot /></span>
  </label>
</template>

<style scoped>
.ck {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  font-size: 13px;
  color: var(--text-primary);
  cursor: pointer;
  user-select: none;
}

.ck.disabled {
  cursor: not-allowed;
  opacity: 0.5;
}

.ck.dim .ck-label {
  opacity: 0.5;
}

/* 视觉框：完全自定义，不依赖 UA 部件 */
.ck-box {
  flex: none;
  width: var(--ck-size, 15px);
  height: var(--ck-size, 15px);
  border: 1.5px solid var(--text-tertiary);
  border-radius: 4px;
  display: inline-block;
  position: relative;
  transition: border-color 0.15s, background 0.15s;
}

.ck input[type="radio"] + .ck-box {
  border-radius: 50%;
}

.ck:hover .ck-box {
  border-color: var(--color-brand);
}

/* 选中态：checkbox 品牌填充 + 白勾；radio 品牌圆点 */
.ck input:checked + .ck-box {
  background: var(--color-brand);
  border-color: var(--color-brand);
}

.ck input:checked + .ck-box::after {
  content: "";
  position: absolute;
  left: 50%;
  top: 50%;
  width: 4px;
  height: 8px;
  border: solid var(--text-inverse);
  border-width: 0 2px 2px 0;
  transform: translate(-50%, -60%) rotate(45deg);
}

.ck input[type="radio"]:checked + .ck-box {
  background: radial-gradient(circle, var(--color-brand) 0 45%, transparent 55%);
}

.ck input[type="radio"]:checked + .ck-box::after {
  display: none;
}

.ck-label {
  line-height: 1.3;
}
</style>
