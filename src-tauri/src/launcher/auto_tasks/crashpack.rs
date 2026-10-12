//! 内置动作：崩溃日志打包（zip + 脱敏）。
//!
//! 崩溃包会被用户直接发给他人排查，故所有文本成员一律过 [`redact`]；
//! 成员缺失静默跳过（崩溃现场本就不整齐），超大成员降级为占位说明。

use std::io::Write;
use std::path::{Path, PathBuf};

use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

use super::ctx::{AutoTaskCtx, INSTANCE_FILE};
use super::redact::redact;

/// 打包输出目录：`{config_dir}/crash-packs/`。
pub fn packs_dir(config_dir: &Path) -> PathBuf {
    config_dir.join("crash-packs")
}

/// 单个成员的大小上限（超过只留占位说明，避免把 GB 级日志读进内存）。
const MAX_MEMBER_BYTES: u64 = 32 * 1024 * 1024;

/// 打包一次崩溃现场，返回 zip 路径。
pub fn pack_crash_logs(ctx: &AutoTaskCtx<'_>) -> Result<PathBuf, String> {
    let dir = packs_dir(ctx.config_dir);
    std::fs::create_dir_all(&dir).map_err(common_err_ctx!("创建崩溃包目录失败"))?;
    let out = dir.join(format!(
        "crash-{}-{}.zip",
        ctx.version_id,
        crate::util::utc_stamp()
    ));
    let file = std::fs::File::create(&out).map_err(common_err_ctx!("创建崩溃包失败"))?;
    let mut zip = ZipWriter::new(file);
    let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);

    add_text(
        &mut zip,
        opts,
        "info.txt",
        &redact(&build_info(ctx), ctx.username),
    )?;

    if let Some(report) = ctx.crash_report {
        let name = Path::new(report)
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "crash-report.txt".into());
        add_file(
            &mut zip,
            opts,
            Path::new(report),
            &format!("crash-report/{name}"),
            ctx.username,
        )?;
    }
    for (label, path) in [
        (
            "logs/latest.log",
            ctx.game_dir.join("logs").join("latest.log"),
        ),
        ("logs/debug.log", ctx.game_dir.join("logs").join("debug.log")),
        (
            "logs/elytra-launch.log",
            ctx.mc_dir.join("logs").join("elytra-launch.log"),
        ),
    ] {
        add_file(&mut zip, opts, &path, label, ctx.username)?;
    }
    add_file(
        &mut zip,
        opts,
        &super::record::mods_json_path(ctx.config_dir, ctx.version_id),
        "mods.json",
        ctx.username,
    )?;
    add_file(
        &mut zip,
        opts,
        &ctx.instance_file(),
        INSTANCE_FILE,
        ctx.username,
    )?;
    // 版本 JSON：目录内文件名可能 ≠ 目录名，按目录名试一次（失败静默跳过）
    add_file(
        &mut zip,
        opts,
        &ctx.game_dir.join(format!("{}.json", ctx.version_id)),
        "version.json",
        ctx.username,
    )?;

    zip.finish().map_err(common_err_ctx!("写入崩溃包失败"))?;
    Ok(out)
}

/// 环境信息（脱敏后作为 `info.txt`；路径与账号信息由调用处统一处理）。
fn build_info(ctx: &AutoTaskCtx<'_>) -> String {
    format!(
        "instance: {}\nminecraft_dir: {}\ninstance_dir: {}\nexit_code: {}\ncrash_report: {}\nos: {} {}\nlauncher: {} {}\n",
        ctx.version_id,
        ctx.mc_dir.display(),
        ctx.game_dir.display(),
        ctx.exit_code
            .map(|c| c.to_string())
            .unwrap_or_else(|| "unknown".into()),
        ctx.crash_report.unwrap_or("none"),
        std::env::consts::OS,
        std::env::consts::ARCH,
        crate::constants::APP_NAME,
        crate::constants::APP_VERSION,
    )
}

/// 写入一个文本成员（已脱敏）。
fn add_text(
    zip: &mut ZipWriter<std::fs::File>,
    opts: SimpleFileOptions,
    name: &str,
    text: &str,
) -> Result<(), String> {
    zip.start_file(name, opts)
        .map_err(common_err_ctx!("写入崩溃包失败"))?;
    zip.write_all(text.as_bytes())
        .map_err(common_err_ctx!("写入崩溃包失败"))
}

