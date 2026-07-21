mod jdk;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            jdk::scan_jdk,
            jdk::scan_custom_jdk,
            jdk::get_jdk_config,
            jdk::save_jdk_config
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
