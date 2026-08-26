//! MCIM 中文简介翻译：内存缓存 + 批量 POST /translate/{source}。

use std::collections::HashMap;

use super::shared::QUERY_TIMEOUT_SECS;
use crate::downloader::endpoints::CommunityEndpoints;
use crate::downloader::fetcher;

/// 翻译缓存 TTL（10 分钟，低频变化数据，对齐 downloader/source.rs 缓存模式）
const TRANSLATION_CACHE_TTL_SECS: u64 = 600;

/// 翻译缓存：id → (查询时间, 中文简介)。模块级 Mutex 串行化；防翻页/重复搜索反复请求。
static TRANSLATION_CACHE: std::sync::LazyLock<
    parking_lot::Mutex<HashMap<String, (std::time::Instant, String)>>,
> = std::sync::LazyLock::new(|| parking_lot::Mutex::new(HashMap::new()));

/// 解析 MCIM 翻译响应（两源字段名不同：modrinth 用 project_id 字符串、curseforge 用 modid 数字），
/// 返回 id → 中文简介；空简介条目跳过。
fn parse_translations(source: &str, data: &serde_json::Value) -> HashMap<String, String> {
    let Some(arr) = data.as_array() else {
        return HashMap::new();
    };
    let key = if source == "modrinth" {
        "project_id"
    } else {
        "modid"
    };
    arr.iter()
        .filter_map(|v| {
            let id = v[key]
                .as_str()
                .map(str::to_string)
                .or_else(|| v[key].as_u64().map(|n| n.to_string()))?;
            let translated = v["translated"].as_str()?.trim();
            if translated.is_empty() {
                return None;
            }
            Some((id, translated.to_string()))
        })
        .collect()
}

/// 构造 MCIM 翻译批量请求体。
/// 注意：两源字段与类型不同——modrinth 的 `project_ids` 是字符串数组；
/// curseforge 的 `modids` 要求**整数数组**（字符串数组会被 MCIM 400 拒绝，
/// 曾导致 CF 侧全部简介翻译静默丢失）。非数字 id 防御性过滤，全过滤返回 None（跳过请求）。
fn translation_body(source: &str, ids: &[String]) -> Option<serde_json::Value> {
    if source == "modrinth" {
        if ids.is_empty() {
            return None;
        }
        return Some(serde_json::json!({ "project_ids": ids }));
    }
    let modids: Vec<i64> = ids.iter().filter_map(|s| s.parse().ok()).collect();
    if modids.is_empty() {
        return None;
    }
    Some(serde_json::json!({ "modids": modids }))
}

