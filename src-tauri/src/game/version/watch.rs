//! 游戏目录变化监听：非递归 watch 激活目录的 `versions/` 与聚焦版本的一级资源子目录。
//! 变化经静默窗口合并后推送；版本列表变化随事件带最新全量列表，前端无需再拉取。
//! 只监听必要路径（非递归）是刻意的：递归整个 `.minecraft` 会在下载与游戏运行期产生事件风暴。

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use parking_lot::Mutex;
use serde::Serialize;
use ts_rs::TS;

use crate::events;
use crate::game::version::dir;
use crate::task::{emit_event, EventSink};

/// 静默窗口：拷贝版本目录会连出多条事件，窗口内合并为一次刷新。
const DEBOUNCE: Duration = Duration::from_millis(300);

/// 聚焦版本目录内监听的一级子目录（前端各列表均取一级条目，不递归）。
const RESOURCE_KINDS: [&str; 5] = ["saves", "mods", "resourcepacks", "datapacks", "shaderpacks"];

/// 版本列表变化（`versions/` 一级条目增删改名，或版本 json 落盘导致 loader 推断变化）。
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export_to = "game/")]
pub struct VersionsChanged {
    /// 激活游戏目录绝对路径（前端比对，丢弃切目录前发出的迟到事件）。
    pub dir: String,
    /// 最新全量列表。
    pub versions: Vec<dir::InstalledVersionInfo>,
}

/// 聚焦版本目录内资源子目录变化。
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export_to = "game/")]
pub struct VersionDirChanged {
    pub version: String,
    /// 发生变化的一级子目录名（`saves`/`mods`/…）。
    pub kinds: Vec<String>,
}

/// 被监听路径的语义，决定事件如何归类。
#[derive(Clone, PartialEq)]
enum Target {
    /// `versions/` 一级条目 = 版本列表变化。
    Versions,
    /// 激活目录本体（`versions/` 尚未创建时的占位）：出现 `versions` 即重定向。
    GameDir,
    /// `versions/<v>` 一级条目 = 资源子目录增删或版本 json 落盘。
    VersionRoot(String),
    /// `versions/<v>/<kind>` = 该资源列表变化。
    VersionResource(String, String),
}

enum Msg {
    Fs(notify::Result<notify::Event>),
    Retarget,
}

struct Inner {
    sink: Arc<dyn EventSink>,
    tx: Sender<Msg>,
    focus: Mutex<Option<String>>,
}

/// 进程级单例（`init` 前为空，`retarget`/`set_focus` 静默忽略）。
static INNER: OnceLock<Arc<Inner>> = OnceLock::new();

/// 启动监听（进程内一次；`lib.rs` 的 setup 调用）。
pub fn init(handle: tauri::AppHandle) {
    init_with_sink(Arc::new(handle));
}

/// 用指定事件出口启动监听（集成测试注入记录型 sink，避免依赖 `AppHandle`）。
pub fn init_with_sink(sink: Arc<dyn EventSink>) {
    if INNER.get().is_some() {
        return;
    }
    let (tx, rx) = channel();
    let inner = Arc::new(Inner {
        sink,
        tx,
        focus: Mutex::new(None),
    });
    if INNER.set(Arc::clone(&inner)).is_err() {
        return;
    }
    if let Err(e) = std::thread::Builder::new()
        .name("game-dir-watch".into())
        .spawn(move || watch_loop(&inner, &rx))
    {
        eprintln!("[watch] 启动监听线程失败：{e}");
        return;
    }
    retarget();
}

/// 重新计算监听目标（激活目录变化后调用）。
pub fn retarget() {
    if let Some(inner) = INNER.get() {
        let _ = inner.tx.send(Msg::Retarget);
    }
}

/// 设置聚焦版本（前端选中版本变化时调用）；未变化时不重复重定向。
pub fn set_focus(version: Option<String>) {
    let Some(inner) = INNER.get() else { return };
    {
        let mut focus = inner.focus.lock();
        if *focus == version {
            return;
        }
        *focus = version;
    }
    retarget();
}

