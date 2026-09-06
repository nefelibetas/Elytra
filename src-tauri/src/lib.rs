mod auth;
mod commands;
pub mod config;
pub mod constants;
pub mod downloader;
mod events;
mod jdk;
pub mod launcher;
pub mod map;
pub mod online;
mod persist;
mod server_status;
pub mod task;
pub mod util;

use commands::account::{
    cancel_ms_login, disable_cape, external_login, fetch_account_skin, fetch_account_skin_profile,
    fetch_external_skin, fetch_external_skin_profile, save_external_skin, fetch_capes,
    fetch_default_skin, get_external_accounts,
    switch_external_role, refresh_external_account,
    switch_external_account, get_ms_account, get_ms_accounts,
    get_third_party_config, refresh_ms_account, remove_external_account, remove_ms_account,
    save_skin, save_third_party_config, set_active_cape, start_ms_login, switch_ms_account,
    upload_skin_file,
};
use commands::community::{get_community_details, get_community_versions, search_community};
use commands::map::{
    get_map_block, get_map_tile, get_prediction_tile, get_prediction_tile_seed, get_seed_map_block,
    list_map_regions,
};
use commands::map_editor::{get_map_tree, get_map_world_icon};
use commands::downloader::{
    get_downloader_config, get_installed_versions, get_version_list, resolve_version_name,
    save_downloader_config,
};
use commands::gamedirs::{
    add_game_dir, auto_activate_version, delete_version_dir, ensure_game_folder,
    get_active_game_dir, get_game_dirs, open_folder, remove_game_dir, rename_game_dir,
    rename_version_dir, set_active_game_dir,
};
use commands::jdk::{auto_scan_jdk, get_jdk_config, save_jdk_config, scan_custom_jdk};
use commands::launch::{
    check_launch_env, get_game_log, get_launcher_log, open_launcher_log_dir, spawn_game,
};
use commands::launch_options::{get_launch_options, get_memory_info, save_launch_options};
use commands::launch_script::export_launch_script;
use commands::loader::{
    get_fabric_api_versions, get_fabric_versions, get_forge_versions, get_neoforge_versions,
    get_optifine_versions, get_quilt_versions,
};
use commands::offline_account::{
    get_last_offline_username, get_offline_accounts, remove_offline_account,
    save_last_offline_username, save_offline_account,
};
use commands::online::{
    get_online_config, online_join, online_start_host, online_status, online_stop,
    save_online_config,
};
use commands::server_status::ping_server;
use commands::task::{
    cancel_task, decide_waiting_task, dismiss_interrupted_task, download_files,
    download_single_file, install_modpack, list_interrupted_tasks, pause_task, resume_task,
    scan_missing_files, start_download, submit_install_jdk, submit_missing_download, task_exists,
};
use commands::version::{
    check_mod_updates, delete_mod_file, delete_resource_file, delete_world, export_modpack,
    get_version_server, get_world_icon, identify_mod_file, list_mods_detailed,
    list_resource_detailed, list_resource_files, list_worlds, save_version_server, toggle_mod_file,
    update_mod_file,
};
use commands::version_icon::{
    get_version_icon, get_version_icon_source, reset_version_icon, set_version_icon,
};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // 启动器日志落盘（stdout/stderr → Elytra/launcher.log）
    util::init_file_logging();
    task::init();
    // 启动时后台自动扫描 JDK（已有持久化结果则跳过）
    std::thread::spawn(|| {
        jdk::init();
    });

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            auto_scan_jdk,
            scan_custom_jdk,
            get_jdk_config,
            save_jdk_config,
            submit_install_jdk,
            get_version_list,
            get_game_dirs,
            add_game_dir,
            rename_game_dir,
            remove_game_dir,
            set_active_game_dir,
            get_active_game_dir,
            rename_version_dir,
            delete_version_dir,
            ensure_game_folder,
            open_folder,
            auto_activate_version,
            list_worlds,
            get_world_icon,
            list_resource_files,
            list_resource_detailed,
            delete_resource_file,
            delete_world,
            identify_mod_file,
            list_mods_detailed,
            check_mod_updates,
            update_mod_file,
            delete_mod_file,
            get_version_server,
            save_version_server,
            toggle_mod_file,
            export_modpack,
            get_version_icon,
            get_version_icon_source,
            set_version_icon,
            reset_version_icon,
            export_launch_script,
            scan_missing_files,
            submit_missing_download,
            get_downloader_config,
            save_downloader_config,
            get_installed_versions,
            resolve_version_name,
            get_forge_versions,
            get_fabric_versions,
            get_quilt_versions,
            get_optifine_versions,
            get_neoforge_versions,
            get_fabric_api_versions,
            search_community,
            get_community_versions,
            get_community_details,
            start_download,
            download_files,
            get_map_tree,
            get_map_world_icon,
            list_map_regions,
            get_map_tile,
            get_map_block,
            get_prediction_tile,
            get_prediction_tile_seed,
            get_seed_map_block,
            download_single_file,
            install_modpack,
            cancel_task,
            pause_task,
            decide_waiting_task,
            task_exists,
            list_interrupted_tasks,
            resume_task,
            dismiss_interrupted_task,
            spawn_game,
            get_game_log,
            get_launcher_log,
            open_launcher_log_dir,
            check_launch_env,
            get_memory_info,
            get_launch_options,
            save_launch_options,
            get_last_offline_username,
            save_last_offline_username,
            get_offline_accounts,
            save_offline_account,
            remove_offline_account,
            start_ms_login,
            external_login,
            get_external_accounts,
            switch_external_role,
            refresh_external_account,
            switch_external_account,
            remove_external_account,
            get_third_party_config,
            save_third_party_config,
            cancel_ms_login,
            get_ms_account,
            get_ms_accounts,
            remove_ms_account,
            switch_ms_account,
            refresh_ms_account,
            fetch_account_skin,
            fetch_account_skin_profile,
            fetch_external_skin,
            fetch_external_skin_profile,
            save_external_skin,
            fetch_default_skin,
            save_skin,
            upload_skin_file,
            fetch_capes,
            set_active_cape,
            disable_cape,
            online_start_host,
            online_join,
            online_stop,
            online_status,
            get_online_config,
            save_online_config,
            ping_server,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
