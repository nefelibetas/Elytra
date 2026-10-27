// ── 任务数据模型（前端经 invoke 传入的统一配置入口）──

use serde::{Deserialize, Serialize};

use crate::game::modpack::ModpackRequest;

/// 任务状态（进度事件 payload，前端 ProgressPayload.status 为 string）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Default, ts_rs::TS)]
#[ts(export_to = "task/")]
#[serde(rename_all = "lowercase")]
pub enum TaskStatus {
    #[default]
    Pending,
    Running,
    Done,
    Error,
    Cancelled,
    /// 用户暂停（任务从内存移除，快照保留可恢复）。
    Paused,
}

/// 步骤状态（进度事件 payload）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Default, ts_rs::TS)]
#[ts(export_to = "task/")]
#[serde(rename_all = "lowercase")]
pub enum StepStatus {
    #[default]
    Pending,
    Running,
    Done,
}

/// 步骤种类：前端按枚举翻译步骤名（`File` 仍显示文件名）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export_to = "task/")]
#[serde(rename_all = "snake_case")]
pub enum StepKind {
    /// 下载原版 version json（唯一硬前置，恒在，索引 0）
    VersionJson,
    /// client.jar（恒在，与 libraries/assets 并行）
    ClientJar,
    Libraries,
    Assets,
    /// native classifier 下载/解压
    Natives,
    /// ModLoader 配置（合并 json 写入）
    LoaderConfig,
    /// ModLoader 本体（fabric libs / installer→jar）
    LoaderLibs,
    #[serde(rename = "optifine")]
    OptiFine,
    FabricApi,
    /// managed JDK 运行时安装
    JavaRuntime,
    /// 修复任务：客户端主体
    RepairClient,
    /// 修复任务：支持库
    RepairLibraries,
    /// 修复任务：资源文件
    RepairAssets,
    /// 修复任务：原生库
    RepairNatives,
    /// 整合包安装：下载包体
    ModpackDownload,
    /// 整合包安装：解析清单
    ModpackParse,
    /// 整合包安装：版本清单
    ModpackVersionJson,
    /// 整合包安装：游戏主文件
    ModpackGame,
    /// 整合包安装：依赖库
    ModpackLibraries,
    /// 整合包安装：资源文件
    ModpackAssets,
    /// 整合包安装：原生库
    ModpackNatives,
    /// 整合包安装：ModLoader 配置
    ModpackLoaderConfig,
    /// 整合包安装：ModLoader 文件
    ModpackLoaderFiles,
    /// 整合包安装：解压 overrides
    ModpackOverrides,
    /// 批量文件下载的单个文件步骤
    File,
}

impl StepKind {
    /// 前端消息 key 后缀（与 serde 名一致）。
    pub fn i18n_key(self) -> &'static str {
        match self {
            StepKind::VersionJson => "version_json",
            StepKind::ClientJar => "client_jar",
            StepKind::Libraries => "libraries",
            StepKind::Assets => "assets",
            StepKind::Natives => "natives",
            StepKind::LoaderConfig => "loader_config",
            StepKind::LoaderLibs => "loader_libs",
            StepKind::OptiFine => "optifine",
            StepKind::FabricApi => "fabric_api",
            StepKind::JavaRuntime => "java_runtime",
            StepKind::RepairClient => "repair_client",
            StepKind::RepairLibraries => "repair_libraries",
            StepKind::RepairAssets => "repair_assets",
            StepKind::RepairNatives => "repair_natives",
            StepKind::ModpackDownload => "modpack_download",
            StepKind::ModpackParse => "modpack_parse",
            StepKind::ModpackVersionJson => "modpack_version_json",
            StepKind::ModpackGame => "modpack_game",
            StepKind::ModpackLibraries => "modpack_libraries",
            StepKind::ModpackAssets => "modpack_assets",
            StepKind::ModpackNatives => "modpack_natives",
            StepKind::ModpackLoaderConfig => "modpack_loader_config",
            StepKind::ModpackLoaderFiles => "modpack_loader_files",
            StepKind::ModpackOverrides => "modpack_overrides",
            StepKind::File => "file",
        }
    }
}