/// 写入一个文件成员：缺失/不可读静默跳过；文本脱敏；超大降级为占位说明。
fn add_file(
    zip: &mut ZipWriter<std::fs::File>,
    opts: SimpleFileOptions,
    path: &Path,
    name: &str,
    username: &str,
) -> Result<(), String> {
    let Ok(meta) = std::fs::metadata(path) else {
        return Ok(());
    };
    if !meta.is_file() {
        return Ok(());
    }
    if meta.len() > MAX_MEMBER_BYTES {
        return add_text(
            zip,
            opts,
            &format!("{name}.skipped.txt"),
            &format!("成员过大已跳过（{} 字节）", meta.len()),
        );
    }
    let Ok(bytes) = std::fs::read(path) else {
        return Ok(());
    };
    match String::from_utf8(bytes) {
        Ok(text) => add_text(zip, opts, name, &redact(&text, username)),
        Err(e) => {
            let raw = e.into_bytes();
            zip.start_file(name, opts)
                .map_err(common_err_ctx!("写入崩溃包失败"))?;
            zip.write_all(&raw)
                .map_err(common_err_ctx!("写入崩溃包失败"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn tmp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("elytra-autotasks-crashpack-{name}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn fixture(name: &str) -> (PathBuf, PathBuf, PathBuf) {
        let config = tmp_dir(&format!("{name}-cfg"));
        let mc = tmp_dir(&format!("{name}-mc"));
        let game = mc.join("versions").join("1.20.1-fabric");
        fs::create_dir_all(game.join("logs")).unwrap();
        fs::create_dir_all(mc.join("logs")).unwrap();
        (config, mc, game)
    }

    fn read_zip(path: &Path) -> std::collections::HashMap<String, String> {
        let file = fs::File::open(path).unwrap();
        let mut archive = zip::ZipArchive::new(file).unwrap();
        let mut out = std::collections::HashMap::new();
        for i in 0..archive.len() {
            let mut entry = archive.by_index(i).unwrap();
            let mut text = String::new();
            std::io::Read::read_to_string(&mut entry, &mut text).unwrap();
            out.insert(entry.name().to_string(), text);
        }
        out
    }

    #[test]
    fn packs_logs_and_redacts_credentials() {
        let (config, mc, game) = fixture("basic");
        fs::write(
            game.join("logs/latest.log"),
            r#"{"accessToken":"eyJhbGci.secret"}"#,
        )
        .unwrap();
        fs::write(
            mc.join("logs/elytra-launch.log"),
            "java --username Steve --version 1.20.1",
        )
        .unwrap();
        let ctx = AutoTaskCtx {
            mc_dir: &mc,
            game_dir: &game,
            version_id: "1.20.1-fabric",
            config_dir: &config,
            username: "Steve",
            exit_code: Some(1),
            crash_report: None,
        };
        let out = pack_crash_logs(&ctx).unwrap();
        let entries = read_zip(&out);

        let latest = entries.get("logs/latest.log").expect("latest.log 应收录");
        assert!(!latest.contains("eyJhbGci.secret"), "凭据必须脱敏");
        let launch = entries
            .get("logs/elytra-launch.log")
            .expect("启动器日志应收录");
        assert!(!launch.contains("Steve"), "--username 必须脱敏");
        let info = entries.get("info.txt").expect("info.txt 应收录");
        assert!(info.contains("instance: 1.20.1-fabric"));
        assert!(info.contains("exit_code: 1"));
        assert!(!entries.contains_key("logs/debug.log"), "缺失成员不产生条目");
    }

    #[test]
    fn includes_crash_report_under_dedicated_dir() {
        let (config, mc, game) = fixture("report");
        let report = game.join("crash-reports").join("crash-2026-10-12.txt");
        fs::create_dir_all(report.parent().unwrap()).unwrap();
        fs::write(&report, "---- Minecraft Crash Report ----\nOutOfMemoryError").unwrap();
        let report_str = report.display().to_string();
        let ctx = AutoTaskCtx {
            mc_dir: &mc,
            game_dir: &game,
            version_id: "1.20.1-fabric",
            config_dir: &config,
            username: "Steve",
            exit_code: Some(1),
            crash_report: Some(&report_str),
        };
        let out = pack_crash_logs(&ctx).unwrap();
        let entries = read_zip(&out);
        assert!(
            entries.contains_key("crash-report/crash-2026-10-12.txt"),
            "崩溃报告应放 crash-report/ 下：{:?}",
            entries.keys().collect::<Vec<_>>()
        );
    }

    #[test]
    fn oversize_member_becomes_placeholder() {
        let (config, mc, game) = fixture("oversize");
        let big = game.join("logs/latest.log");
        fs::write(&big, vec![b'x'; (MAX_MEMBER_BYTES + 1) as usize]).unwrap();
        let ctx = AutoTaskCtx {
            mc_dir: &mc,
            game_dir: &game,
            version_id: "inst",
            config_dir: &config,
            username: "",
            exit_code: None,
            crash_report: None,
        };
        let out = pack_crash_logs(&ctx).unwrap();
        let entries = read_zip(&out);
        assert!(!entries.contains_key("logs/latest.log"));
        assert!(entries.contains_key("logs/latest.log.skipped.txt"));
    }

    #[test]
    fn includes_existing_mods_json() {
        let (config, mc, game) = fixture("mods");
        fs::write(game.join("mods.fake"), b"x").ok();
        let mods_dir = game.join("mods");
        fs::create_dir_all(&mods_dir).unwrap();
        fs::write(mods_dir.join("sodium.jar"), b"j").unwrap();
        super::super::record::record_mods(&config, "inst", &game).unwrap();
        let ctx = AutoTaskCtx {
            mc_dir: &mc,
            game_dir: &game,
            version_id: "inst",
            config_dir: &config,
            username: "",
            exit_code: Some(1),
            crash_report: None,
        };
        let out = pack_crash_logs(&ctx).unwrap();
        let entries = read_zip(&out);
        assert!(entries.contains_key("mods.json"));
        assert!(entries["mods.json"].contains("sodium.jar"));
    }
}
