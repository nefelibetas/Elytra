//! 整合包清单解析、格式嗅探与安装计划（Modrinth / CurseForge / MultiMC / HMCL / MCBBS / PCL / 完整目录）。
//!
//! 嗅探只看标记文件，不看扩展名；各格式统一产出 `PackPlan`（文件列表 + 覆盖前缀 + loader）。
//! PCL 带启动器包递归解析内嵌包；导出侧固定为标准 MRPACK。

mod curseforge;
mod detect;
mod directory;
mod export;
mod hmcl;
mod mcbbs;
mod model;
mod modrinth;
mod multimc;
mod plan;
mod version_meta;

pub use curseforge::{parse_curseforge_manifest, resolve_cf_files};
pub(crate) use export::{pack_modpack, resolve_hosted_mods, resolve_version_meta};
pub use model::{
    ModpackRequest, OverridesRef, PackFile, PackKind, PackMetaInfo, PackPlan, PackSource,
};
pub use modrinth::{parse_modrinth_index, ModrinthParsed};
pub(crate) use plan::cache_dir;
pub use plan::{assemble_plan, build_plan, cache_key, PackMeta};
pub(crate) use version_meta::parse_optifine_version;

#[cfg(test)]
mod tests {
    use super::detect::detect_pack;
    use super::*;
    use crate::task::LoaderKind;
    use std::io::Write as _;
    use std::path::Path;

