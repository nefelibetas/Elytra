/** CSS 兼容性门：基线取 package.json 的 browserslist（Chrome 111 / Safari 16.4 ≈ WebKitGTK 2.40）。 */
export default {
  plugins: ["stylelint-no-unsupported-browser-features"],
  overrides: [{ files: ["**/*.vue"], customSyntax: "postcss-html" }],
  rules: {
    "plugin/no-unsupported-browser-features": [
      true,
      {
        // 仅触摸设备生效；桌面三端鼠标指针路径不受影响
        ignore: ["css-touch-action"],
      },
    ],
  },
};
