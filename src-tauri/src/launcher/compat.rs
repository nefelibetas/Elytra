//! 版本 ↔ 所选 Java 主版本的兼容判定（启动前检查的唯一入口）。
//! 下界：版本 json 的 `javaVersion.majorVersion` 是硬要求（字节码不向下兼容），任何主版本过低一律拦。
//! 上界：Java 9 起向后兼容旧字节码，只有跨 Java 8→9 的老工具链会失败，故只对它们拦/提示。

use crate::game::version::model::VersionJson;

/// launchwrapper 主类（≤1.12.2 原版与 Forge；禁用开关会改走客户端主类）。
const CLASS_LAUNCHWRAPPER: &str = "net.minecraft.launchwrapper.Launch";
/// Java 8 时代只支持 Java 8 的加载器坐标前缀（Fabric/Quilt 在新 Java 上可用，不计入）。
const JAVA8_ONLY_LOADERS: [&str; 3] = ["net.minecraftforge:", "net.neoforged:", "optifine:"];

/// 预检查结论：`Block` 必然失败（拦下启动），`Warn` 可能失败（提示后仍可启动），`Ok` 放行。
pub enum JavaCompat {
    Ok,
    Warn(String),
    Block(String),
}

/// 依据版本声明、所选 Java 主版本与 JDK 位置给出结论；`Block`/`Warn` 的文案是 `@i18n:` 协议串
/// （首行 key，其后中文兜底与细节，前端直接展示）。
pub fn check(
    version_json: &VersionJson,
    java_major: u32,
    jdk_label: &str,
    disable_launch_wrapper: bool,
) -> JavaCompat {
    let required = version_json
        .java_version
        .major_version
        .as_u64()
        .unwrap_or(0) as u32;
    let launchwrapper = !disable_launch_wrapper && version_json.main_class == CLASS_LAUNCHWRAPPER;

    // 下界对全部主版本一视同仁：8/11/16/17/21/25 谁低谁拦（0 = 版本未声明，无法判定）
    if required > 0 && java_major < required {
        return JavaCompat::Block(keyed_msg!(
            "errors.jdk.too_old?need={required}&cur={java_major}",
            "该版本需要 Java {required} 或更高，当前 Java {java_major} 过低\n当前 JDK：{jdk_label}\n请在「设置 → Java 运行时」切换或下载（自动选择会按版本要求匹配）"
        ));
    }
    // 跨 8→9 的破坏性跃迁：launchwrapper 把类加载器强转 URLClassLoader，Java 9 起必然崩
    if launchwrapper && java_major > 8 {
        return JavaCompat::Block(keyed_msg!(
            "errors.jdk.legacy_java",
            "该版本需要 Java 8，请先在「设置 → Java 运行时」下载 Java 8\n当前 JDK：Java {java_major}（{jdk_label}）\n实例主类 launchwrapper 在 Java 9+ 上无法启动（ClassLoader 强转失败）"
        ));
    }
    // 同属 8 时代但主类不是 launchwrapper（1.13–1.16.5 的 Forge/OptiFine）：官方只支持 Java 8，仅提示
    if required > 0 && required <= 8 && java_major > 8 && has_java8_only_loader(version_json) {
        return JavaCompat::Warn(keyed_msg!(
            "launch.warn.jdkTooNew?cur={java_major}",
            "该版本的 Forge/NeoForge/OptiFine 只支持 Java 8，当前 Java {java_major} 可能启动失败\n建议在「设置 → Java 运行时」下载并选择 Java 8（原版与 Fabric/Quilt 不受影响）"
        ));
    }
    JavaCompat::Ok
}