    const MRPACK_INDEX: &str = r#"{
        "formatVersion": 1,
        "game": "minecraft",
        "versionId": "v1.0",
        "name": "Demo Pack",
        "files": [
            {
                "path": "mods/sodium.jar",
                "hashes": {"sha1": "aaa", "sha512": "xxx"},
                "env": {"client": "required", "server": "required"},
                "downloads": ["https://cdn.modrinth.com/sodium.jar"],
                "fileSize": 1234
            },
            {
                "path": "mods/serveronly.jar",
                "hashes": {"sha1": "bbb"},
                "env": {"client": "unsupported", "server": "required"},
                "downloads": ["https://cdn.modrinth.com/serveronly.jar"],
                "fileSize": 10
            },
            {
                "path": "config/opt.cfg",
                "downloads": ["https://cdn.modrinth.com/opt.cfg"],
                "fileSize": 5
            }
        ],
        "dependencies": {"minecraft": "1.21.1", "fabric-loader": "0.16.9"}
    }"#;

    #[test]
    fn parse_modrinth_index_filters_env_and_maps_loader() {
        let p = parse_modrinth_index(MRPACK_INDEX).unwrap();
        assert_eq!(p.mc_version, "1.21.1");
        let l = p.loader.expect("应识别 fabric-loader");
        assert_eq!(l.kind, LoaderKind::Fabric);
        assert_eq!(l.version, "0.16.9");
        assert_eq!(p.files.len(), 2, "server-only 与无 env 各留其一");
        assert_eq!(p.files[0].path, "mods/sodium.jar");
        assert_eq!(p.files[0].sha1, "aaa");
        assert_eq!(p.files[1].path, "config/opt.cfg");
    }

    #[test]
    fn parse_modrinth_rejects_wrong_format_and_unknown_loader() {
        assert!(parse_modrinth_index("{\"formatVersion\": 2}").is_err());
        assert!(parse_modrinth_index(
            r#"{"formatVersion":1,"game":"minecraft","dependencies":{"minecraft":"1.20.1","forge":"14.23.5.2860"},"files":[]}"#
        )
        .map(|p| {
            assert_eq!(p.loader.unwrap().kind, LoaderKind::Forge);
        })
        .is_ok());
        let bad = r#"{"formatVersion":1,"game":"minecraft","dependencies":{"minecraft":"1.20.1","rift":"1.0"},"files":[]}"#;
        let parsed = parse_modrinth_index(bad).unwrap();
        assert!(parsed.loader.is_none(), "未知 loader 键应忽略而非失败");
    }

    #[test]
    fn parse_curseforge_manifest_maps_loaders_and_ids() {
        let text = r#"{
            "manifestType": "minecraftModpack",
            "manifestVersion": 1,
            "name": "CF Demo",
            "minecraft": {
                "version": "1.20.1",
                "modLoaders": [{"id": "forge-47.3.0", "primary": true}]
            },
            "files": [
                {"projectID": 306612, "fileID": 4631262, "required": true},
                {"projectID": 2222, "fileID": 3333, "required": true}
            ]
        }"#;
        let p = parse_curseforge_manifest(text).unwrap();
        assert_eq!(p.mc_version, "1.20.1");
        let l = p.loader.unwrap();
        assert_eq!(l.kind, LoaderKind::Forge);
        assert_eq!(l.version, "47.3.0");
        assert_eq!(p.files.len(), 2);
        assert_eq!(
            (p.files[0].project_id, p.files[0].file_id),
            (306612, 4631262)
        );
        assert_eq!((p.files[1].project_id, p.files[1].file_id), (2222, 3333));
        assert_eq!(p.overrides_dir, "overrides");
        assert_eq!(p.meta.name, "CF Demo");

        let fabric = r#"{"manifestType":"minecraftModpack","manifestVersion":1,"minecraft":{"version":"1.21","modLoaders":[{"id":"neoforge-21.1.95"}]},"files":[]}"#;
        let p2 = parse_curseforge_manifest(fabric).unwrap();
        assert_eq!(p2.loader.unwrap().kind, LoaderKind::NeoForge);
        assert!(parse_curseforge_manifest(r#"{"manifestType":"other"}"#).is_err());
    }

    #[test]
    fn resolve_cf_files_splits_restricted_and_hashes() {
        let data = serde_json::json!([
            {
                "id": 4631262,
                "fileName": "jei.jar",
                "downloadUrl": "https://edge.forgecdn.net/files/4631/262/jei.jar",
                "fileLength": 4096,
                "hashes": [
                    {"algo": 2, "value": "md5-ignored"},
                    {"algo": 1, "value": "sha1-ok"}
                ]
            },
            {
                "id": 3333,
                "fileName": "locked.jar",
                "downloadUrl": null
            }
        ]);
        let (files, restricted) =
            resolve_cf_files(data.as_array().unwrap(), &[(1, 4631262), (2, 3333)]);
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, "mods/jei.jar");
        assert_eq!(files[0].sha1, "sha1-ok");
        assert_eq!(files[0].size, 4096);
        assert_eq!(restricted, vec!["locked.jar".to_string()]);
    }

    /// 构造测试 mrpack zip：modrinth.index.json + overrides 内容（assemble_plan 单测用）。
    fn write_mrpack_zip(path: &Path) {
        let file = std::fs::File::create(path).unwrap();
        let mut w = zip::ZipWriter::new(file);
        let opts = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        w.start_file("modrinth.index.json", opts).unwrap();
        w.write_all(MRPACK_INDEX.as_bytes()).unwrap();
        w.start_file("overrides/mods/a.jar", opts).unwrap();
        w.write_all(b"jar-bytes").unwrap();
        w.finish().unwrap();
    }

    /// assemble_plan：临时 zip + 清单文本 → 计划字段，冲突 `_{n}` 后缀，空内容报错。
    #[test]
    fn assemble_plan_builds_fields_and_suffixes_conflicts() {
        let dir = tempfile::tempdir().unwrap();
        let mc = tempfile::tempdir().unwrap();
        let zip_path = dir.path().join("a.mrpack");
        write_mrpack_zip(&zip_path);

        let mut ar = zip::ZipArchive::new(std::fs::File::open(&zip_path).unwrap()).unwrap();
        let detected = detect_pack(&mut ar).unwrap();
        assert_eq!(detected.kind, PackKind::Modrinth, "标记嗅探应识别 Modrinth");
        assert_eq!(detected.base, "");
        let text = detected.manifest.clone();
        drop(ar);
        let parsed = parse_modrinth_index(&text).unwrap();

        let plan = assemble_plan(PackMeta {
            pack_name: r#"Demo <Pack>"#,
            cache_key: "key-a",
            mc_version: " 1.21.1 ",
            loader: parsed.loader,
            optifine: None,
            files: parsed.files,
            restricted: Vec::new(),
            overrides: vec![OverridesRef {
                zip_path: zip_path.display().to_string(),
                prefix: "overrides/".into(),
                skip: Vec::new(),
            }],
            version_url: "https://piston-meta/v1.json".into(),
            minecraft_dir: mc.path(),
            kind: PackKind::Modrinth,
            meta: PackMetaInfo::default(),
        })
        .unwrap();
        assert_eq!(plan.name, r#"Demo <Pack>"#, "展示名原样保留");
        assert_eq!(plan.mc_version, "1.21.1", "版本号两侧空白应 trim");
        assert!(plan.loader.is_some(), "loader 透传");
        assert_eq!(plan.version_url, "https://piston-meta/v1.json");
        assert_eq!(
            plan.version_dir_name, "Demo _Pack_-1.21.1",
            "非法字符替换 '_' 后拼 净化名-mc",
        );
        assert_eq!(plan.files.len(), 2);
        assert!(plan.restricted.is_empty());
        assert_eq!(plan.overrides.len(), 1, "传入的覆盖目录应保留");
        assert_eq!(plan.overrides[0].prefix, "overrides/");
        assert_eq!(plan.overrides[0].zip_path, zip_path.display().to_string());

        std::fs::create_dir_all(mc.path().join("versions").join("Demo _Pack_-1.21.1")).unwrap();
        let reparsed = parse_modrinth_index(&text).unwrap();
        let plan2 = assemble_plan(PackMeta {
            pack_name: r#"Demo <Pack>"#,
            cache_key: "key-b",
            mc_version: "1.21.1",
            loader: reparsed.loader,
            optifine: None,
            files: reparsed.files,
            restricted: Vec::new(),
            overrides: Vec::new(),
            version_url: "u".into(),
            minecraft_dir: mc.path(),
            kind: PackKind::Modrinth,
            meta: PackMetaInfo::default(),
        })
        .unwrap();
        assert_eq!(plan2.version_dir_name, "Demo _Pack_-1.21.1_1");

        let p = parse_modrinth_index(
            r#"{"formatVersion":1,"game":"minecraft","dependencies":{"minecraft":"1.20.1"},"files":[]}"#,
        )
        .unwrap();
        let err = assemble_plan(PackMeta {
            pack_name: "E",
            cache_key: "key-c",
            mc_version: "1.20.1",
            loader: p.loader,
            optifine: None,
            files: p.files,
            restricted: Vec::new(),
            overrides: Vec::new(),
            version_url: "u".into(),
            minecraft_dir: mc.path(),
            kind: PackKind::Modrinth,
            meta: PackMetaInfo::default(),
        })
        .unwrap_err();
        assert!(err.contains("没有可安装的内容"), "空内容应报错: {err}");
    }

    /// 包名净化走 util::sanitize_fs_name：Windows 非法字符替换 `_`，全非法字符与保留名回退 "modpack"。
    #[test]
    fn plan_naming_falls_back_on_illegal_or_reserved_names() {
        let dir = tempfile::tempdir().unwrap();
        let mc = tempfile::tempdir().unwrap();
        let zip_path = dir.path().join("a.mrpack");
        write_mrpack_zip(&zip_path);
        let mk = |name: &str| {
            let parsed = parse_modrinth_index(MRPACK_INDEX).unwrap();
            assemble_plan(PackMeta {
                pack_name: name,
                cache_key: "key-d",
                mc_version: "1.21.1",
                loader: parsed.loader,
                optifine: None,
                files: parsed.files,
                restricted: Vec::new(),
                overrides: Vec::new(),
                version_url: "u".into(),
                minecraft_dir: mc.path(),
                kind: PackKind::Modrinth,
                meta: PackMetaInfo::default(),
            })
            .unwrap()
            .version_dir_name
        };
        assert!(mk(r#"My<Pack>:"X""#).starts_with("My_Pack___X_-"));
        assert_eq!(mk("***"), "modpack-1.21.1", "全非法字符回退 modpack");
        assert_eq!(mk("CON"), "modpack-1.21.1", "Windows 保留名回退 modpack");
        assert_eq!(mk("  tail.  "), "tail-1.21.1", "去尾点");
    }

    /// 同 cache_key 重装复用标记目录（更新语义，sha1 跳过才生效）；不同包走常规 `{净化名}-{mc}` 命名。
    #[test]
    fn assemble_plan_reuses_dir_marked_with_same_cache_key() {
        let dir = tempfile::tempdir().unwrap();
        let mc = tempfile::tempdir().unwrap();
        let zip_path = dir.path().join("a.mrpack");
        write_mrpack_zip(&zip_path);
        let mk = |key: &str| {
            let parsed = parse_modrinth_index(MRPACK_INDEX).unwrap();
            assemble_plan(PackMeta {
                pack_name: "Demo Pack",
                cache_key: key,
                mc_version: "1.21.1",
                loader: parsed.loader,
                optifine: None,
                files: parsed.files,
                restricted: Vec::new(),
                overrides: Vec::new(),
                version_url: "u".into(),
                minecraft_dir: mc.path(),
                kind: PackKind::Modrinth,
                meta: PackMetaInfo::default(),
            })
            .unwrap()
            .version_dir_name
        };

        let existing = mc.path().join("versions").join("Demo-1.21.1_3");
        std::fs::create_dir_all(&existing).unwrap();
        std::fs::write(
            existing.join("elytra-pack.json"),
            r#"{"cache_key":"key-same"}"#,
        )
        .unwrap();

        assert_eq!(
            mk("key-same"),
            "Demo-1.21.1_3",
            "同包重装应复用原目录（更新语义）"
        );
        assert_eq!(
            mk("key-other"),
            "Demo Pack-1.21.1",
            "不同包不受标记影响，走常规命名"
        );
    }

    /// install_modpack 入参契约：`{ request: { name, source } }`，`source` 为 serde 内部标签（`type: "local" | "online"`）；Windows 反斜杠路径必须原样反序列化。
    #[test]
    fn request_deserializes_frontend_payload() {
        let req: ModpackRequest = serde_json::from_str(
            r#"{"name":"愚者0.3.2","source":{"type":"local","path":"C:\\environment\\愚者0.3.2.zip"}}"#,
        )
        .expect("前端本地包载荷应能反序列化");
        match req.source {
            PackSource::Local { path } => assert_eq!(path, r"C:\environment\愚者0.3.2.zip"),
            other => panic!("来源类型应为 local：{other:?}"),
        }

        let online: ModpackRequest = serde_json::from_str(
            r#"{"name":"P","source":{"type":"online","url":"https://x/a.mrpack","sha1":"","size":0},"icon_url":"https://x/i.png"}"#,
        )
        .expect("在线包载荷应能反序列化");
        assert!(matches!(online.source, PackSource::Online { .. }));
        assert_eq!(online.icon_url.as_deref(), Some("https://x/i.png"));
    }

    // ── 嗅探与格式解析 ──

    fn write_zip(path: &Path, entries: &[(&str, &[u8])]) {
        let file = std::fs::File::create(path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        let opts = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        for (name, data) in entries {
            writer.start_file(*name, opts).unwrap();
            writer.write_all(data).unwrap();
        }
        writer.finish().unwrap();
    }

    fn detect_zip(path: &Path) -> super::detect::Detected {
        let mut archive = zip::ZipArchive::new(std::fs::File::open(path).unwrap()).unwrap();
        detect_pack(&mut archive).unwrap()
    }

    fn detect_zip_error(path: &Path) -> String {
        let mut archive = zip::ZipArchive::new(std::fs::File::open(path).unwrap()).unwrap();
        match detect_pack(&mut archive) {
            Ok(_) => panic!("应嗅探失败"),
            Err(error) => error,
        }
    }

    #[test]
    fn detect_nested_base_and_keeps_client_overrides_order() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("pack.zip");
        write_zip(
            &path,
            &[
                ("Pack/modrinth.index.json", MRPACK_INDEX.as_bytes()),
                ("Pack/overrides/config/a.txt", b"a"),
                ("Pack/client-overrides/config/a.txt", b"b"),
                ("__MACOSX/Pack/._x", b"junk"),
            ],
        );
        let detected = detect_zip(&path);
        assert_eq!(detected.kind, PackKind::Modrinth);
        assert_eq!(detected.base, "Pack/", "允许一层包装目录");

        let prefixes = vec![
            "Pack/overrides/".to_string(),
            "Pack/client-overrides/".to_string(),
        ];
        let overrides = super::detect::existing_overrides(&detected.members, "z", &prefixes);
        assert_eq!(overrides.len(), 2, "client-overrides 必须保留");
        assert_eq!(overrides[0].prefix, "Pack/overrides/");
        assert_eq!(overrides[1].prefix, "Pack/client-overrides/");
    }

    #[test]
    fn detect_rejects_multiple_packs_and_other_launchers() {
        let dir = tempfile::tempdir().unwrap();
        let multi = dir.path().join("multi.zip");
        write_zip(
            &multi,
            &[
                ("a/modrinth.index.json", MRPACK_INDEX.as_bytes()),
                ("b/modrinth.index.json", MRPACK_INDEX.as_bytes()),
            ],
        );
        assert!(detect_zip_error(&multi).contains("多个整合包"));

        let mcbbs = dir.path().join("mcbbs.zip");
        write_zip(
            &mcbbs,
            &[(
                "manifest.json",
                br#"{"manifestType":"minecraftModpack","addons":[]}"#,
            )],
        );
        assert_eq!(detect_zip(&mcbbs).kind, PackKind::Mcbbs);

        let hmcl = dir.path().join("hmcl.zip");
        write_zip(&hmcl, &[("modpack.json", b"{}")]);
        assert!(detect_zip_error(&hmcl).contains("minecraft 目录"));

        let plain = dir.path().join("plain.zip");
        write_zip(&plain, &[("readme.txt", b"hi")]);
        assert!(detect_zip_error(&plain).contains("不是有效的整合包文件"));
    }

    #[test]
    fn curseforge_uses_primary_loader_direct_url_and_custom_overrides() {
        let text = r#"{
            "manifestType": "minecraftModpack",
            "manifestVersion": 1,
            "name": "FTB Demo",
            "minecraft": {
                "version": "1.20.1",
                "modLoaders": [
                    {"id": "forge-47.2.0", "primary": false},
                    {"id": "fabric-0.15.0", "primary": true}
                ]
            },
            "overrides": "custom",
            "files": [
                {"projectID": 1, "fileID": 2, "fileName": "A.jar", "url": "https://example.com/A.jar"},
                {"projectID": 3, "fileID": 4}
            ]
        }"#;
        let parsed = parse_curseforge_manifest(text).unwrap();
        assert_eq!(
            parsed.loader.expect("primary 标记应优先").kind,
            LoaderKind::Fabric
        );
        assert_eq!(parsed.overrides_dir, "custom");
        let direct = super::curseforge::direct_pack_file(&parsed.files[0]).expect("直链应优先生成");
        assert_eq!(direct.path, "mods/A.jar");
        assert_eq!(direct.url, "https://example.com/A.jar");
        assert!(
            super::curseforge::direct_pack_file(&parsed.files[1]).is_none(),
            "无 url 的文件应留给 API 换链"
        );

        let entry = super::curseforge::CfFileEntry {
            project_id: 1,
            file_id: 2,
            file_name: None,
            url: Some("https://x/a%20b.jar?token=1".into()),
        };
        assert_eq!(
            super::curseforge::direct_pack_file(&entry).unwrap().path,
            "mods/a b.jar",
            "无 fileName 时从 URL 取并解码"
        );
    }

    #[test]
    fn curseforge_rejects_bad_overrides_dir_and_manifest_version() {
        let bad_dir = r#"{"manifestType":"minecraftModpack","manifestVersion":1,"minecraft":{"version":"1.20.1","modLoaders":[]},"overrides":"../escape","files":[]}"#;
        assert!(parse_curseforge_manifest(bad_dir).is_err());

        let bad_version = r#"{"manifestType":"minecraftModpack","manifestVersion":2,"minecraft":{"version":"1.20.1"},"files":[]}"#;
        let error = parse_curseforge_manifest(bad_version).unwrap_err();
        assert!(
            error.contains("manifestVersion"),
            "错误应指出清单版本: {error}"
        );
    }

    #[test]
    fn multimc_maps_components_and_content_dir() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mmc.zip");
        write_zip(
            &path,
            &[
                (
                    "Pack/mmc-pack.json",
                    br#"{"formatVersion":1,"components":[
                        {"uid":"net.minecraft","version":"1.20.1"},
                        {"uid":"net.fabricmc.fabric-loader","version":"0.15.11"},
                        {"uid":"org.lwjgl","version":"3.3.1"},
                        {"uid":"net.fabricmc.intermediary","version":"1.20.1"}
                    ]}"#,
                ),
                (
                    "Pack/instance.cfg",
                    b"InstanceType=OneSix\nMaxMemAlloc=4096\n",
                ),
                ("Pack/minecraft/mods/a.jar", b"jar"),
            ],
        );
        let detected = detect_zip(&path);
        assert_eq!(detected.kind, PackKind::MultiMc);
        assert_eq!(detected.base, "Pack/");
        assert_eq!(detected.content_dir.as_deref(), Some("minecraft/"));

        let parsed = super::multimc::parse_mmc_pack(&detected, "z").unwrap();
        assert_eq!(parsed.mc_version, "1.20.1");
        let loader = parsed.loader.expect("fabric-loader");
        assert_eq!(loader.kind, LoaderKind::Fabric);
        assert_eq!(loader.version, "0.15.11");
        assert_eq!(parsed.overrides.len(), 1);
        assert_eq!(parsed.overrides[0].prefix, "Pack/minecraft/");
    }

    #[test]
    fn multimc_falls_back_to_intended_version_and_rejects_liteloader() {
        let dir = tempfile::tempdir().unwrap();
        let legacy = dir.path().join("legacy.zip");
        write_zip(
            &legacy,
            &[
                (
                    "Old/instance.cfg",
                    b"name=Old Pack\nIntendedVersion=1.12.2\n",
                ),
                ("Old/minecraft/mods/a.jar", b"jar"),
            ],
        );
        let detected = detect_zip(&legacy);
        let parsed = super::multimc::parse_mmc_pack(&detected, "z").unwrap();
        assert_eq!(parsed.mc_version, "1.12.2");
        assert!(parsed.loader.is_none());
        assert_eq!(parsed.meta.name, "Old Pack");

        let lite = dir.path().join("lite.zip");
        write_zip(
            &lite,
            &[
                (
                    "Lite/mmc-pack.json",
                    br#"{"components":[{"uid":"net.minecraft","version":"1.7.10"},{"uid":"com.mumfrey.liteloader","version":"1.7.10"}]}"#,
                ),
                ("Lite/minecraft/mods/a.jar", b"jar"),
            ],
        );
        let detected = detect_zip(&lite);
        let error = super::multimc::parse_mmc_pack(&detected, "z").unwrap_err();
        assert!(
            error.contains("LiteLoader"),
            "应明确拒绝 LiteLoader: {error}"
        );
    }

    #[test]
    fn multimc_accepts_optifine_component() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("of.zip");
        write_zip(
            &path,
            &[
                (
                    "P/mmc-pack.json",
                    br#"{"components":[{"uid":"net.minecraft","version":"1.20.1"},{"uid":"optifine","version":"HD_U_I6"}]}"#,
                ),
                ("P/minecraft/mods/a.jar", b"jar"),
            ],
        );
        let detected = detect_zip(&path);
        assert_eq!(detected.kind, PackKind::MultiMc);
        let parsed = super::multimc::parse_mmc_pack(&detected, "z").unwrap();
        assert_eq!(parsed.optifine.as_deref(), Some("HD_U_I6"));
        assert!(parsed.loader.is_none(), "OptiFine 不是 ModLoader");
    }

    #[test]
    fn multimc_missing_game_and_content_are_reported() {
        let dir = tempfile::tempdir().unwrap();
        let no_game = dir.path().join("no-game.zip");
        write_zip(
            &no_game,
            &[
                ("P/mmc-pack.json", br#"{"components":[]}"#),
                ("P/minecraft/mods/a.jar", b"jar"),
            ],
        );
        let detected = detect_zip(&no_game);
        assert!(super::multimc::parse_mmc_pack(&detected, "z")
            .unwrap_err()
            .contains("Minecraft 版本"));

        let no_content = dir.path().join("no-content.zip");
        write_zip(
            &no_content,
            &[(
                "P/mmc-pack.json",
                br#"{"components":[{"uid":"net.minecraft","version":"1.20.1"}]}"#,
            )],
        );
        let detected = detect_zip(&no_content);
        assert!(super::multimc::parse_mmc_pack(&detected, "z")
            .unwrap_err()
            .contains("实例目录"));
    }

    #[test]
    fn plan_deserializes_legacy_single_overrides() {
        let plan: PackPlan = serde_json::from_str(
            r#"{"name":"n","mc_version":"1.20.1","version_url":"u","version_dir_name":"d",
                "overrides":{"zip_path":"z","prefix":"overrides/"}}"#,
        )
        .unwrap();
        assert_eq!(plan.overrides.len(), 1);
        assert_eq!(plan.overrides[0].prefix, "overrides/");
        assert!(plan.kind.is_none());
    }

    // ── 真实包冒烟（需环境变量指向样本；联网只取版本清单）──

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "需要 ELYTRA_SMOKE_MRPACK 指向真实 .mrpack"]
    async fn real_mrpack_build_plan_smoke() {
        let Ok(path) = std::env::var("ELYTRA_SMOKE_MRPACK") else {
            return;
        };
        let path = std::path::PathBuf::from(path);
        let request = ModpackRequest {
            name: "smoke".into(),
            icon_url: None,
            source: PackSource::Local {
                path: path.display().to_string(),
            },
        };
        let plan = build_plan(&path, &request, &crate::persist::downloader_config())
            .await
            .expect("真实 MRPACK 应能构建计划");
        assert_eq!(plan.kind, Some(PackKind::Modrinth));
        assert!(!plan.mc_version.is_empty());
        assert!(!plan.version_url.is_empty());
        assert!(
            plan.overrides
                .iter()
                .any(|ov| ov.prefix.ends_with("overrides/")),
            "真实包应识别 overrides/"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "需要联网解析版本清单"]
    async fn curseforge_direct_url_build_plan_smoke() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cf.zip");
        let manifest = r#"{
            "manifestType": "minecraftModpack",
            "manifestVersion": 1,
            "name": "FTB Smoke",
            "version": "1.0",
            "author": "x",
            "minecraft": {
                "version": "1.20.1",
                "modLoaders": [{"id": "forge-47.2.0", "primary": true}]
            },
            "overrides": "overrides",
            "files": [{
                "projectID": 1,
                "fileID": 2,
                "fileName": "smoke.jar",
                "url": "https://cdn.modrinth.com/data/1KVo5zza/versions/zsRTt1bK/Fabulously.Optimized-v14.0.0-beta.4.mrpack"
            }]
        }"#;
        write_zip(
            &path,
            &[
                ("manifest.json", manifest.as_bytes()),
                ("overrides/config/a.txt", b"a"),
            ],
        );
        let request = ModpackRequest {
            name: "cf-smoke".into(),
            icon_url: None,
            source: PackSource::Local {
                path: path.display().to_string(),
            },
        };
        let plan = build_plan(&path, &request, &crate::persist::downloader_config())
            .await
            .expect("CF 直链包应能构建计划");
        assert_eq!(plan.kind, Some(PackKind::CurseForge));
        assert_eq!(plan.files.len(), 1, "直链文件应直接生成 PackFile");
        assert_eq!(plan.files[0].path, "mods/smoke.jar");
        assert!(plan.restricted.is_empty());
        assert!(plan.overrides.iter().any(|ov| ov.prefix == "overrides/"));
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "需要联网解析版本清单"]
    async fn multimc_build_plan_smoke() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mmc.zip");
        write_zip(
            &path,
            &[
                (
                    "MyPack/mmc-pack.json",
                    br#"{"formatVersion":1,"components":[
                        {"uid":"net.minecraft","version":"1.20.1"},
                        {"uid":"net.fabricmc.fabric-loader","version":"0.15.11"}
                    ]}"#,
                ),
                (
                    "MyPack/instance.cfg",
                    b"InstanceType=OneSix\nname=My Pack\n",
                ),
                ("MyPack/minecraft/mods/a.jar", b"jar"),
            ],
        );
        let request = ModpackRequest {
            name: "mmc-smoke".into(),
            icon_url: None,
            source: PackSource::Local {
                path: path.display().to_string(),
            },
        };
        let plan = build_plan(&path, &request, &crate::persist::downloader_config())
            .await
            .expect("MultiMC 包应能构建计划");
        assert_eq!(plan.kind, Some(PackKind::MultiMc));
        assert_eq!(plan.mc_version, "1.20.1");
        assert_eq!(plan.loader.expect("fabric").kind, LoaderKind::Fabric);
        assert!(plan.files.is_empty(), "MMC 包是纯物理内容");
        assert_eq!(plan.overrides[0].prefix, "MyPack/minecraft/");
        assert!(!plan.version_url.is_empty());
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "需要 ELYTRA_SMOKE_MMC 指向真实 MultiMC 实例 zip"]
    async fn real_multimc_build_plan_smoke() {
        let Ok(path) = std::env::var("ELYTRA_SMOKE_MMC") else {
            return;
        };
        let path = std::path::PathBuf::from(path);
        let request = ModpackRequest {
            name: "smoke-mmc".into(),
            icon_url: None,
            source: PackSource::Local {
                path: path.display().to_string(),
            },
        };
        let plan = build_plan(&path, &request, &crate::persist::downloader_config())
            .await
            .expect("真实 MultiMC 包应能构建计划");
        assert_eq!(plan.kind, Some(PackKind::MultiMc));
        assert!(plan.loader.is_some(), "MultiMC 包应带 loader 组件");
        assert!(plan
            .overrides
            .iter()
            .any(|ov| ov.prefix.ends_with("minecraft/")));
    }

    // ── PCL 带启动器 / 完整目录包 ──

    #[test]
    fn detect_pcl_launcher_and_extract_embedded() {
        let dir = tempfile::tempdir().unwrap();
        let inner = dir.path().join("inner.zip");
        write_zip(
            &inner,
            &[
                ("modrinth.index.json", MRPACK_INDEX.as_bytes()),
                ("overrides/mods/a.jar", b"jar"),
            ],
        );
        let inner_bytes = std::fs::read(&inner).unwrap();
        let outer = dir.path().join("pcl.zip");
        write_zip(
            &outer,
            &[
                ("Plain Craft Launcher.exe", b"MZ"),
                ("modpack.zip", &inner_bytes),
            ],
        );

        let detected = detect_zip(&outer);
        assert_eq!(detected.kind, PackKind::PclLauncher);
        assert_eq!(detected.embedded.as_deref(), Some("modpack.zip"));

        let request = ModpackRequest {
            name: "pcl".into(),
            icon_url: None,
            source: PackSource::Local {
                path: outer.display().to_string(),
            },
        };
        let extracted = super::plan::extract_embedded(&outer, "modpack.zip", &request).unwrap();
        assert!(extracted.is_file());
        assert_eq!(detect_zip(&extracted).kind, PackKind::Modrinth);
        std::fs::remove_file(extracted).ok();
    }

    #[test]
    fn detect_directory_pack_and_parse_loader() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("dir.zip");
        let version = r#"{
            "id": "1.20.1-forge-47.2.0",
            "inheritsFrom": "1.20.1",
            "libraries": [{"name": "net.minecraftforge:forge:1.20.1-47.2.0"}]
        }"#;
        write_zip(
            &path,
            &[
                (
                    "Foo/versions/1.20.1-forge-47.2.0/1.20.1-forge-47.2.0.json",
                    version.as_bytes(),
                ),
                ("Foo/versions/1.20.1-forge-47.2.0/mods/a.jar", b"jar"),
            ],
        );

        let detected = detect_zip(&path);
        assert_eq!(detected.kind, PackKind::Directory);
        assert_eq!(detected.base, "Foo/");
        assert_eq!(
            detected.content_dir.as_deref(),
            Some("versions/1.20.1-forge-47.2.0/")
        );

        let parsed = super::directory::parse_directory_pack(&detected, "z").unwrap();
        assert_eq!(parsed.mc_version, "1.20.1");
        let loader = parsed.loader.expect("forge");
        assert_eq!(loader.kind, LoaderKind::Forge);
        assert_eq!(loader.version, "47.2.0");
        assert_eq!(
            parsed.overrides[0].prefix,
            "Foo/versions/1.20.1-forge-47.2.0/"
        );
        assert!(parsed.overrides[0]
            .skip
            .iter()
            .any(|skip| skip == "libraries/"));
    }

    #[test]
    fn detect_rejects_multiple_versions_and_plain_mods() {
        let dir = tempfile::tempdir().unwrap();
        let multi = dir.path().join("multi.zip");
        write_zip(
            &multi,
            &[
                ("versions/1.20.1/1.20.1.json", br#"{"id":"1.20.1"}"#),
                ("versions/1.19.2/1.19.2.json", br#"{"id":"1.19.2"}"#),
            ],
        );
        assert!(detect_zip_error(&multi).contains("多个游戏版本"));

        let mods = dir.path().join("mods-only.zip");
        write_zip(&mods, &[("mods/a.jar", b"jar")]);
        assert!(detect_zip_error(&mods).contains("不是有效的整合包文件"));
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "需要联网解析版本清单"]
    async fn directory_build_plan_smoke() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("dir.zip");
        let version = r#"{
            "id": "1.20.1-forge-47.2.0",
            "inheritsFrom": "1.20.1",
            "libraries": [{"name": "net.minecraftforge:forge:1.20.1-47.2.0"}]
        }"#;
        write_zip(
            &path,
            &[
                (
                    "versions/1.20.1-forge-47.2.0/1.20.1-forge-47.2.0.json",
                    version.as_bytes(),
                ),
                ("versions/1.20.1-forge-47.2.0/mods/a.jar", b"jar"),
            ],
        );
        let request = ModpackRequest {
            name: "dir-smoke".into(),
            icon_url: None,
            source: PackSource::Local {
                path: path.display().to_string(),
            },
        };
        let plan = build_plan(&path, &request, &crate::persist::downloader_config())
            .await
            .expect("目录包应能构建计划");
        assert_eq!(plan.kind, Some(PackKind::Directory));
        assert_eq!(plan.mc_version, "1.20.1");
        assert_eq!(plan.loader.expect("forge").version, "47.2.0");
        assert!(plan.files.is_empty());
        assert_eq!(plan.overrides[0].prefix, "versions/1.20.1-forge-47.2.0/");
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "需要联网解析版本清单"]
    async fn pcl_launcher_build_plan_smoke() {
        let dir = tempfile::tempdir().unwrap();
        let inner = dir.path().join("inner.zip");
        write_zip(
            &inner,
            &[
                ("modrinth.index.json", MRPACK_INDEX.as_bytes()),
                ("overrides/mods/a.jar", b"jar"),
            ],
        );
        let inner_bytes = std::fs::read(&inner).unwrap();
        let outer = dir.path().join("pcl.zip");
        write_zip(
            &outer,
            &[
                ("Plain Craft Launcher.exe", b"MZ"),
                ("modpack.zip", &inner_bytes),
            ],
        );
        let request = ModpackRequest {
            name: "pcl-smoke".into(),
            icon_url: None,
            source: PackSource::Local {
                path: outer.display().to_string(),
            },
        };
        let plan = build_plan(&outer, &request, &crate::persist::downloader_config())
            .await
            .expect("PCL 带启动器包应能递归构建计划");
        assert_eq!(
            plan.kind,
            Some(PackKind::Modrinth),
            "最终格式应为内嵌包格式"
        );
        assert!(
            plan.overrides
                .iter()
                .all(|ov| ov.zip_path.ends_with("-embedded.zip")),
            "覆盖目录应指向内嵌缓存"
        );
        for ov in &plan.overrides {
            std::fs::remove_file(&ov.zip_path).ok();
        }
    }

    // ── HMCL / MCBBS ──

    #[test]
    fn detect_hmcl_and_parse_loader() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hmcl.zip");
        let info = r#"{"name":"H Pack","author":"a","version":"1.0","gameVersion":"1.20.1"}"#;
        let pack = r#"{
            "id": "1.20.1-fabric-0.15.11",
            "inheritsFrom": "1.20.1",
            "libraries": [{"name": "net.fabricmc:fabric-loader:0.15.11"}]
        }"#;
        write_zip(
            &path,
            &[
                ("Pack/modpack.json", info.as_bytes()),
                ("Pack/minecraft/pack.json", pack.as_bytes()),
                ("Pack/minecraft/mods/a.jar", b"jar"),
            ],
        );

        let detected = detect_zip(&path);
        assert_eq!(detected.kind, PackKind::Hmcl);
        assert_eq!(detected.base, "Pack/");
        assert_eq!(detected.content_dir.as_deref(), Some("minecraft/"));

        let parsed = super::hmcl::parse_hmcl_pack(&detected, "z").unwrap();
        assert_eq!(parsed.mc_version, "1.20.1");
        let loader = parsed.loader.expect("fabric");
        assert_eq!(loader.kind, LoaderKind::Fabric);
        assert_eq!(loader.version, "0.15.11");
        assert_eq!(parsed.overrides[0].prefix, "Pack/minecraft/");
        assert!(parsed.overrides[0]
            .skip
            .iter()
            .any(|skip| skip == "pack.json"));
    }

    #[test]
    fn detect_hmcl_without_minecraft_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hmcl.zip");
        write_zip(&path, &[("modpack.json", b"{}")]);
        assert!(detect_zip_error(&path).contains("minecraft 目录"));
    }

    #[test]
    fn detect_mcbbs_and_parse_addons_files() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mcbbs.zip");
        let meta = r#"{
            "manifestType": "minecraftModpack",
            "manifestVersion": 2,
            "name": "M Pack",
            "version": "1.0",
            "author": "a",
            "fileApi": "https://cdn.example.com/pack",
            "addons": [
                {"id": "game", "version": "1.20.1"},
                {"id": "forge", "version": "47.2.0"}
            ],
            "files": [
                {"type": "addon", "path": "mods/a.jar", "hash": "abc"},
                {"type": "curse", "projectID": 1, "fileID": 2, "fileName": "b.jar"}
            ]
        }"#;
        write_zip(
            &path,
            &[
                ("mcbbs.packmeta", meta.as_bytes()),
                ("overrides/mods/c.jar", b"jar"),
            ],
        );

        let detected = detect_zip(&path);
        assert_eq!(detected.kind, PackKind::Mcbbs);
        let parsed = super::mcbbs::parse_mcbbs_pack(&detected, "z").unwrap();
        assert_eq!(parsed.mc_version, "1.20.1");
        assert_eq!(parsed.loader.expect("forge").kind, LoaderKind::Forge);
        assert_eq!(parsed.files[0].path, "mods/a.jar");
        assert_eq!(
            parsed.files[0].url,
            "https://cdn.example.com/pack/overrides/mods/a.jar"
        );
        assert_eq!(parsed.curse.len(), 1);
        assert_eq!(parsed.overrides[0].prefix, "overrides/");
    }

    #[test]
    fn detect_mcbbs_legacy_manifest_and_accepts_optifine() {
        let dir = tempfile::tempdir().unwrap();
        let legacy = dir.path().join("legacy.zip");
        write_zip(
            &legacy,
            &[(
                "manifest.json",
                br#"{"manifestType":"minecraftModpack","addons":[{"id":"game","version":"1.12.2"}]}"#,
            )],
        );
        let detected = detect_zip(&legacy);
        assert_eq!(detected.kind, PackKind::Mcbbs);
        let parsed = super::mcbbs::parse_mcbbs_pack(&detected, "z").unwrap();
        assert_eq!(parsed.mc_version, "1.12.2");

        let optifine = dir.path().join("optifine.zip");
        write_zip(
            &optifine,
            &[(
                "mcbbs.packmeta",
                br#"{"addons":[{"id":"game","version":"1.20.1"},{"id":"optifine","version":"H9"}]}"#,
            )],
        );
        let detected = detect_zip(&optifine);
        let parsed = super::mcbbs::parse_mcbbs_pack(&detected, "z").unwrap();
        assert_eq!(parsed.optifine.as_deref(), Some("H9"));

        let liteloader = dir.path().join("liteloader.zip");
        write_zip(
            &liteloader,
            &[(
                "mcbbs.packmeta",
                br#"{"addons":[{"id":"game","version":"1.7.10"},{"id":"liteloader","version":"1.7.10"}]}"#,
            )],
        );
        let detected = detect_zip(&liteloader);
        let error = super::mcbbs::parse_mcbbs_pack(&detected, "z").unwrap_err();
        assert!(error.contains("liteloader"), "LiteLoader 仍应拒绝: {error}");
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "需要联网解析版本清单"]
    async fn hmcl_build_plan_smoke() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hmcl.zip");
        let info = r#"{"name":"H Pack","author":"a","version":"1.0","gameVersion":"1.20.1"}"#;
        let pack = r#"{
            "id": "1.20.1-fabric-0.15.11",
            "inheritsFrom": "1.20.1",
            "libraries": [{"name": "net.fabricmc:fabric-loader:0.15.11"}]
        }"#;
        write_zip(
            &path,
            &[
                ("modpack.json", info.as_bytes()),
                ("minecraft/pack.json", pack.as_bytes()),
                ("minecraft/mods/a.jar", b"jar"),
            ],
        );
        let request = ModpackRequest {
            name: "hmcl-smoke".into(),
            icon_url: None,
            source: PackSource::Local {
                path: path.display().to_string(),
            },
        };
        let plan = build_plan(&path, &request, &crate::persist::downloader_config())
            .await
            .expect("HMCL 包应能构建计划");
        assert_eq!(plan.kind, Some(PackKind::Hmcl));
        assert_eq!(plan.mc_version, "1.20.1");
        assert_eq!(plan.loader.expect("fabric").version, "0.15.11");
        assert_eq!(plan.overrides[0].prefix, "minecraft/");
    }

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "需要联网解析版本清单"]
    async fn mcbbs_build_plan_smoke() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mcbbs.zip");
        let meta = r#"{
            "addons": [
                {"id": "game", "version": "1.20.1"},
                {"id": "forge", "version": "47.2.0"},
                {"id": "optifine", "version": "HD_U_I6"}
            ],
            "files": []
        }"#;
        write_zip(
            &path,
            &[
                ("mcbbs.packmeta", meta.as_bytes()),
                ("overrides/mods/a.jar", b"jar"),
            ],
        );
        let request = ModpackRequest {
            name: "mcbbs-smoke".into(),
            icon_url: None,
            source: PackSource::Local {
                path: path.display().to_string(),
            },
        };
        let plan = build_plan(&path, &request, &crate::persist::downloader_config())
            .await
            .expect("MCBBS 包应能构建计划");
        assert_eq!(plan.kind, Some(PackKind::Mcbbs));
        assert_eq!(plan.mc_version, "1.20.1");
        assert_eq!(plan.loader.expect("forge").version, "47.2.0");
        assert_eq!(plan.optifine.as_deref(), Some("HD_U_I6"));
        assert_eq!(plan.overrides[0].prefix, "overrides/");
    }
}