/// 任务名/阶段的 i18n 文本（与中文原文并行，前端优先 key）。
#[derive(Debug, Clone, Default)]
pub struct TaskText {
    pub key: Option<String>,
    pub params: Option<std::collections::BTreeMap<String, String>>,
}

impl TaskText {
    pub fn new(key: &str, params: &[(&str, String)]) -> Self {
        let params: std::collections::BTreeMap<_, _> = params
            .iter()
            .map(|(k, v)| ((*k).to_string(), v.clone()))
            .collect();
        Self {
            key: Some(key.to_string()),
            params: if params.is_empty() {
                None
            } else {
                Some(params)
            },
        }
    }
}

/// 任务进度快照（task-progress 事件推送体）。
#[derive(Debug, Clone, Serialize, ts_rs::TS)]
#[ts(export_to = "task/")]
pub struct TaskProgress {
    pub task_id: String,
    pub name: String,
    /// 任务名的 i18n key；旧快照/未迁移任务为 None，前端回退 name。
    pub name_key: Option<String>,
    pub name_params: Option<std::collections::BTreeMap<String, String>>,
    pub status: TaskStatus,
    pub progress: f64, // 0.0 – 100.0
    pub speed_bps: f64,
    /// 预计剩余秒数；停滞（>4s 无字节推进）或速度过低时 None。
    #[ts(type = "number | null")]
    pub eta_seconds: Option<u64>,
    pub completed_files: u32,
    pub total_files: u32,
    #[ts(type = "number")]
    pub completed_bytes: u64,
    #[ts(type = "number")]
    pub total_bytes: u64,
    pub current_phase: String,
    /// 阶段的 i18n key；动态文件名/进度等仍看 current_phase。
    pub current_phase_key: Option<String>,
    pub current_phase_params: Option<std::collections::BTreeMap<String, String>>,
    pub steps: Vec<StepInfo>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, ts_rs::TS)]
#[ts(export_to = "task/")]
pub struct StepInfo {
    pub name: String,
    /// 步骤枚举；None 时前端显示 name（旧事件/动态步骤）。
    pub kind: Option<StepKind>,
    pub status: StepStatus,
    pub progress: f64, // 0–100, meaningful only when status=running
}

/// ModLoader 种类。前端传 "fabric"/"forge"/"neoforge"/"quilt"。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export_to = "task/")]
pub enum LoaderKind {
    #[serde(rename = "fabric")]
    Fabric,
    #[serde(rename = "forge")]
    Forge,
    #[serde(rename = "neoforge")]
    NeoForge,
    #[serde(rename = "quilt")]
    Quilt,
}

impl LoaderKind {
    /// 英文标识（目录名/URL 模板用），与 serde rename 一致。
    pub fn name(&self) -> &'static str {
        match self {
            LoaderKind::Fabric => "fabric",
            LoaderKind::Forge => "forge",
            LoaderKind::NeoForge => "neoforge",
            LoaderKind::Quilt => "quilt",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            LoaderKind::Fabric => "Fabric",
            LoaderKind::Forge => "Forge",
            LoaderKind::NeoForge => "NeoForge",
            LoaderKind::Quilt => "Quilt",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, ts_rs::TS)]
#[ts(export_to = "task/")]
pub struct LoaderChoice {
    pub kind: LoaderKind,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ts_rs::TS)]
#[ts(export_to = "task/")]
pub struct FabricApi {
    pub url: String,
    pub version: String,
}

/// 下载游戏本体任务（原版，或带 ModLoader/OptiFine/Fabric API）。
#[derive(Debug, Clone, Serialize, Deserialize, ts_rs::TS)]
#[ts(export_to = "task/")]
pub struct DownloadTask {
    pub version_id: String,
    pub version_url: String,
    pub mc_version: String,
    #[serde(default)]
    pub loader: Option<LoaderChoice>,
    #[serde(default)]
    pub optifine_patch: Option<String>,
    #[serde(default)]
    pub version_dir_name: Option<String>,
    #[serde(default)]
    pub fabric_api: Option<FabricApi>,
}

