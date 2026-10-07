//! 本地皮肤收藏域（衣柜「皮肤」半边）：PNG 原文落盘 + 索引持久化。
//! 命令层（commands/skins.rs）只做参数透传与错误展平。
//!
//! 存储布局：
//! - PNG：配置目录 `skins/<id>.png`（原文写入）
//! - 索引：persist section `local_skins`（[`LocalSkinsConfig`]，按添加顺序追加）
//!
//! 索引与文件一致性（索引是唯一事实来源）：
//! - 新增：先写 PNG 再存索引；索引写失败回滚刚写入的 PNG（id 全新，回滚不会丢既有皮肤）。
//! - 删除：先存索引再删 PNG——任何时刻都不会出现「索引指向不存在的文件」；
//!   文件删除失败只留一个不再被引用的孤儿文件（记日志，不影响功能）。
//! - id 来自前端/持久化数据，拼路径前一律过 `util::safe_relative_path`
//!   （拒绝绝对路径、`..`、空段），拼接结果恒在皮肤目录内。

use std::path::PathBuf;

use base64::Engine as _;
use serde::{Deserialize, Serialize};

use crate::constants;
use crate::persist;
use crate::util;

/// 皮肤模型：经典（与官方皮肤 API 的 variant 字符串一致）。
pub const VARIANT_CLASSIC: &str = "CLASSIC";
/// 皮肤模型：纤细（Alex）。
pub const VARIANT_SLIM: &str = "SLIM";

/// 收藏的本地皮肤（persist 索引条目；PNG 原文在 `skins/<id>.png`）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LocalSkin {
    /// 稳定唯一 id（`<毫秒时间戳>-<序号>`），同时是 PNG 文件名
    pub id: String,
    pub name: String,
    /// 模型：`CLASSIC` | `SLIM`
    pub variant: String,
    /// 添加时间（Unix 秒）
    pub added_at: u64,
}

/// 本地皮肤索引（persist section `local_skins`）。
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct LocalSkinsConfig {
    pub skins: Vec<LocalSkin>,
}

/// 返回前端的皮肤条目：索引字段 + 派生信息（绝对路径、PNG base64）。
/// 派生字段**不入库**——路径与贴图由配置目录 + 文件实时派生，
/// 配置目录迁移/文件变动后自动跟随，不会存成过期快照。
#[derive(Debug, Clone, Serialize)]
pub struct LocalSkinView {
    pub id: String,
    pub name: String,
    pub variant: String,
    pub added_at: u64,
    /// 衣柜内 PNG 绝对路径（前端可直接交给 `upload_skin_file` 应用）
    pub path: String,
    /// PNG base64（前端 3D 预览）；文件缺失时为空串
    pub skin: String,
}

/// 皮肤 PNG 存放目录（配置目录下 `skins/`）。
pub fn skins_dir() -> PathBuf {
    util::config_dir().join("skins")
}

/// 皮肤 PNG 绝对路径。id 为外部输入，**拼路径前必须安全校验**。
fn skin_path(id: &str) -> Result<PathBuf, String> {
    // 先校验 id 本身：空 id 会先被拼成 ".png"（合法文件名）从而逃过后面的段校验
    let _ = util::safe_relative_path(id)?;
    Ok(skins_dir().join(util::safe_relative_path(&format!("{id}.png"))?))
}

/// 读取索引（未保存过 → 空）。
fn read_config() -> LocalSkinsConfig {
    persist::read(constants::SECT_LOCAL_SKINS).unwrap_or_default()
}

/// 皮肤列表（添加时间倒序，最新在前）。
pub fn list() -> Vec<LocalSkin> {
    let mut skins = read_config().skins;
    sort_newest_first(&mut skins);
    skins
}

/// 皮肤列表 + 派生信息（绝对路径 / PNG base64），供前端衣柜网格一次渲染。
pub fn list_views() -> Vec<LocalSkinView> {
    list().into_iter().map(to_view).collect()
}

/// 保存皮肤到衣柜（PNG base64）。
pub fn save(name: &str, b64: &str, variant: &str) -> Result<LocalSkinView, String> {
    let bytes = decode_b64(b64)?;
    save_bytes(name, &bytes, variant)
}

/// 保存本地 PNG 文件到衣柜（前端经 dialog 只拿到路径，WebView 无 fs 插件读不了字节）。
pub fn save_path(name: &str, file_path: &str, variant: &str) -> Result<LocalSkinView, String> {
    let bytes = std::fs::read(file_path).map_err(|e| format!("读取皮肤文件失败\n{e}"))?;
    save_bytes(name, &bytes, variant)
}