/// 批量查 MCIM 中文简介翻译（POST /translate/{source}）。先查内存缓存（10 分钟 TTL），
/// 只请求未命中的 id；网络失败/无翻译静默降级（返回空 map，不阻塞搜索结果）。
/// pub(crate)：本地 Mod 详情识别（downloader/mod_updates/identify.rs）复用同一翻译通道。
pub(crate) async fn fetch_translations(source: &str, ids: &[String]) -> HashMap<String, String> {
    if ids.is_empty() {
        return HashMap::new();
    }
    let now = std::time::Instant::now();
    let mut result: HashMap<String, String> = HashMap::new();
    let missed: Vec<String> = {
        let cache = TRANSLATION_CACHE.lock();
        ids.iter()
            .filter(|id| match cache.get(*id) {
                Some((at, text))
                    if now.duration_since(*at).as_secs() < TRANSLATION_CACHE_TTL_SECS =>
                {
                    result.insert((*id).clone(), text.clone());
                    false
                }
                _ => true,
            })
            .cloned()
            .collect()
    };
    if missed.is_empty() {
        return result;
    }
    let Some(body) = translation_body(source, &missed) else {
        return result;
    };
    let resp = fetcher::http()
        .post(format!(
            "{}/translate/{}",
            CommunityEndpoints::TRANSLATION_API_BASE,
            source
        ))
        .json(&body)
        .timeout(std::time::Duration::from_secs(QUERY_TIMEOUT_SECS))
        .send()
        .await;
    let data = match resp {
        Ok(r) if r.status().is_success() => match r.json::<serde_json::Value>().await {
            Ok(v) => v,
            Err(_) => return result,
        },
        _ => return result,
    };

    let mut cache = TRANSLATION_CACHE.lock();
    for (id, text) in parse_translations(source, &data) {
        cache.insert(id.clone(), (now, text.clone()));
        result.insert(id, text);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 翻译响应解析：Modrinth 用 project_id（字符串），CurseForge 用 modid（数字）。
    #[test]
    fn parse_translations_two_sources() {
        let mr = serde_json::json!([
            { "project_id": "AANobbMI", "translated": "高性能渲染引擎替代品", "original": "A high-performance..." },
            { "project_id": "P7dR8mSH", "translated": "轻量级模块化 API", "original": "Lightweight..." }
        ]);
        let m = parse_translations("modrinth", &mr);
        assert_eq!(m.len(), 2, "应解析出 2 条翻译");
        assert_eq!(
            m.get("AANobbMI").map(String::as_str),
            Some("高性能渲染引擎替代品")
        );

        let cf = serde_json::json!([
            { "modid": 238222, "translated": "查看物品和配方", "original": "View Items and Recipes" }
        ]);
        let c = parse_translations("curseforge", &cf);
        assert_eq!(
            c.get("238222").map(String::as_str),
            Some("查看物品和配方"),
            "modid 数字应转字符串"
        );
    }

    /// 翻译响应解析容错：非数组 / 空简介 / 缺字段都跳过，不 panic。
    #[test]
    fn parse_translations_tolerates_bad_input() {
        assert!(
            parse_translations("modrinth", &serde_json::json!({})).is_empty(),
            "非数组返回空"
        );
        let with_empty = serde_json::json!([
            { "project_id": "a", "translated": "" },
            { "project_id": "b" }
        ]);
        assert!(
            parse_translations("modrinth", &with_empty).is_empty(),
            "空简介/缺字段跳过"
        );
        let other_source = serde_json::json!([{ "project_id": "x", "translated": "中文" }]);
        assert!(
            parse_translations("curseforge", &other_source).is_empty(),
            "字段名不匹配返回空"
        );
    }

    /// 翻译请求体：Modrinth 用字符串数组；CurseForge 必须整数数组（字符串数组会被 MCIM 400）。
    #[test]
    fn translation_body_source_formats() {
        let mr = translation_body(
            "modrinth",
            &["AANobbMI".to_string(), "P7dR8mSH".to_string()],
        )
        .unwrap();
        assert_eq!(
            mr,
            serde_json::json!({ "project_ids": ["AANobbMI", "P7dR8mSH"] }),
            "Modrinth 用字符串数组"
        );

        let cf =
            translation_body("curseforge", &["238222".to_string(), "394468".to_string()]).unwrap();
        assert_eq!(
            cf,
            serde_json::json!({ "modids": [238222, 394468] }),
            "CurseForge 必须整数数组，字符串数组会 400"
        );
        assert!(
            cf["modids"].as_array().unwrap().iter().all(|v| v.is_i64()),
            "CurseForge modids 元素必须是整数"
        );
    }

    /// 翻译请求体容错：空 ids / 全非数字 id 返回 None（跳过请求）。
    #[test]
    fn translation_body_tolerates_bad_ids() {
        assert!(translation_body("modrinth", &[]).is_none(), "空 ids 不请求");
        assert!(
            translation_body("curseforge", &[]).is_none(),
            "空 ids 不请求"
        );
        assert!(
            translation_body("curseforge", &["abc".to_string()]).is_none(),
            "全非数字 id 不请求"
        );
        let mixed =
            translation_body("curseforge", &["abc".to_string(), "238222".to_string()]).unwrap();
        assert_eq!(
            mixed,
            serde_json::json!({ "modids": [238222] }),
            "非数字 id 过滤，保留有效数字"
        );
    }

    /// MCIM 翻译 API 真实请求（需网络）：sodium 的 Modrinth 内部 ID 应有中文简介。
    #[tokio::test]
    async fn mcim_translation_api_works() {
        let tr = fetch_translations("modrinth", &["AANobbMI".to_string()]).await;
        assert!(
            !tr.is_empty(),
            "MCIM 翻译应返回 sodium 的中文简介，实际: {:?}",
            tr
        );
    }

    /// MCIM 翻译 API 真实请求（需网络）：CurseForge 侧用字符串 id 应正常返回（回归：曾 400）。
    #[tokio::test]
    async fn mcim_translation_curseforge_works() {
        let tr = fetch_translations("curseforge", &["238222".to_string()]).await;
        assert!(
            !tr.is_empty(),
            "CurseForge 翻译应返回 JEI 的中文简介（字符串 id 需转整数数组），实际: {:?}",
            tr
        );
    }
}
