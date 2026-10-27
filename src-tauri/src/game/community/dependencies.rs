//! 前置 Mod 解析：从主 Mod 版本出发递归展开 required/optional 依赖，按目标实例的 MC/loader
//! 选兼容版本；`visited` 去重与去环，节点上限 512。

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::Path;

use serde::Serialize;

use super::api::CommunityVersion;
use super::{curseforge, modrinth};

/// 节点上限：正常整合包远低于此；仅拦截异常/恶意元数据。
const MAX_DEPENDENCY_NODES: usize = 512;
/// 候选上限：控制宽依赖树的计划体积；当前选中版本始终保留。
const MAX_DEPENDENCY_CANDIDATES: usize = 40;

#[derive(Debug, Clone, Serialize, ts_rs::TS)]
#[ts(export_to = "community/")]
pub struct ModDependencyVersion {
    pub version_id: String,
    pub version_label: String,
    pub version_type: String,
}

#[derive(Debug, Clone, Serialize, ts_rs::TS)]
#[ts(export_to = "community/")]
pub struct ModDependencyItem {
    pub project_id: String,
    pub version_id: String,
    pub name: String,
    pub version_label: String,
    pub loader: String,
    pub game_version: String,
    pub url: String,
    pub filename: String,
    /// required | optional
    pub relation: String,
    /// 所属可选根（None = 主 Mod 的必装树）
    pub root_optional: Option<String>,
    pub installed: bool,
    pub error: Option<String>,
    /// 已安装的其他兼容版本（当前选中版本未安装时给出）。
    pub installed_version_label: Option<String>,
    /// 该项目已落盘的版本文件名；用于安装时替换旧版本。
    pub installed_files: Vec<String>,
    /// 仅含与实例 MC/loader 组合兼容的版本；按发布时间倒序。
    pub candidates: Vec<ModDependencyVersion>,
    pub candidate_total: usize,
}

#[derive(Debug, Clone, Serialize, ts_rs::TS)]
#[ts(export_to = "community/")]
pub struct ModDependencyPlan {
    pub instance_mc: String,
    pub instance_loader: String,
    pub items: Vec<ModDependencyItem>,
    pub errors: Vec<String>,
    /// 目标 mods 目录当前文件名；前端据此识别主 Mod 的旧版本。
    pub installed_files: Vec<String>,
}

struct Pending {
    project_id: String,
    version_hint: Option<String>,
    relation: String,
    root_optional: Option<String>,
}

