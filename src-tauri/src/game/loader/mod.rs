//! ModLoader 引擎：按种类安装（profile 式 Fabric/Quilt、installer 式 Forge/NeoForge、OptiFine 分场景）与版本列表查询。
//! 实现划分：`hook`（阶段事件/钩子）、`ctx`（安装参数）、`shared`（profile 合并/继承展开/幂等/进程管理）、
//! `dispatch`（统一入口）、`fabric`/`forge`/`neoforge`/`optifine`（各安装器）、`processors`、`version_query`。

pub mod fabric;
pub mod forge;
pub mod neoforge;
pub mod optifine;
pub mod processors;
pub mod version_query;

mod ctx;
mod dispatch;
mod hook;
mod shared;

pub use ctx::{InstallCtx, VersionTarget};
pub use dispatch::install_loader;
pub use fabric::{install_fabric, install_quilt};
pub use forge::install_forge;
pub use hook::{LoaderEvent, LoaderHook};
pub use neoforge::install_neoforge;
pub use optifine::{install_optifine, OptifineMode};
pub(crate) use shared::{
    expand_inheritance, merge_loader_profile, run_installer_command,
};


#[cfg(test)]
mod matrix {
    //! ModLoader 安装矩阵：**无需 UI 点击**，一条命令覆盖各代安装器形态（profile 式 Fabric/Quilt、spec-0 根
    //! universal、legacy maven、processors+maven、纯 processors）。
    //! 运行：`JAVA_HOME=<jdk> cargo test --lib -- --ignored --test-threads=1 matrix --nocapture`
    //!
    //! 报告：`src-tauri/target/e2e-loader/<case>.json` + 合并表 `report.md`。

    use std::path::{Path, PathBuf};
    use std::sync::atomic::AtomicBool;
    use std::sync::Arc;
    use std::time::Instant;

    use super::{install_loader, InstallCtx, VersionTarget};
    use crate::task::LoaderKind;
    use crate::config::downloader::DownloaderConfig;
    use crate::config::game_dirs;
    use crate::downloader::progress::ProgressCounters;
    use crate::game::loader::version_query;
    use crate::game::version::manifest::fetch_version_list;
    use crate::game::version::meta::fetch_version_json_text;

    /// 一个安装用例：`ver` 为空表示取该 loader 当前最新稳定版（Fabric/Quilt）。
    struct Case {
        name: &'static str,
        kind: &'static str,
        mc: &'static str,
        ver: &'static str,
    }

    const CASES: &[Case] = &[
        Case { name: "fabric-1.20.1", kind: "fabric", mc: "1.20.1", ver: "" },
        Case { name: "quilt-1.20.1", kind: "quilt", mc: "1.20.1", ver: "" },
        Case { name: "forge-1.7.10-spec0-root", kind: "forge", mc: "1.7.10", ver: "10.13.4.1614-1.7.10" },
        Case { name: "forge-1.8.9-spec0-root", kind: "forge", mc: "1.8.9", ver: "11.15.1.2318-1.8.9" },
        Case { name: "forge-1.12.2-legacy-maven", kind: "forge", mc: "1.12.2", ver: "14.23.5.2864" },
        Case { name: "forge-1.16.5-proc-maven", kind: "forge", mc: "1.16.5", ver: "36.2.34" },
        Case { name: "forge-1.20.1-proc", kind: "forge", mc: "1.20.1", ver: "47.1.0" },
        // NeoForge 支持自 MC 1.20.2 起（前端 LOADER_MIN_VERSION）；1.20.1 上的是另一坐标线（net.neoforged:forge），不在支持内
        Case { name: "neoforge-20.4.237", kind: "neoforge", mc: "1.20.4", ver: "20.4.237" },
        Case { name: "neoforge-1.21.1", kind: "neoforge", mc: "1.21.1", ver: "21.1.248" },
    ];

