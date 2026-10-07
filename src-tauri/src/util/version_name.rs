//! 版本目录名 / Java 版本字符串解析（与前端 versionClassify 同规则）。

/// loader 标记 key 列表与顺序（与前端 utils/versionClassify 的 LOADER_KEYS 一致；
/// 各标记互不重叠，顺序不影响结果，仅保持同一事实源）。
const LOADER_KEYS: &[&str] = &["neoforge", "forge", "fabric", "quilt", "optifine"];

/// 冲突重命名 `{id}_n` 后缀剥离（与前端 classifyVersion 的 /_\d+$/ 语义一致）：
/// 只剥**尾部**一段 `_<数字>`（`26.2-forge-65.1.0_1` → `26.2-forge-65.1.0`；
/// `x_1_2` → `x_1`；`x_` 不剥）。
fn strip_conflict_suffix(name: &str) -> &str {
    if let Some(i) = name.rfind('_') {
        let tail = &name[i + 1..];
        if !tail.is_empty() && tail.bytes().all(|b| b.is_ascii_digit()) {
            return &name[..i];
        }
    }
    name
}

/// 大小写不敏感查找 `-{key}-` loader 标记（标记前必须还有内容，idx > 0）。
/// ASCII 小写化不改变字节长度，返回的 idx 可直接回用于原串切片。
fn find_loader_marker(base: &str) -> Option<(usize, &'static str)> {
    let lower = base.to_ascii_lowercase();
    for key in LOADER_KEYS {
        let marker = format!("-{key}-");
        if let Some(idx) = lower.find(&marker) {
            if idx > 0 {
                return Some((idx, key));
            }
        }
    }
    None
}

/// 从版本目录名提取 MC 版本（与前端 utils/versionClassify 同规则）：
/// 找 `-{loader_key}-` 标记（大小写不敏感）取标记前部分；无标记（vanilla）取整个目录名；
/// 冲突重命名 `_n` 后缀按尾部 `_数字` 剥离一次。
/// **仅限展示与本地键**（natives 共享目录等）；对外表达真实 MC 版本应从版本目录内
/// json 解析（见 commands/version.rs::resolve_version_meta），目录名解析只是兜底。
pub fn mc_version_from_dir(dir_name: &str) -> &str {
    let base = strip_conflict_suffix(dir_name);
    match find_loader_marker(base) {
        Some((idx, _)) => &base[..idx],
        None => base,
    }
}

/// 从版本目录名解析 ModLoader：(loader_key, loader_version)。
/// `26.2-fabric-0.16.9` → Some(("fabric", "0.16.9"))；vanilla → None；
/// 标记匹配大小写不敏感（PCL2 等外部目录名可能大写）。optifine 也识别
/// （导出 mrpack 时忽略——dependencies 无 OptiFine 标准键，文件本体已在 overrides 里）。
pub fn loader_from_dir(dir_name: &str) -> Option<(&str, &str)> {
    let base = strip_conflict_suffix(dir_name);
    let (idx, key) = find_loader_marker(base)?;
    Some((key, &base[idx + key.len() + 2..]))
}

/// 从版本字符串解析主版本号。
/// 支持格式："17.0.8" → 17, "21" → 21, "25-ea" → 25, "1.8.0_202" → 8
pub fn parse_java_major(version: &str) -> Option<u32> {
    // "1.x" 格式（Java 8 及更早，如 "1.8.0_202" → 8）
    if let Some(v) = version.strip_prefix("1.") {
        return v.split('.').next()?.parse::<u32>().ok();
    }
    // 新格式："17.0.8" → 17
    version
        .split(|c: char| !c.is_ascii_digit())
        .next()
        .and_then(|s| s.parse::<u32>().ok())
}

