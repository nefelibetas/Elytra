//! 启动脚本导出命令（薄胶水）：渲染在 launcher/script.rs（PCL 风格 .bat/.sh）。

use crate::launcher::args::build_launch_command;
use crate::launcher::process::prepare_launch;

/// 导出启动脚本：按 PCL 风格生成平台对应脚本（Windows .bat / macOS/Linux .sh）写入指定路径。
/// 返回写入的文件路径。
#[tauri::command]
pub async fn export_launch_script(
    save_path: String,
    version_dir_name: String,
    auth_mode: String,
    username: String,
    access_token: String,
    uuid: String,
    xuid: String,
) -> Result<String, String> {
    let (auth, java_path, version_json, mc_dir, launcher_config, java_major) = prepare_launch(
        &version_dir_name,
        &auth_mode,
        &username,
        &access_token,
        &uuid,
        &xuid,
    )?;
    let cmd = build_launch_command(
        &java_path,
        &version_json,
        &version_dir_name,
        &mc_dir,
        &launcher_config,
        &auth,
        java_major,
        "",
    );

    let script = crate::launcher::script::render_launch_script(&cmd.args, &mc_dir, &version_dir_name);
    std::fs::write(&save_path, script).map_err(|e| format!("写入脚本失败\n{e}"))?;
    Ok(save_path)
}
