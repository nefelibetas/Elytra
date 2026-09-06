//! 启动脚本渲染（PCL 风格：Windows .bat / macOS/Linux .sh）。
//! 命令壳在 commands/launch_script.rs；命令构建见 args.rs。

/// 渲染启动脚本内容（Windows: .bat；macOS/Linux: .sh）。
/// 格式对齐 PCL 导出脚本：chcp 65001 + title + cd 版本目录 + java 命令行 + pause。
pub fn render_launch_script(
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