    fn out_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("e2e-loader")
    }

    fn kind_of(name: &str) -> LoaderKind {
        match name {
            "fabric" => LoaderKind::Fabric,
            "quilt" => LoaderKind::Quilt,
            "forge" => LoaderKind::Forge,
            _ => LoaderKind::NeoForge,
        }
    }

    /// 拉取并缓存原版版本 json + client.jar（跨用例复用同一 MC 的输入）。
    async fn vanilla_inputs(cfg: &DownloaderConfig, mc: &str) -> (String, PathBuf) {
        let dir = out_root().join("vanilla").join(mc);
        std::fs::create_dir_all(&dir).expect("创建缓存目录失败");
        let json_path = dir.join(format!("{mc}.json"));
        let jar_path = dir.join(format!("{mc}.jar"));

        let manifest = fetch_version_list(cfg).await.expect("拉取版本清单失败");
        let meta = manifest
            .versions
            .iter()
            .find(|v| v.id == mc)
            .unwrap_or_else(|| panic!("清单中找不到 {mc}"));
        let url = meta.url.clone();

        if !json_path.is_file() {
            let text = fetch_version_json_text(&url, cfg).await.expect("拉取版本 JSON 失败");
            std::fs::write(&json_path, &text).expect("写入版本 JSON 失败");
        }
        let text = std::fs::read_to_string(&json_path).expect("读版本 JSON 失败");
        let v: serde_json::Value = serde_json::from_str(&text).expect("解析版本 JSON 失败");
        let (cu, cs) = (
            v["downloads"]["client"]["url"].as_str().unwrap_or("").to_string(),
            v["downloads"]["client"]["sha1"].as_str().unwrap_or("").to_string(),
        );
        if !jar_path.is_file() && !cu.is_empty() {
            crate::downloader::fetcher::fetch_verified(&[cu], &jar_path, None::<fn(u64, u64)>, &cs)
                .await
                .expect("下载原版 client.jar 失败");
        }
        (text, jar_path)
    }

    /// 解析 loader 版本（Fabric/Quilt 取列表首个=最新稳定）。
    async fn resolve_ver(case: &Case) -> String {
        if !case.ver.is_empty() {
            return case.ver.to_string();
        }
        let list = match case.kind {
            "fabric" => version_query::fabric_versions(case.mc).await,
            "quilt" => version_query::quilt_versions(case.mc).await,
            other => panic!("{other} 需要显式版本"),
        }
        .unwrap_or_else(|e| panic!("拉取 {} 版本列表失败：{e}", case.kind));
        list.first()
            .cloned()
            .unwrap_or_else(|| panic!("{} 在 {} 无可用版本", case.kind, case.mc))
    }

    /// loader 自身相关的产物（libraries 下的 loader jar 清单）。
    fn loader_artifacts(mc_dir: &Path) -> Vec<(String, u64)> {
        let groups = [
            "net/minecraftforge",
            "net/neoforged",
            "net/fabricmc",
            "org/quiltmc",
        ];
        let mut out = Vec::new();
        for g in groups {
            collect_jars(&mc_dir.join("libraries").join(g), &mut out);
        }
        out.sort();
        out
    }

    fn collect_jars(dir: &Path, out: &mut Vec<(String, u64)>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                collect_jars(&p, out);
            } else if p.extension().and_then(|x| x.to_str()) == Some("jar") {
                let size = p.metadata().map(|m| m.len()).unwrap_or(0);
                out.push((p.to_string_lossy().replace('\\', "/"), size));
            }
        }
    }

    fn dir_bytes(dir: &Path) -> (usize, u64) {
        let mut files = 0usize;
        let mut bytes = 0u64;
        let Ok(entries) = std::fs::read_dir(dir) else {
            return (0, 0);
        };
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                let (f, b) = dir_bytes(&p);
                files += f;
                bytes += b;
            } else if let Ok(m) = p.metadata() {
                files += 1;
                bytes += m.len();
            }
        }
        (files, bytes)
    }

    /// 解析版本 JSON（真实模型）+ 构造启动命令，返回（摘要 JSON、检查项：mainClass/classpath/占位符/loader 关键 jar）。
    fn parse_and_plan(
        text: &str,
        dir_name: &str,
        mc_dir: &Path,
        kind: &str,
        mc: &str,
        ver: &str,
    ) -> Result<(serde_json::Value, serde_json::Value), String> {
        use crate::launcher::args::{build_launch_command, LaunchRequest};
        use crate::launcher::model::{AuthMode, LaunchOptions};
        use crate::game::version::model::VersionJson;

        let vj: VersionJson =
            serde_json::from_str(text).map_err(|e| format!("解析版本 JSON 失败\n{e}"))?;
        let opts = LaunchOptions::default();
        let auth = AuthMode::Offline {
            username: "E2ETester".to_string(),
            uuid: "00000000-0000-0000-0000-000000000001".to_string(),
        };
        let cmd = build_launch_command(
            Path::new("java"),
            &vj,
            &LaunchRequest {
                version_dir_name: dir_name,
                mc_dir,
                config: &opts,
                auth: &auth,
                java_major_version: 17,
                user_properties: "",
            },
        );
        let args = cmd.to_args();
        let cp = args
            .iter()
            .position(|a| a == "-cp" || a == "-classpath")
            .and_then(|i| args.get(i + 1))
            .cloned()
            .unwrap_or_default();
        let sep = if cfg!(windows) { ';' } else { ':' };
        let cp_entries = cp.split(sep).filter(|s| !s.is_empty()).count();
        let placeholders: Vec<String> = args
            .iter()
            .filter(|a| a.contains("${"))
            .take(3)
            .cloned()
            .collect();
        let keyword = loader_cp_keyword(kind, mc, ver);
        // 1.17+ Forge/NeoForge 本体不在 -cp 也不在 argv：走 `-DlibraryDirectory=${library_directory}` + bootstraplauncher（--add-modules ALL-MODULE-PATH）扫描库目录发现 forge-*-client.jar
        let loader_in = if cp.contains(&keyword) {
            "cp"
        } else if args.iter().any(|a| a.contains(&keyword)) {
            "module-path/argv"
        } else if args
            .iter()
            .any(|a| a.starts_with("-DlibraryDirectory=") || a == "--module-path" || a == "-p")
        {
            "libraryDirectory(启动器扫描库目录)"
        } else {
            "缺"
        };
        let v = serde_json::json!({
            "main_class": vj.main_class,
            "arg_count": args.len(),
            "cp_entries": cp_entries,
            "unresolved_placeholders": placeholders,
            "loader_jar_in": loader_in,
            "natives_flag": args.iter().any(|a| a.contains("java.library.path")),
        });
        Ok((serde_json::to_value(&vj).unwrap_or(serde_json::Value::Null), v))
    }

    /// classpath 里应出现的关键片段（按 loader 代际区分）。
    fn loader_cp_keyword(kind: &str, mc: &str, ver: &str) -> String {
        match kind {
            // 纯净版 OptiFine：库落在 libraries/<版本目录>/，必须进 classpath
            "optifine" => "OptiFine_".to_string(),
            "fabric" => "fabric-loader".to_string(),
            "quilt" => "quilt-loader".to_string(),
            "neoforge" => format!("neoforge-{ver}"),
            // 只判基名：老代是 universal 落位的普通 jar，1.14+ 由 processors 生成 `-client.jar`，两种都必须进 classpath
            _ => format!("forge-{mc}-{ver}"),
        }
    }

    /// 用例 installer.jar（缓存复用）：URL 按与生产同一套源顺序构造。
    async fn cached_installer(case: &Case, cfg: &DownloaderConfig, ver: &str) -> PathBuf {
        let sources = cfg.file_sources_list();
        let urls: Vec<String> = match case.kind {
            "forge" => sources
                .iter()
                .map(|s| s.url(&s.forge_installer_url, &[("mc", case.mc), ("ver", ver)]))
                .collect(),
            "neoforge" => sources
                .iter()
                .map(|s| s.url(&s.neoforge_installer_url, &[("ver", ver)]))
                .collect(),
            other => panic!("{other} 没有 installer.jar"),
        };
        let dest = out_root()
            .join("installers")
            .join(urls[0].rsplit('/').next().unwrap_or("installer.jar"));
        if !dest.is_file() {
            std::fs::create_dir_all(dest.parent().expect("缓存路径无父目录"))
                .expect("创建 installer 缓存目录失败");
            crate::downloader::fetcher::fetch_fallback(&urls, &dest, None::<fn(u64, u64)>)
                .await
                .unwrap_or_else(|e| panic!("下载 {} installer 失败：{e}", case.kind));
        }
        dest
    }

    /// install_profile 声明的可下载构件里，未落盘（或 sha1 不符）的清单。这是 Forge 1.17+ 运行时模块 jar（`fmlcore` /
    /// `javafmllanguage` / `lowcodelanguage` / `mclanguage`）**唯一**的声明处：version.json 只声明 fmlloader/fmlearlydisplay，
    /// 安装时若只下 version.json 的库，游戏启动会 `Invalid paths argument, contained no existing paths` 秒退（退出码 1）。
    fn missing_profile_libraries(installer: &Path, lib_dir: &Path) -> Vec<String> {
        let text = super::processors::read_profile_from_installer(installer)
            .unwrap_or_else(|e| panic!("读 install_profile.json 失败：{e}"));
        let value: serde_json::Value =
            serde_json::from_str(&text).unwrap_or_else(|e| panic!("解析 install_profile.json 失败：{e}"));
        super::forge::profile_libraries(&value)
            .into_iter()
            .filter_map(|lib| {
                let artifact = lib.downloads?.artifact?;
                let dest = lib_dir.join(&artifact.path);
                let ok = !artifact.sha1.is_empty()
                    && crate::downloader::hash::file_sha1_matches(&dest, &artifact.sha1);
                (!ok).then_some(artifact.path)
            })
            .collect()
    }

    /// 跑一个用例，返回结构化结果（失败也返回，供报告）。
    async fn run_case(case: &Case, cfg: &DownloaderConfig) -> serde_json::Value {
        let started = Instant::now();
        let ver = resolve_ver(case).await;
        let dir_name = format!("{}-{}-{}", case.mc, case.kind, ver);
        let case_dir = out_root().join("cases").join(case.name);
        let _ = std::fs::remove_dir_all(&case_dir);
        let mc_dir = case_dir.join(".minecraft");
        let vdir = mc_dir.join("versions").join(&dir_name);
        std::fs::create_dir_all(&vdir).expect("创建版本目录失败");

        let (parent_text, vanilla_jar) = vanilla_inputs(cfg, case.mc).await;
        // 原版 json 放到 versions/<mc>/（继承解析与 client jar 兜底都按父目录找）
        let parent_dir = mc_dir.join("versions").join(case.mc);
        std::fs::create_dir_all(&parent_dir).expect("创建父版本目录失败");
        std::fs::write(parent_dir.join(format!("{}.json", case.mc)), &parent_text)
            .expect("写父版本 JSON 失败");
        let _ = std::fs::copy(&vanilla_jar, parent_dir.join(format!("{}.jar", case.mc)));
        // install 步骤本身不等 runner：{MINECRAFT_JAR} 取目标版本目录同名 jar（生产由 runner 的 ClientJar 步骤并行写入，installer 侧 wait_for_vanilla_client 轮询该路径）
        let _ = std::fs::copy(&vanilla_jar, vdir.join(format!("{dir_name}.jar")));

        // 用例自持配置：minecraft_dir 指向本用例的 .minecraft（安装函数读 ctx.config，不能只改持久化配置）
        let mut case_cfg = cfg.clone();
        case_cfg.minecraft_dir = mc_dir.to_string_lossy().to_string();
        game_dirs::set_active(&case_cfg.minecraft_dir).expect("设置激活目录失败");

        let cancel = Arc::new(AtomicBool::new(false));
        let pc = ProgressCounters::default();
        let target = VersionTarget { dir_name: &dir_name, vdir: &vdir };
        let ctx = InstallCtx { config: &case_cfg, pc: &pc, cancel: &cancel, hook: None };

        let result = install_loader(
            kind_of(case.kind),
            case.mc,
            &ver,
            target,
            &parent_text,
            ctx,
        )
        .await;

        // 关键检查：按**真实模型**解析安装出的版本 JSON 并构造启动命令（1.7.10/1.8.9 的 "arguments": null 就在这一步暴露：装得下、读不开）
        let (vjson, launch_plan) = match std::fs::read_to_string(vdir.join(format!("{dir_name}.json")))
        {
            Ok(text) => match parse_and_plan(&text, &dir_name, &mc_dir, case.kind, case.mc, &ver) {
                Ok((v, plan)) => (Some(v), Some(plan)),
                Err(e) => (None, Some(serde_json::json!({ "error": e }))),
            },
            Err(e) => (None, Some(serde_json::json!({ "error": format!("读版本 JSON 失败: {e}") }))),
        };
        if let Some(plan) = &launch_plan {
            if plan.get("error").is_some() {
                return serde_json::json!({
                    "case": case.name, "kind": case.kind, "mc": case.mc, "ver": ver,
                    "ok": false,
                    // 安装错误优先（读不到 JSON 时它才是根因）
                    "error": result.as_ref().err().map(|e| serde_json::Value::from(e.clone()))
                        .unwrap_or_else(|| plan["error"].clone()),
                    "first_line": result.as_ref().err()
                        .map(|e| e.lines().next().unwrap_or("").to_string())
                        .unwrap_or_else(|| plan["error"].as_str().map(|s| s.lines().next().unwrap_or("").to_string()).unwrap_or_default()),
                    "plan_error": plan["error"],
                    "elapsed_s": (started.elapsed().as_secs_f64() * 10.0).round() / 10.0,
                    "version_dir": dir_name, "lib_files": 0, "lib_bytes": 0, "artifacts": [],
                    "launch_plan": plan,
                });
            }
        }
        let (lib_files, lib_bytes) = dir_bytes(&mc_dir.join("libraries"));
        // 安装器声明的构件（含 version.json 未声明的运行时模块 jar）必须全部落盘
        let missing_libs = match case.kind {
            "fabric" | "quilt" => Vec::new(),
            _ => {
                let installer = cached_installer(case, cfg, &ver).await;
                missing_profile_libraries(&installer, &mc_dir.join("libraries"))
            }
        };
        let ok = result.is_ok()
            && missing_libs.is_empty()
            && launch_plan.as_ref().map(|p| p.get("error").is_none()).unwrap_or(false);
        let artifacts = loader_artifacts(&mc_dir);

        serde_json::json!({
            "case": case.name,
            "kind": case.kind,
            "mc": case.mc,
            "ver": ver,
            "ok": ok,
            "error": result.as_ref().err().cloned(),
            "first_line": result.as_ref().err().map(|e| e.lines().next().unwrap_or("").to_string()),
            "elapsed_s": (started.elapsed().as_secs_f64() * 10.0).round() / 10.0,
            "version_dir": dir_name,
            "version_json": vjson.as_ref().map(|v| serde_json::json!({
                "id": v["id"], "mainClass": v["mainClass"], "inheritsFrom": v["inheritsFrom"],
                "libraries": v["libraries"].as_array().map(|a| a.len()),
            })),
            "lib_files": lib_files,
            "lib_bytes": lib_bytes,
            "missing_libs": missing_libs,
            "artifacts": artifacts.iter().map(|(p, s)| serde_json::json!({"path": p, "size": s})).collect::<Vec<_>>(),
            "launch_plan": launch_plan,
        })
    }

    fn write_report(rows: &[serde_json::Value], label: &str) {
        let dir = out_root();
        std::fs::create_dir_all(&dir).expect("创建报告目录失败");
        for r in rows {
            let name = r["case"].as_str().unwrap_or("unknown");
            let pretty = serde_json::to_string_pretty(r).unwrap_or_default();
            let _ = std::fs::write(dir.join(format!("{name}.json")), pretty);
        }
        let mut md = String::from("| 用例 | loader | MC | 版本 | 结果 | 耗时(s) | libraries | 启动计划(cp/占位符) | loader 产物 |\n|---|---|---|---|---|---|---|---|---|\n");
        for r in rows {
            let arts = r["artifacts"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .map(|x| {
                            let p = x["path"].as_str().unwrap_or("");
                            let short = p.rsplit(['/', '\\']).next().unwrap_or(p).to_string();
                            let kb = x["size"].as_u64().unwrap_or(0) / 1024;
                            format!("{short} ({kb}KB)")
                        })
                        .collect::<Vec<_>>()
                        .join("; ")
                })
                .unwrap_or_default();
            md.push_str(&format!(
                "| {} | {} | {} | {} | {} | {} | {} 个 / {:.1}MB | {} | {} |\n",
                r["case"].as_str().unwrap_or(""),
                r["kind"].as_str().unwrap_or(""),
                r["mc"].as_str().unwrap_or(""),
                r["ver"].as_str().unwrap_or(""),
                if r["ok"].as_bool().unwrap_or(false) { "✅" } else { "❌" },
                r["elapsed_s"],
                r["lib_files"],
                r["lib_bytes"].as_u64().unwrap_or(0) as f64 / 1e6,
                if arts.is_empty() { "—" } else { &arts },
                r["launch_plan"]
                    .as_object()
                    .map(|_| {
                        if let Some(e) = r["launch_plan"]["error"].as_str() {
                            format!("解析/构造失败: {}", e.lines().next().unwrap_or(""))
                        } else {
                            format!(
                                "cp {} 项 | 占位符残留 {} | loader 本体在: {} | natives: {}",
                                r["launch_plan"]["cp_entries"],
                                r["launch_plan"]["unresolved_placeholders"].as_array().map(|a| a.len()).unwrap_or(0),
                                r["launch_plan"]["loader_jar_in"],
                                r["launch_plan"]["natives_flag"],
                            )
                        }
                    })
                    .unwrap_or_else(|| "—".to_string()),
            ));
            if !r["ok"].as_bool().unwrap_or(false) {
                md.push_str(&format!(
                    "\n> ❌ {} 失败：{} | {}: {} |\n",
                    r["case"].as_str().unwrap_or(""),
                    r["first_line"].as_str().unwrap_or(""),
                    if r["error"].is_null() { "无" } else { "细节" },
                    r["error"].as_str().unwrap_or("").replace('\n', " ⏎ "),
                ));
                let missing = r["missing_libs"].as_array().cloned().unwrap_or_default();
                if !missing.is_empty() {
                    md.push_str(&format!(
                        "> ⚠️ installer 声明的构件未落盘 {} 项：{}\n",
                        missing.len(),
                        missing
                            .iter()
                            .map(|m| m.as_str().unwrap_or(""))
                            .collect::<Vec<_>>()
                            .join("; "),
                    ));
                }
            }
        }
        let _ = std::fs::write(dir.join(format!("report-{label}.md")), &md);
        eprintln!("\n📋 报告：{}", dir.join(format!("report-{label}.md")).display());
        eprintln!("{md}");
    }

    /// 全矩阵：一次跑完所有形态（真实网络；Forge/NeoForge 需要 JAVA_HOME）。
    #[tokio::test(flavor = "multi_thread")]
    #[ignore]
    async fn loader_install_matrix() {
        let cfg = DownloaderConfig::default();
        // ELY_E2E_CASES=fabric-1.20.1,forge-1.20.1 可只跑指定用例（逗号分隔，便于重跑单例）
        let only: Vec<String> = std::env::var("ELY_E2E_CASES")
            .unwrap_or_default()
            .split(',')
            .filter(|x| !x.trim().is_empty())
            .map(|x| x.trim().to_string())
            .collect();
        let cases: Vec<&Case> = CASES
            .iter()
            .filter(|c| only.is_empty() || only.iter().any(|o| o == c.name))
            .collect();
        assert!(!cases.is_empty(), "ELY_E2E_CASES 未匹配任何用例");
        let mut rows = Vec::new();
        for case in cases {
            eprintln!("\n=== {} ===", case.name);
            let row = run_case(case, &cfg).await;
            eprintln!(
                "{} {}（{}s）",
                if row["ok"].as_bool().unwrap_or(false) { "✅" } else { "❌" },
                case.name,
                row["elapsed_s"]
            );
            if !row["ok"].as_bool().unwrap_or(false) {
                eprintln!("   首行：{}", row["first_line"].as_str().unwrap_or(""));
            }
            rows.push(row);
            write_report(&rows, "loader");
        }
        let failed: Vec<&str> = rows
            .iter()
            .filter(|r| !r["ok"].as_bool().unwrap_or(false))
            .filter_map(|r| r["case"].as_str())
            .collect();
        assert!(failed.is_empty(), "以下用例失败：{failed:?}");
    }

    // ─────────────────────────── OptiFine 矩阵 ───────────────────────────

    /// OptiFine 用例：mode 决定落位（纯净版 → 独立版本 + classpath 库；Forge/Fabric 组合 → mods/）。`pre` = 先装的 ModLoader。
    struct OfCase {
        name: &'static str,
        mc: &'static str,
        mode: &'static str,
        pre: Option<(&'static str, &'static str)>,
    }

    /// OptiFine 类型段（BMCLAPI 列表全为 HD_U；preview 已在列表侧过滤）。
    const OF_TYPE: &str = "HD_U";

    const OF_CASES: &[OfCase] = &[
        OfCase { name: "optifine-1.20.1-vanilla", mc: "1.20.1", mode: "vanilla", pre: None },
        OfCase {
            name: "optifine-1.20.1-forge",
            mc: "1.20.1",
            mode: "forge",
            pre: Some(("forge", "47.1.0")),
        },
        OfCase {
            name: "optifine-1.20.1-fabric",
            mc: "1.20.1",
            mode: "fabric",
            pre: Some(("fabric", "")),
        },
        OfCase { name: "optifine-1.12.2-vanilla", mc: "1.12.2", mode: "vanilla", pre: None },
        OfCase {
            name: "optifine-1.12.2-forge",
            mc: "1.12.2",
            mode: "forge",
            pre: Some(("forge", "14.23.5.2864")),
        },
    ];

    /// OptiFine 最新稳定 patch（列表已过滤 preview）。
    async fn optifine_patch(mc: &str) -> String {
        version_query::optifine_versions(mc)
            .await
            .unwrap_or_else(|e| panic!("拉取 OptiFine 列表失败（{mc}）：{e}"))
            .into_iter()
            .next()
            .unwrap_or_else(|| panic!("{mc} 无 OptiFine 稳定版"))
    }

    /// 目录下 jar 清单（mods/ 或 libraries/<dir>/）。
    fn jars_in(dir: &Path) -> Vec<(String, u64)> {
        let mut out = Vec::new();
        collect_jars(dir, &mut out);
        out.sort();
        out
    }

    async fn run_optifine_case(case: &OfCase, cfg: &DownloaderConfig) -> serde_json::Value {
        let started = Instant::now();
        let patch = optifine_patch(case.mc).await;
        // 落位版本目录：组合场景 = loader 的版本目录；纯净版 = OptiFine 自己的独立版本
        let (dir_name, pre_kind, pre_ver) = match case.pre {
            Some((kind, ver)) => {
                let ver = if ver.is_empty() {
                    resolve_ver(&Case { name: case.name, kind, mc: case.mc, ver: "" }).await
                } else {
                    ver.to_string()
                };
                (format!("{}-{}-{}", case.mc, kind, ver), Some(kind), Some(ver))
            }
            None => (format!("{}-optifine-{}", case.mc, patch), None, None),
        };
        let case_dir = out_root().join("cases").join(case.name);
        let _ = std::fs::remove_dir_all(&case_dir);
        let mc_dir = case_dir.join(".minecraft");
        let vdir = mc_dir.join("versions").join(&dir_name);
        std::fs::create_dir_all(&vdir).expect("创建版本目录失败");

        let (parent_text, vanilla_jar) = vanilla_inputs(cfg, case.mc).await;
        let parent_dir = mc_dir.join("versions").join(case.mc);
        std::fs::create_dir_all(&parent_dir).expect("创建父版本目录失败");
        std::fs::write(parent_dir.join(format!("{}.json", case.mc)), &parent_text).expect("写父 JSON");
        let _ = std::fs::copy(&vanilla_jar, parent_dir.join(format!("{}.jar", case.mc)));
        let _ = std::fs::copy(&vanilla_jar, vdir.join(format!("{dir_name}.jar")));

        let mut case_cfg = cfg.clone();
        case_cfg.minecraft_dir = mc_dir.to_string_lossy().to_string();
        game_dirs::set_active(&case_cfg.minecraft_dir).expect("设置激活目录失败");
        let cancel = Arc::new(AtomicBool::new(false));
        let pc = ProgressCounters::default();

        // 1) 组合场景先装 loader（与真实流程一致）
        let mut pre_err: Option<String> = None;
        if let (Some(kind), Some(ver)) = (pre_kind, pre_ver.as_deref()) {
            let r = install_loader(
                kind_of(kind),
                case.mc,
                ver,
                VersionTarget { dir_name: &dir_name, vdir: &vdir },
                &parent_text,
                InstallCtx { config: &case_cfg, pc: &pc, cancel: &cancel, hook: None },
            )
            .await;
            if let Err(e) = r {
                pre_err = Some(format!("先装 {kind} {ver} 失败：{}", e.lines().next().unwrap_or("")));
            }
        }

        // 2) OptiFine
        let mode = match case.mode {
            "forge" => super::OptifineMode::ForgeLike,
            "fabric" => super::OptifineMode::FabricLike,
            _ => super::OptifineMode::Vanilla,
        };
        let of_result = if pre_err.is_some() {
            Err(pre_err.clone().unwrap_or_default())
        } else {
            super::install_optifine(
                case.mc,
                &patch,
                OF_TYPE,
                VersionTarget { dir_name: &dir_name, vdir: &vdir },
                mode,
                InstallCtx { config: &case_cfg, pc: &pc, cancel: &cancel, hook: None },
            )
            .await
        };

        // 3) 证据：组合场景看 mods/，纯净版看版本目录 + libraries
        let mods = jars_in(&vdir.join("mods"));
        // 与产品同构：OptiFine_{mc}_{of_type}_{patch}.jar
        let of_name = format!("OptiFine_{}_{}_{}.jar", case.mc, OF_TYPE, patch);
        let (of_jar_present, placement) = match case.pre {
            Some(_) => (
                mods.iter().any(|(p, _)| p.ends_with(&of_name)),
                format!("mods/（{} 个 jar）", mods.len()),
            ),
            None => {
                let lib = mc_dir.join("libraries").join(&dir_name).join(&of_name);
                (lib.is_file(), format!("libraries/{}/", dir_name))
            }
        };
        // 启动计划：纯净版 json 是"继承式 profile"（只有 id/mainClass/libraries/inheritsFrom），生产里由 launcher::env 展开继承后再解析 → 这里照做
        let plan = match std::fs::read_to_string(vdir.join(format!("{dir_name}.json"))) {
            Ok(text) if case.pre.is_none() => {
                match serde_json::from_str::<serde_json::Value>(&text)
                    .map_err(|e| format!("解析失败\n{e}"))
                    .and_then(|child| super::expand_inheritance(&parent_text, child, &dir_name))
                    .and_then(|merged| {
                        serde_json::to_string(&merged).map_err(|e| format!("序列化失败\n{e}"))
                    }) {
                    Ok(merged_text) => {
                        match parse_and_plan(&merged_text, &dir_name, &mc_dir, "optifine", case.mc, &patch) {
                            Ok((_, p)) => p,
                            Err(e) => serde_json::json!({"error": e}),
                        }
                    }
                    Err(e) => serde_json::json!({"error": e}),
                }
            }
            Ok(text) => {
                // 组合场景：OptiFine 在 mods/，loader 的版本 json 不变 → 仍检查它可读可启动
                match parse_and_plan(
                    &text,
                    &dir_name,
                    &mc_dir,
                    pre_kind.unwrap_or("forge"),
                    case.mc,
                    pre_ver.as_deref().unwrap_or(""),
                ) {
                    Ok((_, p)) => p,
                    Err(e) => serde_json::json!({"error": e}),
                }
            }
            Err(e) => serde_json::json!({"error": format!("缺版本 JSON: {e}")}),
        };
        let (lib_files, lib_bytes) = dir_bytes(&mc_dir.join("libraries"));
        // 失败原因必须可读：无错误但检查不过时，明确指出是哪个检查
        let check_err = if !of_jar_present {
            Some(format!("OptiFine 未落位（期望 {of_name}，落位方式 {placement}）"))
        } else if plan.get("error").is_some() {
            plan["error"].as_str().map(|e| e.to_string())
        } else {
            None
        };
        let err = of_result.as_ref().err().cloned().or(check_err.clone());
        let ok = of_result.is_ok() && check_err.is_none();
        serde_json::json!({
            "case": case.name, "kind": format!("optifine({})", case.mode), "mc": case.mc,
            "ver": format!("{patch} ({})", if case.pre.is_some() { "组合" } else { "纯净" }),
            "ok": ok,
            "error": err,
            "first_line": check_err.as_deref().map(|e| e.lines().next().unwrap_or("").to_string())
                .or_else(|| of_result.as_ref().err().map(|e| e.lines().next().unwrap_or("").to_string())),
            "elapsed_s": (started.elapsed().as_secs_f64() * 10.0).round() / 10.0,
            "version_dir": dir_name,
            "lib_files": lib_files,
            "lib_bytes": lib_bytes,
            "artifacts": match case.pre {
                Some(_) => mods.iter().map(|(p, sz)| serde_json::json!({"path": p, "size": sz})).collect::<Vec<_>>(),
                None => vec![serde_json::json!({"path": format!("libraries/{dir_name}/{of_name}"), "size": mc_dir.join("libraries").join(&dir_name).join(&of_name).metadata().map(|m| m.len()).unwrap_or(0)})],
            },
            "placement": placement,
            "launch_plan": plan,
            "optifine_jar_placed": of_jar_present,
            "optifine_jar": of_name,
            "mods": mods.iter().map(|(p, s)| serde_json::json!({"path": p, "size": s})).collect::<Vec<_>>(),
        })
    }

    /// OptiFine 矩阵：纯净版 + Forge/Fabric 组合（真实网络；Fabric 组合需 CurseForge 取 OptiFabric）。
    #[tokio::test(flavor = "multi_thread")]
    #[ignore]
    async fn optifine_matrix() {
        let cfg = DownloaderConfig::default();
        let mut rows = Vec::new();
        for case in OF_CASES {
            eprintln!("\n=== {} ===", case.name);
            let row = run_optifine_case(case, &cfg).await;
            eprintln!(
                "{} {}（{}s）{}",
                if row["ok"].as_bool().unwrap_or(false) { "✅" } else { "❌" },
                case.name,
                row["elapsed_s"],
                if row["optifine_jar_placed"].as_bool().unwrap_or(false) { "OptiFine 已落位" } else { "OptiFine 未落位" }
            );
            if !row["ok"].as_bool().unwrap_or(false) {
                eprintln!("   首行：{}", row["first_line"].as_str().unwrap_or(""));
            }
            rows.push(row);
            write_report(&rows, "optifine");
        }
        let failed: Vec<&str> = rows
            .iter()
            .filter(|r| !r["ok"].as_bool().unwrap_or(false))
            .filter_map(|r| r["case"].as_str())
            .collect();
        assert!(failed.is_empty(), "以下用例失败：{failed:?}");
    }
}