/// 删除皮肤（幂等：索引无此项、PNG 不存在均视为成功），返回删除后的列表。
pub fn remove(id: &str) -> Result<Vec<LocalSkinView>, String> {
    let path = skin_path(id)?; // 校验先于任何写操作
    let mut cfg = read_config();
    cfg.skins.retain(|s| s.id != id);
    persist::save(constants::SECT_LOCAL_SKINS, &cfg)
        .map_err(|e| format!("保存皮肤索引失败\n{e}"))?;
    match std::fs::remove_file(&path) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => eprintln!("[skins] 删除皮肤文件失败（索引已更新，仅剩孤儿文件）: {e}"),
    }
    Ok(list_views())
}

/// 读取皮肤 PNG（base64），供前端点击预览/应用时单独刷新。
pub fn read(id: &str) -> Result<String, String> {
    let path = skin_path(id)?;
    let bytes = std::fs::read(&path).map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => format!("皮肤文件不存在\n{id}"),
        _ => format!("读取皮肤文件失败\n{e}"),
    })?;
    Ok(util::to_base64(&bytes))
}

/// 写 PNG + 索引（save / save_path 共用尾段）。
fn save_bytes(name: &str, bytes: &[u8], variant: &str) -> Result<LocalSkinView, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("皮肤名称不能为空\nname is empty".to_string());
    }
    let variant = normalize_variant(variant)?;
    ensure_png(bytes)?;

    let mut cfg = read_config();
    let entry = LocalSkin {
        id: new_id(&cfg.skins),
        name: name.to_string(),
        variant,
        added_at: now_secs(),
    };
    let path = skin_path(&entry.id)?;
    std::fs::create_dir_all(skins_dir()).map_err(|e| format!("创建皮肤目录失败\n{e}"))?;
    std::fs::write(&path, bytes).map_err(|e| format!("保存皮肤文件失败\n{e}"))?;

    // 索引 upsert（同 id 覆盖，防重复条目）；写失败则回滚 PNG，
    // 保持「索引是事实来源、无孤儿文件」——新 id 必然无旧文件，回滚不会丢数据。
    upsert(&mut cfg.skins, entry.clone());
    if let Err(e) = persist::save(constants::SECT_LOCAL_SKINS, &cfg) {
        if let Err(re) = std::fs::remove_file(&path) {
            eprintln!("[skins] 回滚皮肤文件失败（索引未写入，该文件未被引用）: {re}");
        }
        return Err(format!("保存皮肤索引失败\n{e}"));
    }
    Ok(to_view(entry))
}

/// 索引条目 → 前端视图（补绝对路径与 PNG base64；读文件失败时 base64 为空串，前端显示占位）。
fn to_view(entry: LocalSkin) -> LocalSkinView {
    let (path, skin) = match skin_path(&entry.id) {
        Ok(p) => {
            let skin = std::fs::read(&p)
                .map(|b| util::to_base64(&b))
                .unwrap_or_default();
            (p.to_string_lossy().to_string(), skin)
        }
        Err(_) => (String::new(), String::new()),
    };
    LocalSkinView {
        id: entry.id,
        name: entry.name,
        variant: entry.variant,
        added_at: entry.added_at,
        path,
        skin,
    }
}

/// 添加时间倒序（最新在前）。索引按添加顺序追加 ⇒ 先反转再稳定排序，
/// 使同一秒内添加的多条也保持「后添加的在前」。
fn sort_newest_first(skins: &mut [LocalSkin]) {
    skins.reverse();
    skins.sort_by_key(|a| std::cmp::Reverse(a.added_at));
}

/// 索引 upsert：同 id 覆盖（保持原位置），否则追加。
fn upsert(list: &mut Vec<LocalSkin>, entry: LocalSkin) {
    match list.iter_mut().find(|s| s.id == entry.id) {
        Some(slot) => *slot = entry,
        None => list.push(entry),
    }
}

/// 生成稳定唯一 id：`<毫秒时间戳>-<序号>`，序号在冲突时递增（不引入 uuid 依赖）。
/// 只含数字与 `-`，天然通过 `safe_relative_path`。
fn new_id(existing: &[LocalSkin]) -> String {
    next_id(now_millis(), existing)
}

/// 按给定时间戳生成不冲突 id（纯函数，便于单测）。
fn next_id(ts_millis: u128, existing: &[LocalSkin]) -> String {
    let mut n = 0u32;
    loop {
        let id = format!("{ts_millis}-{n}");
        if !existing.iter().any(|s| s.id == id) {
            return id;
        }
        n += 1;
    }
}

/// 归一化模型字符串：大小写不敏感，仅接受 CLASSIC / SLIM。
fn normalize_variant(variant: &str) -> Result<String, String> {
    let v = variant.trim().to_ascii_uppercase();
    if v == VARIANT_CLASSIC || v == VARIANT_SLIM {
        return Ok(v);
    }
    Err(format!(
        "不支持的皮肤模型\nvariant 仅支持 {VARIANT_CLASSIC}/{VARIANT_SLIM}，收到 {variant:?}"
    ))
}