/// 从版本 json 文本推断 loader（轻量字符串检查，无需完整解析）：
/// 特征名匹配优先级 fabric → neoforge → forge → optifine（neoforge 在 forge 前，
/// 避免 "neoforged" 子串误匹配）；无特征视为原版。
pub fn infer_loader_from_json(text: &str) -> String {
    if text.contains("net.fabricmc") {
        "fabric".into()
    } else if text.contains("net.neoforged") {
        "neoforge".into()
    } else if text.contains("net.minecraftforge") {
        "forge".into()
    } else if text.contains("optifine") {
        "optifine".into()
    } else {
        "vanilla".into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mc_version_from_dir_all_naming_forms() {
        assert_eq!(mc_version_from_dir("26.2"), "26.2");
        assert_eq!(mc_version_from_dir("26.2-forge-65.1.0"), "26.2");
        assert_eq!(mc_version_from_dir("26.2-neoforge-26.2.0.53-beta"), "26.2");
        assert_eq!(
            mc_version_from_dir("1.20.1-pre1-forge-40.1.0"),
            "1.20.1-pre1"
        );
        assert_eq!(mc_version_from_dir("1.18.2-fabric-0.16.11"), "1.18.2");
        assert_eq!(mc_version_from_dir("1.12.2"), "1.12.2");
        assert_eq!(mc_version_from_dir("26.2-forge-65.1.0_1"), "26.2");
        // 与前端 classifyVersion 对齐：标记大小写不敏感（PCL2 等外部目录名大写）
        assert_eq!(mc_version_from_dir("1.20.1-Fabric-0.14.22"), "1.20.1");
        assert_eq!(mc_version_from_dir("1.20.1-FABRIC-0.14.22"), "1.20.1");
        // 尾部 `_数字` 只剥一段（与前端 /_\d+$/ 一致）
        assert_eq!(mc_version_from_dir("1.20.1_1_2"), "1.20.1_1");
        // 任意名无标记：整串兜底（vanilla 语义，仅供展示/本地键）
        assert_eq!(mc_version_from_dir("[abcd]"), "[abcd]");
        assert_eq!(mc_version_from_dir("我的整合包-fabric-1.0"), "我的整合包");
    }

    /// loader 解析与前端同规则（大小写不敏感 + 尾部冲突后缀）；解析器本身信任目录名，
    /// 对外真实语义由版本 json 权威层兜底（commands/version.rs::resolve_version_meta）。
    #[test]
    fn loader_from_dir_parity_forms() {
        assert_eq!(
            loader_from_dir("26.2-fabric-0.16.9"),
            Some(("fabric", "0.16.9"))
        );
        assert_eq!(
            loader_from_dir("1.20.1-Fabric-0.14.22"),
            Some(("fabric", "0.14.22"))
        );
        assert_eq!(
            loader_from_dir("26.2-forge-65.1.0_1"),
            Some(("forge", "65.1.0"))
        );
        assert_eq!(loader_from_dir("26.2"), None);
        assert_eq!(loader_from_dir("[abcd]"), None);
        // 伪标记示例：解析器原样给出，json 权威层会覆盖为真实 loader
        assert_eq!(loader_from_dir("X-fabric-Y"), Some(("fabric", "Y")));
    }

    #[test]
    fn test_parse_java_major() {
        assert_eq!(parse_java_major("1.8.0_202"), Some(8));
        assert_eq!(parse_java_major("1.7.0"), Some(7));
        assert_eq!(parse_java_major("1.6.0"), Some(6));
        assert_eq!(parse_java_major("17.0.8"), Some(17));
        assert_eq!(parse_java_major("21"), Some(21));
        assert_eq!(parse_java_major("25-ea"), Some(25));
        assert_eq!(parse_java_major("11.0.2"), Some(11));
    }

    #[test]
    fn infer_loader_variants() {
        assert_eq!(
            infer_loader_from_json(
                r#"{"mainClass":"net.fabricmc.loader.impl.launch.knot.KnotClient"}"#
            ),
            "fabric"
        );
        assert_eq!(
            infer_loader_from_json(r#"{"libraries":[{"name":"net.minecraftforge:forge:47.1.0"}]}"#),
            "forge"
        );
        assert_eq!(
            infer_loader_from_json(r#"{"libraries":[{"name":"net.neoforged:neoforge:21.0.0"}]}"#),
            "neoforge"
        );
        // neoforged 含 forge 子串：neoforge 必须在 forge 前匹配
        assert_eq!(
            infer_loader_from_json(r#"{"mainClass":"net.neoforged.bootstrap.BootstrapLauncher"}"#),
            "neoforge"
        );
        assert_eq!(
            infer_loader_from_json(r#"{"mainClass":"net.minecraft.client.main.Main"}"#),
            "vanilla"
        );
    }
}
