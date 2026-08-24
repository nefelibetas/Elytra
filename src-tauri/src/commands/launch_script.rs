//! 启动脚本渲染与导出命令（PCL 风格脚本：Windows .bat / macOS/Linux .sh）。

use crate::launcher::args::build_launch_command;

use super::launch::prepare_launch;

/// 渲染启动脚本内容（Windows: .bat；macOS/Linux: .sh）。
/// 格式对齐 PCL 导出脚本：chcp 65001 + title + cd 版本目录 + java 命令行 + pause。
fn render_launch_script(
    args: &[String],
    mc_dir: &std::path::Path,
    version_dir_name: &str,
) -> String {
    #[cfg(target_os = "windows")]
    {
        let mut out = String::new();
        out.push_str("chcp 65001>nul\r\n");
        out.push_str("@echo off\r\n");
        out.push_str(&format!("title 启动 - {version_dir_name}\r\n"));
        out.push_str("echo 游戏正在启动，请稍候。\r\n");
        let vdir = mc_dir.join("versions").join(version_dir_name);
        out.push_str(&format!("cd /D \"{}\"\r\n\r\n", vdir.display()));
        for a in args {
            out.push('"');
            out.push_str(&a.replace('%', "%%"));
            out.push_str("\" ");
        }
        out.push_str("\r\necho 游戏已退出。\r\n");
        out.push_str("pause\r\n");
        out
    }
    #[cfg(not(target_os = "windows"))]
    {
        let mut out = String::from("#!/usr/bin/env sh\n");
        out.push_str(&format!("# 启动 - {version_dir_name}\n"));
        out.push_str(&format!(
            "cd \"{}\"\n",
            mc_dir.join("versions").join(version_dir_name).display()
        ));
        out.push_str("exec ");
        for a in args {
            out.push('\'');
            out.push_str(&a.replace('\'', "'\\''"));
            out.push_str("' ");
        }
        out.push('\n');
        out
    }
}

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

    let script = render_launch_script(&cmd.args, &mc_dir, &version_dir_name);
    std::fs::write(&save_path, script).map_err(|e| format!("写入脚本失败\n{e}"))?;
    Ok(save_path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_launch_script_bat() {
        #[cfg(target_os = "windows")]
        {
            let args = vec![
                r"C:\java\bin\java.exe".to_string(),
                "-Xmx2G".to_string(),
                "net.minecraft.client.main.Main".to_string(),
            ];
            let s = render_launch_script(
                &args,
                std::path::Path::new(r"D:\Minecraft\.minecraft"),
                "26.2",
            );
            assert!(s.starts_with("chcp 65001>nul\r\n"));
            assert!(s.contains("title 启动 - 26.2\r\n"));
            assert!(s.contains(r#"cd /D "D:\Minecraft\.minecraft\versions\26.2""#));
            assert!(
                s.contains(r#""C:\java\bin\java.exe" "-Xmx2G" "net.minecraft.client.main.Main" "#)
            );
            assert!(s.ends_with("pause\r\n"));
        }
    }
}
