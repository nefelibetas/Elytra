mod commands;
pub mod config;
pub mod downloader;
mod persist;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            commands::jdk::auto_scan_jdk,
            commands::jdk::scan_custom_jdk,
            commands::jdk::get_jdk_config,
            commands::jdk::save_jdk_config,
            commands::downloader::get_version_list,
            commands::downloader::download_version,
            commands::downloader::get_downloader_config,
            commands::downloader::save_downloader_config,
            commands::downloader::get_installed_versions,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
