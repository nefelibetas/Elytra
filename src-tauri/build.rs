use std::env;
use std::path::{Path, PathBuf};

fn main() {
    tauri_build::build();
    build_cubiomes();

    let env_file = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../.env");
    println!("cargo:rerun-if-changed={}", env_file.display());
    if let Ok(content) = std::fs::read_to_string(&env_file) {
        let mut count = 0usize;
        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some((key, value)) = line.split_once('=') {
                let key = key.trim();
                let value = value.trim();
                // 空值视为未配置，避免复制 .env.example 后把空字符串注入 option_env!
                if key.is_empty() || value.is_empty() {
                    continue;
                }
                println!("cargo:rustc-env={}={}", key, value);
                count += 1;
            }
        }
        if count > 0 {
            println!("cargo:warning=已从 .env 注入 {} 个构建环境变量", count);
        }
    }
}

/// 种子预览：编译 vendored xpple/cubiomes。
/// Windows MSVC 目标用 clang-cl（上游不支持 cl.exe 的 VLA）；LLVM 已是 kcp-sys 构建依赖。
fn build_cubiomes() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("vendor/cubiomes");
    if env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") && env::var_os("CC").is_none() {
        let Some(clang) = find_clang_cl() else {
            panic!(
                "种子预览需要 clang-cl 编译 cubiomes：请安装 LLVM 并确保 clang-cl.exe 可用，或设置 CC/AR"
            );
        };
        env::set_var("CC", &clang);
        let llvm_lib = clang.with_file_name("llvm-lib.exe");
        if llvm_lib.is_file() {
            env::set_var("AR", llvm_lib);
        }
    }

    let mut build = cc::Build::new();
    build.include(&root);
    for file in [
        "biomenoise.c",
        "biomes.c",
        "layers.c",
        "generator.c",
        "finders.c",
        "util.c",
        "noise.c",
        "terrainnoise.c",
        "quadbase.c",
        "xradv.c",
        "carver.c",
        "features/stronghold.c",
        "features/end_city.c",
        "features/fortress.c",
        "features/abandoned_camp.c",
        "features/ore.c",
        "elytra_shim.c",
    ] {
        build.file(root.join(file));
    }
    build.flag_if_supported("-fwrapv");
    build.warnings(false);
    build.compile("cubiomes_elytra");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        println!("cargo:rustc-link-lib=m");
    }
    println!("cargo:rerun-if-changed={}", root.display());
}

fn find_clang_cl() -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(path) = env::var_os("LIBCLANG_PATH") {
        candidates.push(Path::new(&path).join("clang-cl.exe"));
    }
    candidates.push(PathBuf::from(r"C:\Program Files\LLVM\bin\clang-cl.exe"));
    if let Some(paths) = env::var_os("PATH") {
        for dir in env::split_paths(&paths) {
            candidates.push(dir.join("clang-cl.exe"));
        }
    }
    candidates.into_iter().find(|p| p.is_file())
}