impl DownloadTask {
    /// 信任边界校验：组合约束（Fabric API 必须配 Fabric）、空字段拦截。
    pub fn validate(&self) -> Result<(), String> {
        if self.version_id.trim().is_empty() || self.version_url.trim().is_empty() {
            return Err(keyed_msg!(
                "errors.task.versionInfoIncomplete",
                "版本信息不完整"
            ));
        }
        if let Some(loader) = &self.loader {
            if loader.version.trim().is_empty() {
                let name = loader.kind.display_name();
                return Err(keyed_msg!(
                    "errors.task.versionEmpty?name={name}",
                    "{name} 版本不能为空"
                ));
            }
        }
        if let Some(api) = &self.fabric_api {
            match &self.loader {
                Some(l) if l.kind == LoaderKind::Fabric => {}
                _ => {
                    return Err(keyed_msg!(
                        "errors.task.fabricApiNeedsFabric",
                        "Fabric API 仅支持与 Fabric 加载器组合"
                    ))
                }
            }
            if api.url.trim().is_empty() || api.version.trim().is_empty() {
                return Err(keyed_msg!(
                    "errors.task.fabricApiIncomplete",
                    "Fabric API 信息不完整"
                ));
            }
        }
        Ok(())
    }
}

/// 单文件下载种类。前端传 "mod"/"resourcepack"/"datapack"/"shader"/"world"。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[ts(export_to = "task/")]
pub enum DownloadFileKind {
    #[serde(rename = "mod")]
    Mod,
    #[serde(rename = "resourcepack")]
    ResourcePack,
    #[serde(rename = "datapack")]
    DataPack,
    #[serde(rename = "shader")]
    Shader,
    #[serde(rename = "world")]
    World,
}

impl DownloadFileKind {
    /// 落盘目录（版本隔离：versions/{vdir}/{kind_dir}/{filename}）。
    pub fn kind_dir(&self) -> &'static str {
        match self {
            DownloadFileKind::Mod => "mods",
            DownloadFileKind::ResourcePack => "resourcepacks",
            DownloadFileKind::DataPack => "datapacks",
            DownloadFileKind::Shader => "shaderpacks",
            DownloadFileKind::World => "saves",
        }
    }
}

/// 下载单个文件任务（mod / 资源包 / 数据包 / 光影包）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadFileTask {
    pub version_dir_name: String,
    pub kind: DownloadFileKind,
    pub url: String,
    pub filename: String,
}

impl DownloadFileTask {
    pub fn validate(&self) -> Result<(), String> {
        if self.version_dir_name.trim().is_empty() {
            return Err(keyed_msg!(
                "errors.task.versionDirEmpty",
                "版本目录不能为空"
            ));
        }
        validate_file_item(&self.url, &self.filename)
    }
}

/// 批量文件下载的单个条目（版本目录由 DownloadFilesTask 统一持有）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadFileItem {
    pub kind: DownloadFileKind,
    pub url: String,
    pub filename: String,
    /// 下载成功后要替换掉的旧版本文件名（仅 mods；简单文件名）。
    #[serde(default)]
    pub replace: Vec<String>,
}

/// 批量下载任务；步骤 = 每文件一步，create 时按文件数动态生成。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadFilesTask {
    pub version_dir_name: String,
    #[serde(default)]
    pub files: Vec<DownloadFileItem>,
}

impl DownloadFilesTask {
    pub fn validate(&self) -> Result<(), String> {
        if self.version_dir_name.trim().is_empty() {
            return Err(keyed_msg!(
                "errors.task.versionDirEmpty",
                "版本目录不能为空"
            ));
        }
        if self.files.is_empty() {
            return Err(keyed_msg!("errors.task.fileListEmpty", "文件列表不能为空"));
        }
        for f in &self.files {
            validate_file_item(&f.url, &f.filename)?;
            for name in &f.replace {
                if !crate::util::is_simple_name(name)
                    || self.files.iter().any(|other| other.filename == *name)
                {
                    return Err(keyed_msg!("errors.task.fileNameInvalid", "文件名不合法"));
                }
            }
        }
        Ok(())
    }
}

fn validate_file_item(url: &str, filename: &str) -> Result<(), String> {
    if url.trim().is_empty() {
        return Err(keyed_msg!("errors.task.urlEmpty", "下载地址不能为空"));
    }
    // 挡路径穿越：文件名不得含路径分隔符
    if filename.trim().is_empty() || filename.contains(['/', '\\']) {
        return Err(keyed_msg!("errors.task.fileNameInvalid", "文件名不合法"));
    }
    Ok(())
}

