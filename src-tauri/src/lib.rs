#[macro_use]
mod macros;

mod auth;
mod commands;
pub mod config;
pub mod constants;
pub mod downloader;
mod events;
pub mod game;
mod jdk;
pub mod launcher;
pub mod online;
mod persist;
mod server_status;
pub mod task;
pub mod tooldl;
pub mod ts_export;
pub mod util;

use tauri::Emitter;

use commands::account::{
    cancel_ms_login, disable_cape, ensure_ms_account, external_login, fetch_account_skin,
    fetch_account_skin_profile, fetch_capes, fetch_default_skin, fetch_external_skin,
    fetch_external_skin_profile, get_external_accounts, get_ms_account, get_ms_accounts,
    get_external_config, refresh_external_account, refresh_ms_account, remove_external_account,
    remove_ms_account, save_external_skin, save_skin, save_external_config, set_active_cape,
    start_ms_login, switch_external_account, switch_external_role, switch_ms_account,
    upload_skin_file,
};
use commands::community::{get_community_details, get_community_versions, search_community};
use commands::config::take_config_recovery_notice;
use commands::downloader::{
    get_downloader_config, get_installed_versions, get_version_list, resolve_version_name,
    save_downloader_config,
};
use commands::gamedirs::{
    add_game_dir, auto_activate_version, delete_version_dir, ensure_game_folder,
    get_active_game_dir, get_game_dirs, open_folder, remove_game_dir, rename_game_dir,
    rename_version_dir, set_active_game_dir, set_watched_version,
};
use commands::auto_tasks::{
    get_instance_auto_tasks, run_auto_tasks_now, save_global_auto_tasks, save_instance_auto_tasks,
};
use commands::jdk::{
    auto_scan_jdk, get_discovered_jdks, get_jdk_options, save_jdk_options, scan_custom_jdk,
};
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
use commands::skins::{
    export_skin_png, import_skin_png, list_local_skins, read_local_skin, remove_local_skin,
    save_local_skin,
};
use commands::task::{
    cancel_task, decide_waiting_task, dismiss_interrupted_task, download_files,
    download_single_file, install_modpack, list_interrupted_tasks, pause_task, resume_task,
    scan_missing_files, start_download, submit_install_jdk, submit_missing_download, task_exists,
};
use commands::tooldl::{
    tooldl_cancel, tooldl_clear_finished, tooldl_enqueue, tooldl_get_config, tooldl_list,
    tooldl_pause, tooldl_remove, tooldl_resume, tooldl_save_config,
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
    // 子进程不得弹控制台：GUI 进程先持有一个隐藏控制台，第三方库内部 spawn
    // （easytier 的 cmd/ipconfig 等）才会继承它而非新建窗口
    util::ensure_hidden_console();
    // 启动器日志落盘（stdout/stderr → Elytra/launcher.log）
    util::init_file_logging();
    task::init();
    tooldl::init();
    // 启动时后台自动扫描 JDK（已有持久化结果则跳过）
    std::thread::spawn(|| {
        jdk::init();
    });

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            // 配置损坏恢复：运行期经事件推送（启动期由前端 take_config_recovery_notice 拉取兜底）
            let handle = app.handle().clone();
            persist::set_recovery_hook(move |notice| {
                let _ = handle.emit(events::CONFIG_RECOVERY, notice);
            });
            // 游戏目录变化监听（激活目录的 versions/ 与聚焦版本子目录 → 事件推送）
            game::version::watch::init(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            auto_scan_jdk,
            scan_custom_jdk,
            get_jdk_options,
            save_jdk_options,
            get_discovered_jdks,
            submit_install_jdk,
            get_version_list,
            get_game_dirs,
            add_game_dir,
            rename_game_dir,
            remove_game_dir,
            set_active_game_dir,
            set_watched_version,
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
            get_instance_auto_tasks,
            save_instance_auto_tasks,
            save_global_auto_tasks,
            run_auto_tasks_now,
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
            get_external_config,
            save_external_config,
            cancel_ms_login,
            get_ms_account,
            get_ms_accounts,
            remove_ms_account,
            switch_ms_account,
            ensure_ms_account,
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
            list_local_skins,
            save_local_skin,
            remove_local_skin,
            read_local_skin,
            export_skin_png,
            import_skin_png,
            online_start_host,
            online_join,
            online_stop,
            online_status,
            get_online_config,
            save_online_config,
            ping_server,
            tooldl_enqueue,
            tooldl_pause,
            tooldl_resume,
            tooldl_cancel,
            tooldl_remove,
            tooldl_clear_finished,
            tooldl_list,
            tooldl_get_config,
            tooldl_save_config,
            take_config_recovery_notice
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