/// 监听线程：收事件 → 静默窗口合并 → 分类推送；`Retarget` 期间到达的批次整体丢弃（多属旧目录）。
fn watch_loop(inner: &Arc<Inner>, rx: &Receiver<Msg>) {
    let tx = inner.tx.clone();
    let mut watcher =
        match notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
            let _ = tx.send(Msg::Fs(res));
        }) {
            Ok(w) => w,
            Err(e) => {
                eprintln!("[watch] 初始化文件监听失败：{e}");
                return;
            }
        };
    let mut watched: Vec<(PathBuf, Target)> = Vec::new();
    loop {
        let Ok(mut msg) = rx.recv() else { return };
        let mut batch: Vec<notify::Result<notify::Event>> = Vec::new();
        loop {
            match msg {
                Msg::Retarget => {
                    batch.clear();
                    apply_targets(inner, &mut watcher, &mut watched);
                }
                Msg::Fs(res) => batch.push(res),
            }
            match rx.recv_timeout(DEBOUNCE) {
                Ok(next) => msg = next,
                Err(RecvTimeoutError::Timeout) => break,
                Err(RecvTimeoutError::Disconnected) => return,
            }
        }
        if !batch.is_empty() {
            dispatch(inner, &watched, batch);
        }
    }
}

/// 期望的监听目标：`versions/`（不存在时退化为激活目录本体）+ 聚焦版本的一级资源子目录。
fn desired_targets(inner: &Inner) -> Vec<(PathBuf, Target)> {
    let mc_dir = dir::active_minecraft_dir();
    let versions_dir = mc_dir.join("versions");
    let mut out = Vec::new();
    if versions_dir.is_dir() {
        out.push((versions_dir.clone(), Target::Versions));
    } else if mc_dir.is_dir() {
        out.push((mc_dir, Target::GameDir));
    }
    let Some(focus) = inner.focus.lock().clone() else {
        return out;
    };
    let version_dir = versions_dir.join(&focus);
    if !version_dir.is_dir() {
        return out;
    }
    out.push((version_dir.clone(), Target::VersionRoot(focus.clone())));
    for kind in RESOURCE_KINDS {
        let path = version_dir.join(kind);
        if path.is_dir() {
            out.push((
                path,
                Target::VersionResource(focus.clone(), kind.to_string()),
            ));
        }
    }
    out
}

/// 增量对齐监听集合（只增删差集，避免整体重建）。
fn apply_targets(
    inner: &Inner,
    watcher: &mut RecommendedWatcher,
    watched: &mut Vec<(PathBuf, Target)>,
) {
    let desired = desired_targets(inner);
    let stale: Vec<PathBuf> = watched
        .iter()
        .filter(|(path, _)| !desired.iter().any(|(want, _)| want == path))
        .map(|(path, _)| path.clone())
        .collect();
    for path in &stale {
        let _ = watcher.unwatch(path);
    }
    watched.retain(|(path, _)| !stale.contains(path));
    for (path, target) in desired {
        if watched.iter().any(|(p, _)| p == &path) {
            continue;
        }
        // 路径可能在两次探测之间被删除（PathNotFound）：跳过，等下一轮重定向
        if watcher.watch(&path, RecursiveMode::NonRecursive).is_ok() {
            watched.push((path, target));
        }
    }
}

/// 命中路径的最长监听目标（notify 报回的路径前缀与 watch 时传入的一致）。
fn longest_match<'a>(
    watched: &'a [(PathBuf, Target)],
    path: &Path,
) -> Option<(&'a Path, &'a Target)> {
    watched
        .iter()
        .filter(|(base, _)| path.starts_with(base))
        .max_by_key(|(base, _)| base.components().count())
        .map(|(base, target)| (base.as_path(), target))
}

