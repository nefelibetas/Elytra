mod auth;
mod commands;
pub mod config;
pub mod constants;
pub mod downloader;
mod events;
mod jdk;
mod persist;
pub mod task;
pub mod util;

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
            commands::task::submit_install_jdk,
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
            commands::version::list_worlds,
            commands::version::get_world_icon,
            commands::version::list_resource_files,
            commands::version::list_resource_detailed,
            commands::version::delete_resource_file,
            commands::version::delete_world,
            commands::version::list_mods_detailed,
            commands::version::check_mod_updates,
            commands::version::update_mod_file,
            commands::version::delete_mod_file,
            commands::version::export_modpack,
            commands::version_icon::get_version_icon,
            commands::version_icon::get_version_icon_source,
            commands::version_icon::set_version_icon,
            commands::version_icon::reset_version_icon,
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
            commands::community::search_community,
            commands::community::get_community_versions,
            commands::community::get_community_details,
            commands::task::start_download,
            commands::task::download_files,
            commands::task::download_single_file,
            commands::task::install_modpack,
            commands::task::cancel_task,
            commands::task::decide_waiting_task,
            commands::task::task_exists,
            commands::task::list_interrupted_tasks,
            commands::task::resume_task,
            commands::task::dismiss_interrupted_task,
            commands::launch::spawn_game,
            commands::launch::check_launch_env,
            commands::launch::get_memory_info,
            commands::launch::get_launch_options,
            commands::launch::save_launch_options,
            commands::launch::get_last_offline_username,
            commands::launch::save_last_offline_username,
            commands::launch::get_offline_accounts,
            commands::launch::save_offline_account,
            commands::launch::remove_offline_account,
            commands::account::start_ms_login,
            commands::account::cancel_ms_login,
            commands::account::get_ms_account,
            commands::account::get_ms_accounts,
            commands::account::remove_ms_account,
            commands::account::switch_ms_account,
            commands::account::refresh_ms_account,
            commands::account::fetch_avatar,
            commands::account::fetch_texture_id,
            commands::account::fetch_account_skin,
            commands::account::fetch_default_skin,
            commands::account::save_skin,
            commands::account::upload_skin_file,
            commands::account::fetch_capes,
            commands::account::set_active_cape,
            commands::account::disable_cape,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