/// 解析主 Mod 的全部前置（required + optional）。主版本不适用/依赖链过大走 `errors`，
/// 单个前置无兼容版本走 `item.error`（不阻断其他节点）。
pub(crate) async fn resolve_mod_dependencies(
    source: &str,
    project_id: &str,
    version_id: &str,
    mc_version: &str,
    loader: &str,
    version_overrides: &HashMap<String, String>,
    mods_dir: &Path,
) -> Result<ModDependencyPlan, String> {
    let installed = installed_filenames(mods_dir);
    let mut installed_files: Vec<String> = installed.iter().cloned().collect();
    installed_files.sort();
    let mut plan = ModDependencyPlan {
        instance_mc: mc_version.to_string(),
        instance_loader: loader.to_string(),
        items: Vec::new(),
        errors: Vec::new(),
        installed_files,
    };

    let root_versions = fetch_versions(source, project_id, mc_version, loader).await?;
    let Some(root) = root_versions
        .iter()
        .find(|version| version.version_id == version_id)
    else {
        plan.errors.push(keyed_msg!(
            "errors.community.dependencyMainVersionMismatch",
            "主 Mod 版本不适用于当前实例"
        ));
        return Ok(plan);
    };

    let mut visited: HashSet<String> = HashSet::new();
    visited.insert(project_id.to_string());

    let mut required_queue: VecDeque<Pending> = VecDeque::new();
    let mut optional_queue: VecDeque<Pending> = VecDeque::new();
    seed_dependencies(root, None, &mut required_queue, &mut optional_queue);

    while let Some(pending) = required_queue
        .pop_front()
        .or_else(|| optional_queue.pop_front())
    {
        if !visited.insert(pending.project_id.clone()) {
            continue;
        }
        if visited.len() > MAX_DEPENDENCY_NODES {
            plan.errors.push(keyed_msg!(
                "errors.community.dependencyTooLarge",
                "依赖链过大，已停止解析"
            ));
            break;
        }

        let versions = match fetch_versions(source, &pending.project_id, mc_version, loader).await {
            Ok(versions) => versions,
            Err(_) => {
                plan.items.push(missing_item(
                    &pending,
                    mc_version,
                    loader,
                    &keyed_msg!("errors.community.dependencyFetchFailed", "无法获取前置版本"),
                    &[],
                ));
                continue;
            }
        };
        let selected = match pick_version(
            &versions,
            version_overrides
                .get(&pending.project_id)
                .map(String::as_str),
            pending.version_hint.as_deref(),
            &installed,
        ) {
            Ok(version) => version,
            Err(error) => {
                plan.items.push(missing_item(
                    &pending, mc_version, loader, &error, &versions,
                ));
                continue;
            }
        };

        let filename = selected.filename.clone();
        let url = selected.download_url.clone();
        let error = url.is_empty().then(|| {
            keyed_msg!(
                "errors.community.dependencyNoDownloadUrl",
                "缺少下载地址（作者可能禁止第三方分发）"
            )
        });
        let (candidates, candidate_total) = build_candidates(&versions, &selected.version_id);
        let installed_files: Vec<String> = versions
            .iter()
            .filter(|version| installed.contains(&version.filename))
            .map(|version| version.filename.clone())
            .collect();
        let installed_version_label = versions
            .iter()
            .find(|version| installed.contains(&version.filename))
            .filter(|version| version.version_id != selected.version_id)
            .map(|version| version.version_number.clone());
        plan.items.push(ModDependencyItem {
            project_id: pending.project_id.clone(),
            version_id: selected.version_id.clone(),
            name: selected.filename.clone(),
            version_label: selected.version_number.clone(),
            loader: loader.to_string(),
            game_version: mc_version.to_string(),
            url,
            filename: filename.clone(),
            relation: pending.relation.clone(),
            root_optional: pending.root_optional.clone(),
            installed: installed.contains(&filename),
            installed_version_label,
            installed_files,
            error,
            candidates,
            candidate_total,
        });
        if plan.items.last().is_some_and(|item| item.error.is_some()) {
            continue;
        }

        let child_optional_root = pending.root_optional.as_ref();
        for dependency in &selected.dependencies {
            let relation = dependency.relation.as_str();
            if relation != "required" && relation != "optional" {
                continue;
            }
            let is_optional_root = relation == "optional";
            let root = if is_optional_root {
                Some(dependency.project_id.clone())
            } else {
                child_optional_root.cloned()
            };
            let pending = Pending {
                project_id: dependency.project_id.clone(),
                version_hint: dependency.version_id.clone(),
                relation: relation.to_string(),
                root_optional: root,
            };
            if child_optional_root.is_some() || is_optional_root {
                optional_queue.push_back(pending);
            } else {
                required_queue.push_back(pending);
            }
        }
    }

    fill_names(source, &mut plan.items).await;
    Ok(plan)
}

async fn fetch_versions(
    source: &str,
    project_id: &str,
    mc_version: &str,
    loader: &str,
) -> Result<Vec<CommunityVersion>, String> {
    let versions = match source {
        "modrinth" => modrinth::modrinth_versions(project_id, Some(mc_version), Some(loader)).await,
        "curseforge" => {
            curseforge::curseforge_versions(project_id, Some(mc_version), Some(loader)).await
        }
        other => {
            return Err(keyed_msg!(
                "errors.community.unsupportedSource",
                "不支持的资源来源\n{other}"
            ))
        }
    }?;
    Ok(versions
        .into_iter()
        .filter(|version| version_supports(version, mc_version, loader))
        .collect())
}