/// 整合包安装任务（单卡片全流程：解析 → 本体 → 内容）；恢复 = 重跑整卡，各环节幂等。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModpackInstallSpec {
    pub request: ModpackRequest,
}

#[cfg(test)]
pub(crate) fn test_valid_task() -> DownloadTask {
    DownloadTask {
        version_id: "26.2".into(),
        version_url: "https://example.com/26.2.json".into(),
        mc_version: "26.2".into(),
        loader: None,
        optifine_patch: None,
        version_dir_name: None,
        fabric_api: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::task::steps::TaskSpec;

    /// 序列化结果与前端 payload 约定的字符串一致（前端 ProgressPayload.status 为 string）。
    #[test]
    fn task_status_serde_lowercase() {
        assert_eq!(
            serde_json::to_string(&TaskStatus::Pending).unwrap(),
            "\"pending\""
        );
        assert_eq!(
            serde_json::to_string(&TaskStatus::Running).unwrap(),
            "\"running\""
        );
        assert_eq!(
            serde_json::to_string(&TaskStatus::Done).unwrap(),
            "\"done\""
        );
        assert_eq!(
            serde_json::to_string(&TaskStatus::Error).unwrap(),
            "\"error\""
        );
        assert_eq!(
            serde_json::to_string(&TaskStatus::Cancelled).unwrap(),
            "\"cancelled\""
        );
        assert_eq!(
            serde_json::to_string(&StepStatus::Pending).unwrap(),
            "\"pending\""
        );
        assert_eq!(
            serde_json::to_string(&StepStatus::Running).unwrap(),
            "\"running\""
        );
        assert_eq!(
            serde_json::to_string(&StepStatus::Done).unwrap(),
            "\"done\""
        );
    }

    #[test]
    fn loader_kind_serde_and_name() {
        assert_eq!(
            serde_json::from_str::<LoaderKind>("\"fabric\"").unwrap(),
            LoaderKind::Fabric
        );
        assert_eq!(
            serde_json::from_str::<LoaderKind>("\"forge\"").unwrap(),
            LoaderKind::Forge
        );
        assert_eq!(
            serde_json::from_str::<LoaderKind>("\"neoforge\"").unwrap(),
            LoaderKind::NeoForge
        );
        assert_eq!(
            serde_json::from_str::<LoaderKind>("\"quilt\"").unwrap(),
            LoaderKind::Quilt
        );
        assert!(serde_json::from_str::<LoaderKind>("\"optifine\"").is_err());
        assert_eq!(LoaderKind::NeoForge.name(), "neoforge");
        assert_eq!(LoaderKind::Quilt.display_name(), "Quilt");
    }

    #[test]
    fn download_file_kind_serde_and_kind_dir() {
        assert_eq!(
            serde_json::from_str::<DownloadFileKind>("\"mod\"").unwrap(),
            DownloadFileKind::Mod
        );
        assert_eq!(
            serde_json::from_str::<DownloadFileKind>("\"resourcepack\"").unwrap(),
            DownloadFileKind::ResourcePack
        );
        assert_eq!(
            serde_json::from_str::<DownloadFileKind>("\"datapack\"").unwrap(),
            DownloadFileKind::DataPack
        );
        assert_eq!(
            serde_json::from_str::<DownloadFileKind>("\"shader\"").unwrap(),
            DownloadFileKind::Shader
        );
        assert_eq!(
            serde_json::from_str::<DownloadFileKind>("\"world\"").unwrap(),
            DownloadFileKind::World
        );
        assert!(serde_json::from_str::<DownloadFileKind>("\"mods\"").is_err());
        assert_eq!(DownloadFileKind::Mod.kind_dir(), "mods");
        assert_eq!(DownloadFileKind::ResourcePack.kind_dir(), "resourcepacks");
        assert_eq!(DownloadFileKind::DataPack.kind_dir(), "datapacks");
        assert_eq!(DownloadFileKind::Shader.kind_dir(), "shaderpacks");
        assert_eq!(DownloadFileKind::World.kind_dir(), "saves");
    }

    #[test]
    fn download_task_validate_ok() {
        assert!(test_valid_task().validate().is_ok());
        let mut t = test_valid_task();
        t.loader = Some(LoaderChoice {
            kind: LoaderKind::Fabric,
            version: "0.19.3".into(),
        });
        assert!(t.validate().is_ok());
        t.fabric_api = Some(FabricApi {
            url: "https://cdn.modrinth.com/a.jar".into(),
            version: "15.0.0".into(),
        });
        assert!(t.validate().is_ok(), "Fabric loader + Fabric API 应合法");
    }

    #[test]
    fn download_task_validate_rejects() {
        let mut t = test_valid_task();
        t.fabric_api = Some(FabricApi {
            url: "u".into(),
            version: "v".into(),
        });
        assert!(t.validate().is_err(), "Fabric API 无 Fabric loader 应拒绝");

        let mut t = test_valid_task();
        t.loader = Some(LoaderChoice {
            kind: LoaderKind::Forge,
            version: "  ".into(),
        });
        assert!(t.validate().is_err(), "空 loader 版本应拒绝");

        let mut t = test_valid_task();
        t.loader = Some(LoaderChoice {
            kind: LoaderKind::Quilt,
            version: "1.0".into(),
        });
        t.fabric_api = Some(FabricApi {
            url: "u".into(),
            version: "v".into(),
        });
        assert!(t.validate().is_err(), "Fabric API 配 Quilt 应拒绝");

        let mut t = test_valid_task();
        t.version_id = "".into();
        assert!(t.validate().is_err(), "空版本 id 应拒绝");
    }

    #[test]
    fn download_file_task_validate() {
        let f = |filename: &str| DownloadFileTask {
            version_dir_name: "26.2-fabric-0.19.3".into(),
            kind: DownloadFileKind::Mod,
            url: "https://example.com/a.jar".into(),
            filename: filename.into(),
        };
        assert!(f("a.jar").validate().is_ok());
        assert!(f("../evil.jar").validate().is_err(), "路径穿越文件名应拒绝");
        assert!(f("a/b.jar").validate().is_err(), "含斜杠文件名应拒绝");
        assert!(f("").validate().is_err(), "空文件名应拒绝");
        let mut t = f("a.jar");
        t.url = "".into();
        assert!(t.validate().is_err(), "空 URL 应拒绝");
    }

    #[test]
    fn download_files_task_validate_and_steps() {
        let item = |filename: &str| DownloadFileItem {
            kind: DownloadFileKind::Mod,
            url: "https://example.com/a.jar".into(),
            filename: filename.into(),
            replace: Vec::new(),
        };
        let t = |files: Vec<DownloadFileItem>| DownloadFilesTask {
            version_dir_name: "26.2-fabric-0.19.3".into(),
            files,
        };
        assert!(t(vec![item("a.jar"), item("b.jar")]).validate().is_ok());
        assert!(t(vec![]).validate().is_err(), "空文件列表应拒绝");
        assert!(
            t(vec![item("../evil.jar")]).validate().is_err(),
            "穿越文件名应拒绝"
        );
        let mut bad_replace = item("a.jar");
        bad_replace.replace.push("../old.jar".into());
        assert!(
            t(vec![bad_replace]).validate().is_err(),
            "穿越替换文件名应拒绝"
        );
        let mut conflicting_replace = item("a.jar");
        conflicting_replace.replace.push("b.jar".into());
        assert!(
            t(vec![conflicting_replace, item("b.jar")])
                .validate()
                .is_err(),
            "替换目标不能同时是本批新文件"
        );
        let mut empty_dir = t(vec![item("a.jar")]);
        empty_dir.version_dir_name = "".into();
        assert!(empty_dir.validate().is_err(), "空版本目录应拒绝");

        // 步骤 = 每文件一步（文件名作步骤名，TaskSpec）
        let batch = t(vec![item("a.jar"), item("b.jar")]);
        let names: Vec<String> = batch.steps().iter().map(|s| s.name.clone()).collect();
        assert_eq!(names, vec!["a.jar", "b.jar"]);
    }
}