/// 相对深度（`path` 落在 `base` 下的层数）；不在其下时为 `None`。
fn relative_depth(base: &Path, path: &Path) -> Option<usize> {
    path.strip_prefix(base)
        .ok()
        .map(|rel| rel.components().count())
}

/// 归类一批事件：`versions/` 一级变化 → 列表事件；聚焦版本内子目录变化 → 资源事件。
/// 返回 `(版本列表是否变化, 变化的资源子目录集合, 是否需要重定向)`。
fn classify(
    watched: &[(PathBuf, Target)],
    batch: &[notify::Result<notify::Event>],
) -> (bool, HashSet<String>, bool) {
    let mut versions_changed = false;
    let mut kinds: HashSet<String> = HashSet::new();
    let mut need_retarget = false;
    for res in batch {
        // 监听错误（目标被删/后端异常）：等下一轮重定向自愈，不推送
        let Ok(event) = res else { continue };
        for path in &event.paths {
            let Some((base, target)) = longest_match(watched, path) else {
                continue;
            };
            match target {
                Target::Versions => versions_changed = true,
                Target::GameDir => {
                    if path.file_name().is_some_and(|name| name == "versions") {
                        need_retarget = true;
                    }
                }
                Target::VersionRoot(_) => match relative_depth(base, path) {
                    // 版本目录本身被增删改名 = `versions/` 一级条目变化（焦点目标不得吞掉该事件）
                    Some(0) => {
                        versions_changed = true;
                        need_retarget = true;
                    }
                    Some(1) => {
                        let name = path
                            .file_name()
                            .map(|n| n.to_string_lossy().to_lowercase())
                            .unwrap_or_default();
                        if RESOURCE_KINDS.contains(&name.as_str()) {
                            kinds.insert(name);
                        } else if name.ends_with(".json") {
                            // 版本 json 落盘/改写：loader 推断结果可能变化，重推列表
                            versions_changed = true;
                        }
                    }
                    _ => {}
                },
                Target::VersionResource(_, kind) => {
                    kinds.insert(kind.clone());
                }
            }
        }
    }
    (versions_changed, kinds, need_retarget)
}

/// 归类后推送：重定向请求回投线程，列表/资源变化各发一条事件。
fn dispatch(
    inner: &Inner,
    watched: &[(PathBuf, Target)],
    batch: Vec<notify::Result<notify::Event>>,
) {
    let (versions_changed, kinds, need_retarget) = classify(watched, &batch);
    if need_retarget {
        let _ = inner.tx.send(Msg::Retarget);
    }
    if versions_changed {
        emit_versions(inner);
    }
    if !kinds.is_empty() {
        let Some(focus) = inner.focus.lock().clone() else {
            return;
        };
        let mut kinds: Vec<String> = kinds.into_iter().collect();
        kinds.sort();
        emit_event(
            inner.sink.as_ref(),
            events::VERSION_DIR_CHANGED,
            VersionDirChanged {
                version: focus,
                kinds,
            },
        );
    }
}

