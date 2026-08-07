mod auth;
mod commands;
pub mod config;
pub mod constants;
pub mod downloader;
mod events;
mod persist;
pub mod task;
mod jdk;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    task::init();
    // 启动时后台自动扫描 JDK（已有持久化结果则跳过）
    std::thread::spawn(|| {
        jdk::init();
    });

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            commands::jdk::auto_scan_jdk,
            commands::jdk::scan_custom_jdk,
            commands::jdk::get_jdk_config,
            commands::jdk::save_jdk_config,
            commands::downloader::get_version_list,
            commands::gamedirs::get_game_dirs,
            commands::gamedirs::add_game_dir,
            commands::gamedirs::rename_game_dir,
            commands::gamedirs::remove_game_dir,
            commands::gamedirs::set_active_game_dir,
            commands::gamedirs::get_active_game_dir,
            commands::gamedirs::rename_version_dir,
            commands::gamedirs::delete_version_dir,
            commands::gamedirs::ensure_game_folder,
            commands::gamedirs::open_folder,
            commands::gamedirs::auto_activate_version,
            commands::launch::export_launch_script,
            commands::task::scan_missing_files,
            commands::task::submit_missing_download,
            commands::downloader::get_downloader_config,
            commands::downloader::save_downloader_config,
            commands::downloader::get_installed_versions,
            commands::downloader::resolve_version_name,
            commands::loader::get_forge_versions,
            commands::loader::get_fabric_versions,
            commands::loader::get_quilt_versions,
            commands::loader::get_optifine_versions,
            commands::loader::get_neoforge_versions,
            commands::loader::get_fabric_api_versions,
            commands::task::start_download,
            commands::task::download_files,
            commands::task::download_single_file,
            commands::task::cancel_task,
            commands::launch::spawn_game,
            commands::launch::check_launch_env,
            commands::launch::get_memory_info,
            commands::launch::get_launch_options,
            commands::launch::save_launch_options,
            commands::launch::get_last_offline_username,
            commands::launch::save_last_offline_username,
            commands::account::start_ms_login,
            commands::account::get_ms_account,
            commands::account::clear_ms_account,
            commands::account::refresh_ms_account,
            commands::account::fetch_avatar,
            commands::account::fetch_texture_id,
            commands::account::fetch_default_skin,
            commands::account::fetch_player_skin_texture,
            commands::account::save_skin,
            commands::account::upload_skin_file,
            commands::account::fetch_capes,
            commands::account::set_active_cape,
            commands::account::disable_cape,
            commands::account::get_offline_skin,
            commands::account::set_offline_skin,
            commands::account::read_skin_file,
            commands::account::fetch_player_skin,
            commands::account::save_player_skin,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
