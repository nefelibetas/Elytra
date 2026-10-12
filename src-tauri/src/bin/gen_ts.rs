//! TS 类型生成命令：`cargo run --bin gen_ts --features gen-ts`（在 src-tauri 下执行）。
//! 输出到 `src/types/`（路径见 `.cargo/config.toml` 的 TS_RS_EXPORT_DIR）。

fn main() {
    elytra_lib::ts_export::export_all();
    println!("TS 类型已导出（src/types/）");
}