/// 平台过滤之外再校验一次，确保候选只属于实例的 MC/loader 组合。
fn version_supports(version: &CommunityVersion, mc_version: &str, loader: &str) -> bool {
    version
        .game_versions
        .iter()
        .any(|value| value == mc_version)
        && version
            .loaders
            .iter()
            .any(|value| value.eq_ignore_ascii_case(loader))
}

/// 版本覆盖优先，其次精确 `version_hint`；否则 release 优先，最后取第一个。
fn pick_version<'a>(
    versions: &'a [CommunityVersion],
    override_id: Option<&str>,
    hint: Option<&str>,
    installed: &HashSet<String>,
) -> Result<&'a CommunityVersion, String> {
    if let Some(id) = override_id.or(hint) {
        return versions
            .iter()
            .find(|version| version.version_id == id)
            .ok_or_else(|| {
                keyed_msg!(
                    "errors.community.dependencyVersionMismatch",
                    "指定前置版本不适用于当前实例"
                )
            });
    }
    versions
        .iter()
        .find(|version| installed.contains(&version.filename))
        .or_else(|| {
            versions
                .iter()
                .find(|version| version.version_type == "release")
        })
        .or_else(|| versions.first())
        .ok_or_else(|| {
            keyed_msg!(
                "errors.community.dependencyNoCompatibleVersion",
                "没有适用于当前实例的前置版本"
            )
        })
}

/// 候选按发布时间倒序，限制数量；当前选中项即使超出上限也补进列表。
fn build_candidates(
    versions: &[CommunityVersion],
    selected_id: &str,
) -> (Vec<ModDependencyVersion>, usize) {
    let mut seen = HashSet::new();
    let mut unique: Vec<&CommunityVersion> = versions
        .iter()
        .filter(|version| seen.insert(version.version_id.as_str()))
        .collect();
    unique.sort_by(|a, b| b.date_published.cmp(&a.date_published));
    let total = unique.len();
    let mut candidates: Vec<ModDependencyVersion> = unique
        .iter()
        .take(MAX_DEPENDENCY_CANDIDATES)
        .map(|version| dependency_version(version))
        .collect();
    if !candidates
        .iter()
        .any(|candidate| candidate.version_id == selected_id)
    {
        if let Some(selected) = unique
            .iter()
            .find(|version| version.version_id == selected_id)
        {
            candidates.push(dependency_version(selected));
        }
    }
    (candidates, total)
}

fn dependency_version(version: &CommunityVersion) -> ModDependencyVersion {
    ModDependencyVersion {
        version_id: version.version_id.clone(),
        version_label: version.version_number.clone(),
        version_type: version.version_type.clone(),
    }
}

fn seed_dependencies(
    root: &CommunityVersion,
    root_optional: Option<&str>,
    required: &mut VecDeque<Pending>,
    optional: &mut VecDeque<Pending>,
) {
    for dependency in &root.dependencies {
        let relation = dependency.relation.as_str();
        let is_optional_root = relation == "optional";
        if relation != "required" && !is_optional_root {
            continue;
        }
        let pending = Pending {
            project_id: dependency.project_id.clone(),
            version_hint: dependency.version_id.clone(),
            relation: relation.to_string(),
            root_optional: if is_optional_root {
                Some(dependency.project_id.clone())
            } else {
                root_optional.map(str::to_string)
            },
        };
        if is_optional_root || root_optional.is_some() {
            optional.push_back(pending);
        } else {
            required.push_back(pending);
        }
    }
}

fn installed_filenames(mods_dir: &Path) -> HashSet<String> {
    let Ok(entries) = std::fs::read_dir(mods_dir) else {
        return HashSet::new();
    };
    entries
        .flatten()
        .filter_map(|entry| entry.file_name().to_str().map(str::to_string))
        .collect()
}

