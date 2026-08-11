// ── 任务数据模型（前端经 invoke 传入的统一配置入口）──

use serde::{Deserialize, Serialize};

/// 任务状态（进度事件 payload，前端 ProgressPayload.status 为 string）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum TaskStatus {
    #[default]
    Pending,
    Running,
    Done,
    Error,
    Cancelled,
}

/// 步骤状态（进度事件 payload）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum StepStatus {
    #[default]
    Pending,
    Running,
    Done,
}

/// 任务进度快照（task-progress 事件推送体）。
#[derive(Debug, Clone, Serialize)]
pub struct TaskProgress {
    pub task_id: String,
    pub name: String,
    pub status: TaskStatus, // pending | running | done | error | cancelled（serde lowercase）
    pub progress: f64,      // 0.0 – 100.0
    pub speed_bps: f64,
    pub completed_files: u32,
    pub total_files: u32,
    pub completed_bytes: u64,
    pub total_bytes: u64,
    pub current_phase: String,
    pub steps: Vec<StepInfo>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct StepInfo {
    pub name: String,
    pub status: StepStatus, // pending | running | done
    pub progress: f64,      // 0–100, meaningful only when status=running
}

/// ModLoader 种类。前端传 "fabric"/"forge"/"neoforge"/"quilt"。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoaderChoice {
    pub kind: LoaderKind,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FabricApi {
    pub url: String,
    pub version: String,
}

/// 下载游戏本体任务（原版，或带 ModLoader/OptiFine/Fabric API）。
#[derive(Debug, Clone, Serialize, Deserialize)]
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
            return Err("版本信息不完整".into());
        }
        if let Some(loader) = &self.loader {
            if loader.version.trim().is_empty() {
                return Err(format!("{} 版本不能为空", loader.kind.display_name()));
            }
        }
        if let Some(api) = &self.fabric_api {
            match &self.loader {
                Some(l) if l.kind == LoaderKind::Fabric => {}
                _ => return Err("Fabric API 仅支持与 Fabric 加载器组合".into()),
            }
            if api.url.trim().is_empty() || api.version.trim().is_empty() {
                return Err("Fabric API 信息不完整".into());
            }
        }
        Ok(())
    }
}

/// 单文件下载种类。前端传 "mod"/"resourcepack"/"datapack"/"shader"。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DownloadFileKind {
    #[serde(rename = "mod")]
    Mod,
    #[serde(rename = "resourcepack")]
    ResourcePack,
    #[serde(rename = "datapack")]
    DataPack,
    #[serde(rename = "shader")]
    Shader,
}

impl DownloadFileKind {
    /// 落盘目录（版本隔离：versions/{vdir}/{kind_dir}/{filename}）。
    pub fn kind_dir(&self) -> &'static str {
        match self {
            DownloadFileKind::Mod => "mods",
            DownloadFileKind::ResourcePack => "resourcepacks",
            DownloadFileKind::DataPack => "datapacks",
            DownloadFileKind::Shader => "shaders",
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
            return Err("版本目录不能为空".into());
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
}

/// 批量下载任务（一次下载多个 mod / 资源包 / 数据包 / 光影包）。
/// 步骤 = 每文件一步，create 时按文件数动态生成。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadFilesTask {
    pub version_dir_name: String,
    #[serde(default)]
    pub files: Vec<DownloadFileItem>,
}

impl DownloadFilesTask {
    pub fn validate(&self) -> Result<(), String> {
        if self.version_dir_name.trim().is_empty() {
            return Err("版本目录不能为空".into());
        }
        if self.files.is_empty() {
            return Err("文件列表不能为空".into());
        }
        for f in &self.files {
            validate_file_item(&f.url, &f.filename)?;
        }
        Ok(())
    }
}

fn validate_file_item(url: &str, filename: &str) -> Result<(), String> {
    if url.trim().is_empty() {
        return Err("下载地址不能为空".into());
    }
    // 挡路径穿越：文件名不得含路径分隔符
    if filename.trim().is_empty() || filename.contains(['/', '\\']) {
        return Err("文件名不合法".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::task::steps::TaskSpec;

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
        assert!(serde_json::from_str::<DownloadFileKind>("\"mods\"").is_err());
        assert_eq!(DownloadFileKind::Mod.kind_dir(), "mods");
        assert_eq!(DownloadFileKind::ResourcePack.kind_dir(), "resourcepacks");
        assert_eq!(DownloadFileKind::DataPack.kind_dir(), "datapacks");
        assert_eq!(DownloadFileKind::Shader.kind_dir(), "shaders");
    }

    fn valid_task() -> DownloadTask {
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

    #[test]
    fn download_task_validate_ok() {
        assert!(valid_task().validate().is_ok());
        let mut t = valid_task();
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
        let mut t = valid_task();
        t.fabric_api = Some(FabricApi {
            url: "u".into(),
            version: "v".into(),
        });
        assert!(t.validate().is_err(), "Fabric API 无 Fabric loader 应拒绝");

        let mut t = valid_task();
        t.loader = Some(LoaderChoice {
            kind: LoaderKind::Forge,
            version: "  ".into(),
        });
        assert!(t.validate().is_err(), "空 loader 版本应拒绝");

        let mut t = valid_task();
        t.loader = Some(LoaderChoice {
            kind: LoaderKind::Quilt,
            version: "1.0".into(),
        });
        t.fabric_api = Some(FabricApi {
            url: "u".into(),
            version: "v".into(),
        });
        assert!(t.validate().is_err(), "Fabric API 配 Quilt 应拒绝");

        let mut t = valid_task();
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
        };
        let t = |files: Vec<DownloadFileItem>| DownloadFilesTask {
            version_dir_name: "26.2-fabric-0.19.3".into(),
            files,
        };
        assert!(t(vec![item("a.jar"), item("b.jar")]).validate().is_ok());
        assert!(t(vec![]).validate().is_err(), "空文件列表应拒绝");
        assert!(t(vec![item("../evil.jar")]).validate().is_err(), "穿越文件名应拒绝");
        let mut empty_dir = t(vec![item("a.jar")]);
        empty_dir.version_dir_name = "".into();
        assert!(empty_dir.validate().is_err(), "空版本目录应拒绝");

        // 步骤 = 每文件一步（文件名作步骤名，TaskSpec）
        let batch = t(vec![item("a.jar"), item("b.jar")]);
        let names: Vec<String> = batch.steps().iter().map(|s| s.name.clone()).collect();
        assert_eq!(names, vec!["a.jar", "b.jar"]);
    }
}