/// 推送全量版本列表（读盘失败时不推送，保留下一次事件机会）。
fn emit_versions(inner: &Inner) {
    let Ok(versions) = dir::list_installed() else {
        return;
    };
    let mc_dir = dir::active_minecraft_dir();
    emit_event(
        inner.sink.as_ref(),
        events::VERSIONS_CHANGED,
        VersionsChanged {
            dir: mc_dir.to_string_lossy().into_owned(),
            versions,
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 测试根目录：相对路径组装，避免盘符与分隔符等平台相关字面量。
    fn root() -> PathBuf {
        PathBuf::from("mc")
    }

    /// 监听目标：`versions/` + 聚焦版本的 mods 目录。
    fn targets() -> Vec<(PathBuf, Target)> {
        let versions = root().join("versions");
        vec![
            (versions.clone(), Target::Versions),
            (
                versions.join("1.20.1").join("mods"),
                Target::VersionResource("1.20.1".into(), "mods".into()),
            ),
        ]
    }

    #[test]
    fn longest_match_picks_deepest_target() {
        let watched = targets();
        let mods = root().join("versions/1.20.1/mods");
        let hit = longest_match(&watched, &mods.join("a.jar"));
        assert_eq!(hit.map(|(base, _)| base.to_path_buf()), Some(mods.clone()));
        // 事件路径等于被监听目录本身（目录被删/改名）时命中自身
        let hit = longest_match(&watched, &mods);
        assert_eq!(hit.map(|(base, _)| base.to_path_buf()), Some(mods.clone()));
        let hit = longest_match(&watched, &root().join("versions/1.21"));
        assert_eq!(
            hit.map(|(base, _)| base.to_path_buf()),
            Some(root().join("versions"))
        );
        assert!(longest_match(&watched, &root().join("assets/x")).is_none());
    }

    #[test]
    fn relative_depth_counts_components() {
        let base = root().join("versions/1.20.1");
        assert_eq!(relative_depth(&base, &base.join("mods")), Some(1));
        assert_eq!(relative_depth(&base, &base.join("mods/a.jar")), Some(2));
        assert_eq!(relative_depth(&base, &root().join("versions")), None);
    }

    /// 构造一条事件（分类只看 `paths`；路径按 `root()` 相对组装）。
    fn event(paths: &[&str]) -> notify::Result<notify::Event> {
        Ok(notify::Event {
            kind: notify::EventKind::Any,
            paths: paths.iter().map(|p| root().join(p)).collect(),
            attrs: notify::event::EventAttributes::default(),
        })
    }

    #[test]
    fn classify_routes_paths_to_changes() {
        let versions_dir = root().join("versions");
        let watched = vec![
            (versions_dir.clone(), Target::Versions),
            (
                versions_dir.join("1.20.1"),
                Target::VersionRoot("1.20.1".into()),
            ),
            (
                versions_dir.join("1.20.1").join("mods"),
                Target::VersionResource("1.20.1".into(), "mods".into()),
            ),
        ];

        // 资源子目录内部条目（mods/a.jar）→ 只报 mods
        let (versions, kinds, retarget) =
            classify(&watched, &[event(&["versions/1.20.1/mods/a.jar"])]);
        assert!(!versions && !retarget);
        assert_eq!(kinds, HashSet::from(["mods".to_string()]));

        // versions/ 一级条目（新增/删除版本目录）→ 列表变化
        let (versions, kinds, _) = classify(&watched, &[event(&["versions/1.21"])]);
        assert!(versions && kinds.is_empty());

        // 聚焦版本的一级资源子目录被创建/删除 → 资源变化
        let (_, kinds, _) = classify(&watched, &[event(&["versions/1.20.1/saves"])]);
        assert_eq!(kinds, HashSet::from(["saves".to_string()]));

        // 版本 json 落盘/改写 → loader 推断可能变化，重推列表
        let (versions, _, _) = classify(&watched, &[event(&["versions/1.20.1/1.20.1.json"])]);
        assert!(versions);

        // 聚焦版本目录本身被删除/改名 → 列表变化（焦点目标不得吞掉该事件）
        let (versions, kinds, _) = classify(&watched, &[event(&["versions/1.20.1"])]);
        assert!(versions && kinds.is_empty());

        // 更深层（存档内部文件）→ 不触发（非递归监听的意义所在）
        let (versions, kinds, _) = classify(
            &watched,
            &[event(&["versions/1.20.1/saves/world/level.dat"])],
        );
        assert!(!versions && kinds.is_empty());

        // versions/ 未创建时的占位目标：目录一出现即重定向
        let placeholder = vec![(root(), Target::GameDir)];
        let (_, _, retarget) = classify(&placeholder, &[event(&["versions"])]);
        assert!(retarget);
    }
}