/// PNG 魔数校验：落盘扩展名固定 `.png`，拒绝把任意字节写成皮肤文件
/// （与 `util::is_supported_image` 不同——衣柜皮肤会被游戏/官方 API 消费，只接受 PNG）。
fn ensure_png(bytes: &[u8]) -> Result<(), String> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Ok(());
    }
    Err("皮肤文件不是 PNG 格式\n期望 PNG 魔数 \\x89PNG\\r\\n\\x1a\\n".to_string())
}

/// base64 → 字节（容忍 `data:image/png;base64,` 前缀：前端 canvas 导出常见）。
fn decode_b64(b64: &str) -> Result<Vec<u8>, String> {
    let raw = b64.trim();
    let raw = raw.rsplit_once("base64,").map_or(raw, |(_, r)| r);
    base64::engine::general_purpose::STANDARD
        .decode(raw)
        .map_err(|e| format!("皮肤数据不是合法 base64\n{e}"))
}

/// 当前 Unix 秒。
fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// 当前 Unix 毫秒。
fn now_millis() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn skin(id: &str, name: &str, added_at: u64) -> LocalSkin {
        LocalSkin {
            id: id.to_string(),
            name: name.to_string(),
            variant: VARIANT_CLASSIC.to_string(),
            added_at,
        }
    }

    /// 时间戳相同则序号递增，避开已占用 id。
    #[test]
    fn next_id_skips_taken_ids() {
        assert_eq!(next_id(7, &[]), "7-0");
        let taken = vec![skin("7-0", "a", 0), skin("7-1", "b", 0)];
        assert_eq!(next_id(7, &taken), "7-2");
    }

    /// 生成的 id 通过路径安全校验（无 `..` / 分隔符）。
    #[test]
    fn generated_id_passes_path_guard() {
        let id = next_id(1_700_000_000_000, &[]);
        assert!(util::safe_relative_path(&format!("{id}.png")).is_ok());
    }

    /// 同 id 覆盖保持原位置，新 id 追加（不产生重复条目）。
    #[test]
    fn upsert_overwrites_same_id_in_place() {
        let mut list = vec![skin("1-0", "a", 1), skin("1-1", "b", 2)];
        upsert(&mut list, skin("1-0", "a2", 3));
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].name, "a2");
        assert_eq!(list[0].added_at, 3);

        upsert(&mut list, skin("1-2", "c", 4));
        assert_eq!(list.len(), 3);
        assert_eq!(list.last().unwrap().id, "1-2");
    }

    /// 同秒条目按「后添加的在前」，不同秒按时间倒序。
    #[test]
    fn sort_newest_first_orders_by_time_then_insertion() {
        let mut list = vec![
            skin("1-0", "old", 100),
            skin("1-1", "mid", 200),
            skin("1-2", "new", 200),
        ];
        sort_newest_first(&mut list);
        let ids: Vec<&str> = list.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(ids, ["1-2", "1-1", "1-0"]);
    }

    /// 模型字符串大小写不敏感，非法值报错。
    #[test]
    fn normalize_variant_accepts_known_models_only() {
        assert_eq!(normalize_variant("slim").unwrap(), VARIANT_SLIM);
        assert_eq!(normalize_variant(" Classic ").unwrap(), VARIANT_CLASSIC);
        assert!(normalize_variant("AUTO").is_err());
    }

    /// 非 PNG 字节被拒绝。
    #[test]
    fn ensure_png_rejects_non_png() {
        assert!(ensure_png(b"\x89PNG\r\n\x1a\nrest").is_ok());
        assert!(ensure_png(b"<html>").is_err());
        assert!(ensure_png(&[]).is_err());
    }

    /// base64 解码容忍 data URL 前缀，非法输入报错。
    #[test]
    fn decode_b64_accepts_data_url_prefix() {
        assert_eq!(decode_b64("Zm9v").unwrap(), b"foo");
        assert_eq!(decode_b64("data:image/png;base64,Zm9v").unwrap(), b"foo");
        assert!(decode_b64("not base64!!!").is_err());
    }

    /// 路径穿越 / 绝对路径 / 空 id 一律拒绝（拼路径前守卫）。
    #[test]
    fn skin_path_rejects_unsafe_ids() {
        assert!(skin_path("").is_err());
        assert!(skin_path("../evil").is_err());
        assert!(skin_path("a/../../b").is_err());
        assert!(skin_path("/abs").is_err());
        assert!(skin_path("a//b").is_err());
        let ok = skin_path("1700000000000-0").unwrap();
        assert!(
            ok.ends_with("skins/1700000000000-0.png") || ok.ends_with("skins\\1700000000000-0.png")
        );
    }
}
