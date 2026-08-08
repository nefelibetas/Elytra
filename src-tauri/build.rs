fn main() {
    tauri_build::build();

    // 从项目根 .env(已被 .gitignore 忽略)读取敏感配置,以编译期环境变量注入。
    // 代码中用 env!("KEY") / option_env!("KEY") 读取;修改 .env 会触发重编译。
    // 这样源码仓库不包含任何明文密钥,key 仅存在于本地构建机。
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
                println!("cargo:rustc-env={}={}", key.trim(), value.trim());
                count += 1;
            }
        }
        if count > 0 {
            println!("cargo:warning=已从 .env 注入 {} 个构建环境变量", count);
        }
    }
}
