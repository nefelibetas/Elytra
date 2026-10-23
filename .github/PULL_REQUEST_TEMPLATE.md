## 改动内容

<!-- 简要说明做了什么、为什么这样做 -->

## 关联 Issue

<!-- Closes #123；没有则写“无” -->

## 验证

<!-- 在已执行的项前打 x；未执行或不适用的项请说明原因 -->

- [ ] `cd src-tauri && cargo fmt --check`
- [ ] `cd src-tauri && cargo check`
- [ ] `cd src-tauri && cargo clippy --lib`（零警告）
- [ ] `cd src-tauri && cargo test --lib -- --test-threads=1`
- [ ] `npx vue-tsc --noEmit`
- [ ] `pnpm lint:css`
- [ ] `pnpm i18n:check`
- [ ] 改动涉及 Rust 数据结构时已重跑 `cargo run --bin gen_ts --features gen-ts`

## 截图 / 录屏

<!-- 涉及界面改动时附上；无界面改动可删除本节 -->

## 备注

<!-- 破坏性变更、迁移说明、已知限制等 -->
