use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};

pub mod runner;

// ── 任务数据模型（前端经 invoke 传入的统一配置入口）──

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

// ── 步骤分析：任务 → 步骤序列（TaskSpec 统一入口，命令层与 runner 共用）──

/// 步骤种类：runner 按 kind 定位执行点（索引动态计算，无硬编码常量）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepKind {
    /// 下载原版 version json（唯一硬前置，恒在，索引 0）
    VersionJson,
    /// client.jar（恒在，与 libraries/assets 并行）
    ClientJar,
    /// 依赖库
    Libraries,
    /// 资源文件
    Assets,
    /// native classifier 下载/解压
    Natives,
    /// ModLoader 配置（合并 json 写入）
    LoaderConfig,
    /// ModLoader 本体（fabric libs / installer→jar）
    LoaderLibs,
    OptiFine,
    FabricApi,
    /// 批量文件下载的单个文件步骤
    File,
}

#[derive(Debug, Clone)]
pub struct StepSpec {
    pub name: String,
    pub kind: StepKind,
}

/// 任务 → 描述（任务名 + 步骤分析）的统一入口。
/// 新任务类型实现本 trait 即获得步骤能力（命令层/状态机/前端零改动）。
pub trait TaskSpec {
    fn name(&self) -> String;
    fn steps(&self) -> Vec<StepSpec>;
}

impl TaskSpec for DownloadTask {
    fn name(&self) -> String {
        match &self.loader {
            Some(l) => format!("{} {}-{} 安装", self.mc_version, l.kind.display_name(), l.version),
            None => format!("{} 下载", self.version_id),
        }
    }

    /// 步骤序列（顺序即执行顺序；零缺失维度由 runner 检测后过滤）。
    fn steps(&self) -> Vec<StepSpec> {
        let mut v = vec![
            StepSpec { name: "下载原版json文件".into(), kind: StepKind::VersionJson },
            StepSpec { name: "下载原版客户端文件".into(), kind: StepKind::ClientJar },
            StepSpec { name: "下载原版支持库文件".into(), kind: StepKind::Libraries },
            StepSpec { name: "下载原版资源文件".into(), kind: StepKind::Assets },
            StepSpec { name: "处理 native 库".into(), kind: StepKind::Natives },
        ];
        if let Some(l) = &self.loader {
            v.push(StepSpec { name: format!("生成 {} 配置", l.kind.display_name()), kind: StepKind::LoaderConfig });
            v.push(StepSpec { name: format!("下载 {} 本体", l.kind.display_name()), kind: StepKind::LoaderLibs });
        }
        if self.optifine_patch.is_some() {
            v.push(StepSpec { name: "安装 OptiFine".into(), kind: StepKind::OptiFine });
        }
        if self.fabric_api.is_some() {
            v.push(StepSpec { name: "下载 Fabric API".into(), kind: StepKind::FabricApi });
        }
        v
    }
}

impl TaskSpec for DownloadFilesTask {
    fn name(&self) -> String {
        format!("下载 {} 个文件", self.files.len())
    }

    /// 每文件一步（文件名作步骤名）。
    fn steps(&self) -> Vec<StepSpec> {
        self.files
            .iter()
            .map(|f| StepSpec { name: f.filename.clone(), kind: StepKind::File })
            .collect()
    }
}

static TM: OnceLock<Arc<TaskManager>> = OnceLock::new();

pub fn init() {
    TM.set(Arc::new(TaskManager::default())).ok();
}

pub fn global() -> &'static Arc<TaskManager> {
    TM.get().expect("TaskManager not initialized")
}

#[derive(Debug, Clone, Serialize)]
pub struct TaskProgress {
    pub task_id: String,
    pub name: String,
    pub status: String, // pending | running | done | error | cancelled
    pub progress: f64,  // 0.0 – 100.0
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
    pub status: String, // pending | running | done | error
    pub progress: f64,  // 0–100, meaningful only when status=running
}

pub struct TaskManager {
    inner: Mutex<Inner>,
}

impl Default for TaskManager {
    fn default() -> Self {
        Self {
            inner: Mutex::new(Inner {
                tasks: HashMap::new(),
                active_paths: HashMap::new(),
                counter: 0,
            }),
        }
    }
}

struct Inner {
    tasks: HashMap<String, TaskHandle>,
    /// 被下载任务占用的路径（版本目录 / 目标文件），同路径并发互斥
    active_paths: HashMap<std::path::PathBuf, ()>,
    counter: u64,
}

struct TaskHandle {
    cancel: Arc<AtomicBool>,
    progress: Arc<Mutex<TaskProgress>>,
    cleanup_path: Option<std::path::PathBuf>,
}

impl TaskManager {
    /// Register a new task, return (task_id, cancel_token, progress_guard).
    pub fn create(
        &self,
        name: &str,
        steps: &[&str],
    ) -> (String, Arc<AtomicBool>, Arc<Mutex<TaskProgress>>) {
        let mut inner = self.inner.lock().unwrap();
        let id = format!("t{}", inner.counter);
        inner.counter += 1;
        let cancel = Arc::new(AtomicBool::new(false));
        let progress = Arc::new(Mutex::new(TaskProgress {
            task_id: id.clone(),
            name: name.to_string(),
            status: "pending".into(),
            progress: 0.0,
            speed_bps: 0.0,
            completed_files: 0,
            total_files: 0,
            completed_bytes: 0,
            total_bytes: 0,
            current_phase: "等待中".into(),
            steps: steps
                .iter()
                .map(|s| StepInfo {
                    name: s.to_string(),
                    status: "pending".into(),
                    progress: 0.0,
                })
                .collect(),
            error: None,
        }));
        inner.tasks.insert(
            id.clone(),
            TaskHandle {
                cancel: cancel.clone(),
                progress: progress.clone(),
                cleanup_path: None,
            },
        );
        (id, cancel, progress)
    }

