fn main() {
    tauri_build::build();

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
