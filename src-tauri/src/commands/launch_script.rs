//! 启动脚本导出命令（薄胶水）：渲染在 launcher/script.rs（PCL 风格 .bat/.sh）。

use crate::launcher::args;
use crate::launcher::process;
use crate::launcher::script;

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
    // 与 spawn_game 同：正版模式用后端取的有效 token（自动刷新），其余沿用传入值
    let access_token = if auth_mode == "online" {
        crate::auth::account::valid_access_token().await?
    } else {
        access_token
    };
    let (auth, java_path, version_json, mc_dir, launcher_config, java_major) =
        process::prepare_launch(
            &version_dir_name,
            &auth_mode,
            &username,
            &access_token,
            &uuid,
            &xuid,
        )
        .await?;
    let cmd = args::build_launch_command(
        &java_path,
        &version_json,
        &args::LaunchRequest {
            version_dir_name: &version_dir_name,
            mc_dir: &mc_dir,
            config: &launcher_config,
            auth: &auth,
            java_major_version: java_major,
            user_properties: "",
        },
    );

    let script = script::render_launch_script(&cmd.args, &mc_dir, &version_dir_name);
    std::fs::write(&save_path, script).map_err(common_err_ctx!("写入脚本失败"))?;
    Ok(save_path)
}