    /// 独占占用一个路径（版本目录/目标文件）；同路径已被占用返回 false。
    pub fn acquire(&self, path: &std::path::Path) -> bool {
        let mut inner = self.inner.lock().unwrap();
        if inner.active_paths.contains_key(path) {
            return false;
        }
        inner.active_paths.insert(path.to_path_buf(), ());
        true
    }

    pub fn release(&self, path: &std::path::Path) {
        self.inner.lock().unwrap().active_paths.remove(path);
    }

    pub fn set_cleanup_path(&self, id: &str, path: std::path::PathBuf) {
        if let Some(h) = self.inner.lock().unwrap().tasks.get_mut(id) {
            h.cleanup_path = Some(path);
        }
    }

    pub fn cancel(&self, id: &str) -> bool {
        let inner = self.inner.lock().unwrap();
        if let Some(h) = inner.tasks.get(id) {
            h.cancel.store(true, Ordering::SeqCst);
            if let Some(ref p) = h.cleanup_path {
                let _ = std::fs::remove_dir_all(p);
            }
            if let Ok(mut p) = h.progress.try_lock() {
                p.status = "cancelled".into();
            }
            true
        } else {
            false
        }
    }

    pub fn fail(&self, id: &str) {
        self.inner.lock().unwrap().tasks.remove(id);
    }

    pub fn list(&self) -> Vec<TaskProgress> {
        let inner = self.inner.lock().unwrap();
        inner
            .tasks
            .values()
            .filter_map(|h| h.progress.try_lock().ok().map(|p| p.clone()))
            .collect()
    }

    pub fn remove(&self, id: &str) {
        self.inner.lock().unwrap().tasks.remove(id);
    }
}

pub struct PathLock {
    tm: Arc<TaskManager>,
    path: std::path::PathBuf,
}

impl PathLock {
    pub fn try_acquire(tm: &Arc<TaskManager>, path: &std::path::Path) -> Option<PathLock> {
        if tm.acquire(path) {
            Some(PathLock {
                tm: tm.clone(),
                path: path.to_path_buf(),
            })
        } else {
            None
        }
    }
}

impl Drop for PathLock {
    fn drop(&mut self) {
        self.tm.release(&self.path);
    }
}

pub fn emit(app: &AppHandle, p: &TaskProgress) {
    let _ = app.emit(crate::events::TASK_PROGRESS, p);
}

pub fn update_and_emit(
    app: &AppHandle,
    guard: &Arc<Mutex<TaskProgress>>,
    f: impl FnOnce(&mut TaskProgress),
) {
    if let Ok(mut p) = guard.lock() {
        f(&mut p);
        emit(app, &p);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn download_task_steps_analysis() {
        // 仅原版：json/client/libs/assets/natives
        let names = |t: &DownloadTask| {
            t.steps().iter().map(|s| s.name.clone()).collect::<Vec<_>>()
        };
        assert_eq!(
            names(&valid_task()),
            vec!["下载原版json文件", "下载原版客户端文件", "下载原版支持库文件", "下载原版资源文件", "处理 native 库"]
        );

        // Fabric + Fabric API：追加 生成配置/下载本体/Fabric API
        let mut t = valid_task();
        t.loader = Some(LoaderChoice { kind: LoaderKind::Fabric, version: "0.19.3".into() });
        t.fabric_api = Some(FabricApi { url: "u".into(), version: "15.0.0".into() });
        let n = names(&t);
        assert_eq!(
            n,
            vec![
                "下载原版json文件", "下载原版客户端文件", "下载原版支持库文件",
                "下载原版资源文件", "处理 native 库", "生成 Fabric 配置",
                "下载 Fabric 本体", "下载 Fabric API"
            ]
        );

        // OptiFine：追加步骤
        let mut t = valid_task();
        t.optifine_patch = Some("G8".into());
        assert_eq!(names(&t).last().unwrap(), "安装 OptiFine");

        // 任务名
        assert_eq!(valid_task().name(), "26.2 下载");
        let mut t = valid_task();
        t.loader = Some(LoaderChoice { kind: LoaderKind::Quilt, version: "1.0".into() });
        assert_eq!(t.name(), "26.2 Quilt-1.0 安装");
    }

    #[test]
    fn path_lock_acquire_release() {
        let tm = Arc::new(TaskManager::default());
        let p = std::path::Path::new("C:/test/versions/26.2");
        assert!(tm.acquire(p));
        assert!(!tm.acquire(p), "同路径二次占用应失败");
        tm.release(p);
        assert!(tm.acquire(p), "释放后可再次占用");
        tm.release(p);
    }

    #[test]
    fn path_lock_guard_drops_automatically() {
        let tm = Arc::new(TaskManager::default());
        let p = std::path::Path::new("C:/test/versions/26.2");
        {
            let _g = PathLock::try_acquire(&tm, p).expect("首次占用应成功");
            assert!(!tm.acquire(p), "守卫持有期间占用应失败");
        }
        assert!(tm.acquire(p), "守卫 Drop 后占用应成功");
        tm.release(p);
    }
}