fn missing_item(
    pending: &Pending,
    mc_version: &str,
    loader: &str,
    error: &str,
    versions: &[CommunityVersion],
) -> ModDependencyItem {
    let (candidates, candidate_total) = build_candidates(versions, "");
    ModDependencyItem {
        project_id: pending.project_id.clone(),
        version_id: pending.version_hint.clone().unwrap_or_default(),
        name: pending.project_id.clone(),
        version_label: String::new(),
        loader: loader.to_string(),
        game_version: mc_version.to_string(),
        url: String::new(),
        filename: String::new(),
        relation: pending.relation.clone(),
        root_optional: pending.root_optional.clone(),
        installed: false,
        installed_version_label: None,
        installed_files: Vec::new(),
        error: Some(error.to_string()),
        candidates,
        candidate_total,
    }
}

/// 批量补项目标题；失败时保留文件名/项目 ID。
async fn fill_names(source: &str, items: &mut [ModDependencyItem]) {
    let ids: Vec<String> = items
        .iter()
        .map(|item| item.project_id.clone())
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    if ids.is_empty() {
        return;
    }
    let names: HashMap<String, String> = match source {
        "modrinth" => modrinth::modrinth_projects_raw(&ids)
            .await
            .map(|projects| {
                projects
                    .iter()
                    .filter_map(|project| {
                        Some((
                            project["id"].as_str()?.to_string(),
                            project["title"].as_str()?.to_string(),
                        ))
                    })
                    .collect()
            })
            .unwrap_or_default(),
        "curseforge" => curseforge::curseforge_details(&ids)
            .await
            .map(|details| {
                details
                    .into_iter()
                    .map(|detail| (detail.id, detail.title))
                    .collect()
            })
            .unwrap_or_default(),
        _ => HashMap::new(),
    };
    for item in items {
        if let Some(name) = names.get(&item.project_id) {
            item.name = name.clone();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::api::CommunityDependency;
    use super::*;

    fn version(
        id: &str,
        version_type: &str,
        dependencies: Vec<CommunityDependency>,
    ) -> CommunityVersion {
        CommunityVersion {
            version_id: id.to_string(),
            version_number: id.to_string(),
            game_versions: vec!["1.20.1".into()],
            loaders: vec!["fabric".into()],
            filename: format!("{id}.jar"),
            download_url: format!("https://example.com/{id}.jar"),
            file_size: 0,
            sha1: None,
            date_published: String::new(),
            downloads: 0,
            version_type: version_type.to_string(),
            dependencies,
        }
    }

    fn dependency(
        project_id: &str,
        relation: &str,
        version_id: Option<&str>,
    ) -> CommunityDependency {
        CommunityDependency {
            project_id: project_id.to_string(),
            relation: relation.to_string(),
            version_id: version_id.map(str::to_string),
        }
    }

    #[test]
    fn pick_version_prefers_hint_then_release() {
        let beta = version("beta", "beta", Vec::new());
        let release = version("release", "release", Vec::new());
        let versions = vec![beta, release];

        assert_eq!(
            pick_version(&versions, None, Some("beta"), &HashSet::new())
                .unwrap()
                .version_id,
            "beta"
        );
        assert_eq!(
            pick_version(&versions, None, None, &HashSet::new())
                .unwrap()
                .version_id,
            "release"
        );
        assert!(pick_version(&versions, None, Some("missing"), &HashSet::new()).is_err());
    }

    #[test]
    fn pick_version_override_wins_and_must_match() {
        let beta = version("beta", "beta", Vec::new());
        let release = version("release", "release", Vec::new());
        let versions = vec![beta, release];

        assert_eq!(
            pick_version(&versions, Some("beta"), Some("release"), &HashSet::new())
                .unwrap()
                .version_id,
            "beta"
        );
        assert!(pick_version(&versions, Some("missing"), None, &HashSet::new()).is_err());
    }

    #[test]
    fn pick_version_prefers_installed_when_unpinned() {
        let beta = version("beta", "beta", Vec::new());
        let release = version("release", "release", Vec::new());
        let versions = vec![beta, release];
        let installed = HashSet::from(["beta.jar".to_string()]);

        assert_eq!(
            pick_version(&versions, None, None, &installed)
                .unwrap()
                .version_id,
            "beta"
        );
    }

    #[test]
    fn version_supports_requires_matching_game_and_loader() {
        let mut item = version("v", "release", Vec::new());
        assert!(version_supports(&item, "1.20.1", "fabric"));
        assert!(!version_supports(&item, "1.21.1", "fabric"));
        assert!(!version_supports(&item, "1.20.1", "forge"));
        item.loaders.push("Quilt".into());
        assert!(version_supports(&item, "1.20.1", "quilt"));
    }

    #[test]
    fn candidates_include_selected_outside_limit() {
        let mut versions = Vec::new();
        for index in 0..(MAX_DEPENDENCY_CANDIDATES + 5) {
            let mut item = version(&format!("v{index}"), "release", Vec::new());
            item.date_published = format!("2026-01-{:02}", index + 1);
            versions.push(item);
        }
        let (candidates, total) = build_candidates(&versions, "v0");

        assert_eq!(total, versions.len());
        assert_eq!(candidates.len(), MAX_DEPENDENCY_CANDIDATES + 1);
        assert_eq!(candidates[0].version_id, format!("v{}", versions.len() - 1));
        assert!(candidates.iter().any(|item| item.version_id == "v0"));
    }

    #[test]
    fn seeds_required_and_optional_with_optional_root() {
        let root = version(
            "root",
            "release",
            vec![
                dependency("required", "required", Some("v1")),
                dependency("optional", "optional", None),
                dependency("incompatible", "incompatible", None),
            ],
        );
        let mut required = VecDeque::new();
        let mut optional = VecDeque::new();
        seed_dependencies(&root, None, &mut required, &mut optional);

        assert_eq!(required.len(), 1);
        assert_eq!(required[0].project_id, "required");
        assert!(required[0].root_optional.is_none());
        assert_eq!(required[0].version_hint.as_deref(), Some("v1"));

        assert_eq!(optional.len(), 1);
        assert_eq!(optional[0].project_id, "optional");
        assert_eq!(optional[0].root_optional.as_deref(), Some("optional"));
    }

    #[test]
    fn installed_filenames_reads_mods_dir() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.jar"), b"a").unwrap();
        std::fs::write(dir.path().join("b.jar.disabled"), b"b").unwrap();
        let names = installed_filenames(dir.path());
        assert!(names.contains("a.jar"));
        assert!(names.contains("b.jar.disabled"));
    }
}

#[cfg(test)]
mod real_tests {
    use std::collections::HashMap;

    use super::super::modrinth::modrinth_versions;
    use super::resolve_mod_dependencies;

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "需要真实 Modrinth API"]
    async fn resolves_real_modrinth_dependencies() {
        let versions = modrinth_versions("YL57xq9U", Some("1.20.1"), Some("fabric"))
            .await
            .expect("拉取 Iris 版本");
        let version = versions
            .iter()
            .find(|version| version.version_type == "release")
            .or_else(|| versions.first())
            .expect("Iris 应有 1.20.1 Fabric 版本");
        let dir = tempfile::tempdir().unwrap();
        let plan = resolve_mod_dependencies(
            "modrinth",
            "YL57xq9U",
            &version.version_id,
            "1.20.1",
            "fabric",
            &HashMap::new(),
            dir.path(),
        )
        .await
        .expect("解析前置");
        assert!(plan.errors.is_empty(), "{}", plan.errors.join("; "));
        assert!(!plan.items.is_empty(), "Iris 应有前置 Mod");
        println!("items: {:#?}", plan.items);
    }
}