fn has_java8_only_loader(version_json: &VersionJson) -> bool {
    version_json
        .libraries
        .iter()
        .any(|l| JAVA8_ONLY_LOADERS.iter().any(|p| l.name.starts_with(p)))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 最小版本 JSON：主类 + Java 要求 + 可选库坐标。
    fn version_json(main_class: &str, java_major: u32, libraries: &[&str]) -> VersionJson {
        let libs = libraries
            .iter()
            .map(|name| {
                format!(
                    r#"{{"name":"{name}","downloads":{{"artifact":{{"path":"a/b/1/a.jar","url":"x","sha1":"y","size":1}}}}}}"#
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        serde_json::from_str(&format!(
            r#"{{"id":"x","mainClass":"{main_class}","assets":"x","assetIndex":{{"id":"x","url":"x","sha1":"y","size":1,"totalSize":1}},"downloads":{{"client":{{"url":"x","sha1":"y","size":1}}}},"arguments":{{"game":[],"jvm":[]}},"complianceLevel":0,"minimumLauncherVersion":21,"javaVersion":{{"majorVersion":{java_major}}},"releaseTime":"2021-11-30T00:00:00Z","time":"2021-11-30T00:00:00Z","type":"release","libraries":[{libs}]}}"#
        ))
        .unwrap_or_else(|e| panic!("测试 JSON 应可解析: {e}"))
    }

    fn blocked(compat: JavaCompat) -> Option<String> {
        match compat {
            JavaCompat::Block(msg) => Some(msg),
            _ => None,
        }
    }

    fn warned(compat: JavaCompat) -> Option<String> {
        match compat {
            JavaCompat::Warn(msg) => Some(msg),
            _ => None,
        }
    }

    /// 下界矩阵：每个主版本都按同一规则判（过低拦、够用放行），不是只认 Java 8。
    #[test]
    fn lower_bound_covers_every_major() {
        let modern = "net.minecraft.client.main.Main";
        let cases = [
            (17, 8, true),
            (17, 11, true),
            (17, 17, false),
            (17, 21, false),
            (17, 25, false),
            (21, 17, true),
            (21, 21, false),
            (21, 25, false),
            (25, 21, true),
            (25, 25, false),
            (11, 8, true),
            (16, 8, true),
            (16, 17, false),
            (8, 8, false),
            (0, 8, false),
        ];
        for (required, selected, expect_block) in cases {
            let vj = version_json(modern, required, &[]);
            let msg = blocked(check(&vj, selected, "/jdk", false));
            assert_eq!(
                msg.is_some(),
                expect_block,
                "要求 Java {required}、所选 Java {selected}: {msg:?}"
            );
        }
    }

    /// 上界只对跨 8→9 的老工具链生效：主类与加载器决定拦还是提示。
    #[test]
    fn upper_bound_only_for_java8_toolchains() {
        let lw = version_json("net.minecraft.launchwrapper.Launch", 8, &[]);
        for major in [9, 11, 17, 21, 25] {
            assert!(
                blocked(check(&lw, major, "/jdk", false)).is_some(),
                "launchwrapper 在 Java {major} 上应拦"
            );
        }
        assert!(
            blocked(check(&lw, 8, "/jdk", false)).is_none(),
            "Java 8 放行"
        );
        assert!(
            blocked(check(&lw, 21, "/jdk", true)).is_none(),
            "禁用 launchwrapper 后不受此限"
        );

        // 1.13–1.16.5 的 Forge/OptiFine：官方只支持 Java 8，提示但不拦
        let era_forge = version_json(
            "cpw.mods.modlauncher.Launcher",
            8,
            &["net.minecraftforge:forge:36.2.0"],
        );
        assert!(warned(check(&era_forge, 17, "/jdk", false)).is_some());
        assert!(warned(check(&era_forge, 8, "/jdk", false)).is_none());

        // 现代版本用更新 Java、8 时代的原版/Fabric 用更新 Java：都不拦不提示
        let modern = version_json("net.minecraft.client.main.Main", 21, &[]);
        assert!(matches!(check(&modern, 25, "/jdk", false), JavaCompat::Ok));
        let era_fabric = version_json(
            "net.fabricmc.loader.impl.launch.knot.KnotClient",
            8,
            &["net.fabricmc:fabric-loader:0.14.21"],
        );
        assert!(matches!(
            check(&era_fabric, 17, "/jdk", false),
            JavaCompat::Ok
        ));
    }

    /// 下界优先于上界判定：launchwrapper 版本用 Java 8 以下（如 7）仍按过低拦。
    #[test]
    fn lower_bound_wins_over_launchwrapper() {
        let lw = version_json("net.minecraft.launchwrapper.Launch", 8, &[]);
        let msg = blocked(check(&lw, 7, "/jdk", false)).expect("Java 7 低于版本要求应拦");
        assert!(
            msg.starts_with("@i18n:errors.jdk.too_old?need=8&cur=7"),
            "{msg}"
        );
    }
}
