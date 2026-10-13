//! MC百科内置条目表（解析 data/wiki_entries.txt，零网络；**行号 = 百科条目 id**）。格式：条目间 `¨` 分隔（同行共享行号）；slug `xxx@`=CF(Modrinth 同名)/`@xxx`=MR/`xxx@yyy`=CF+MR/`xxx`=CF；中文名 `|译名` 可省略（无翻译），含 `*` 时原名由 slug 派生（本项目用搜索 title）；末尾 popularity 行解析时跳过。

use std::collections::HashMap;
use std::sync::LazyLock;

const WIKI_ENTRIES: &str = include_str!("../../data/wiki_entries.txt");

/// 中文词频统计候选上限（取够样本即可，不逐条搜索）。
const CHINESE_MATCH_LIMIT: usize = 200;
/// 词频统计忽略的通用词 / 平台词（作为查询词无区分度）。
const QUERY_STOP_WORDS: [&str; 8] = ["a", "an", "the", "of", "for", "and", "mod", "minecraft"];
/// 高频词的最低出现次数/最短长度（滤掉偶然重复的短词）。
const MIN_WORD_HITS: usize = 2;
const MIN_WORD_LEN: usize = 3;

/// 一个百科条目（可对应多平台 slug，共享同一 mcmod_id）。
pub struct WikiEntry {
    pub mcmod_id: u32,
    /// 原始中文译名（未处理 `*` 与 ` (原名)` 后缀）；无译名为 None
    pub chinese_raw: Option<String>,
    /// CurseForge slug（该条目在此平台收录时）；供中文反查派生英文查询词
    curseforge_slug: Option<String>,
    modrinth_slug: Option<String>,
}

/// slug → 条目索引（两平台独立）。
pub struct WikiIndex {
    curseforge: HashMap<String, usize>,
    modrinth: HashMap<String, usize>,
    entries: Vec<WikiEntry>,
    /// (清洗后中文译名, 条目索引)：解析时一次性建好，避免每次搜索重算
    chinese: Vec<(String, usize)>,
}

/// 中文查询反查命中：把一个中文词映射到该平台上可用的英文查询词。
pub struct ChineseHit {
    /// 该平台可用的英文查询词；两平台都无 slug 且译名不含原名时为 None
    pub query: Option<String>,
}

/// 解析 slug 字段 → (CurseForge slug, Modrinth slug)；格式见文件头。
fn split_slugs(slugs: &str) -> (Option<String>, Option<String>) {
    if let Some(s) = slugs.strip_prefix('@') {
        (None, Some(s.to_string()))
    } else if let Some(s) = slugs.strip_suffix('@') {
        (Some(s.to_string()), Some(s.to_string()))
    } else if let Some((cf, mr)) = slugs.split_once('@') {
        (Some(cf.to_string()), Some(mr.to_string()))
    } else if slugs.is_empty() {
        (None, None)
    } else {
        (Some(slugs.to_string()), None)
    }
}

/// 从原始译名里取 `(English Name)` 括号内的原名（括号内须以 ASCII 字母开头，防误伤中文括号）。
fn english_from_raw(raw: &str) -> Option<String> {
    let cleaned = raw.replace('*', "");
    let open = cleaned.rfind(" (")?;
    if !cleaned.ends_with(')') {
        return None;
    }
    let tail = &cleaned[open + 2..cleaned.len() - 1];
    tail.starts_with(|c: char| c.is_ascii_alphabetic())
        .then(|| tail.to_string())
}

impl WikiEntry {
    /// 派生该平台英文查询词：括号内原名优先，其次由平台 slug 还原（`entity-culling`→`entity culling`）。
    fn derived_query(&self, platform: &str) -> Option<String> {
        if let Some(en) = self.chinese_raw.as_deref().and_then(english_from_raw) {
            return Some(en);
        }
        let slug = if platform == "modrinth" {
            self.modrinth_slug.as_deref()
        } else {
            self.curseforge_slug.as_deref()
        }?;
        let words = slug.replace('-', " ");
        (!words.is_empty()).then_some(words)
    }
}

fn parse() -> WikiIndex {
    let mut curseforge: HashMap<String, usize> = HashMap::new();
    let mut modrinth: HashMap<String, usize> = HashMap::new();
    let mut entries: Vec<WikiEntry> = Vec::new();
    let mut chinese: Vec<(String, usize)> = Vec::new();

    let lines: Vec<&str> = WIKI_ENTRIES.lines().collect();
    let body_len = lines.len().saturating_sub(1); // 跳过最后一行 popularity 压缩数据
    for (i, line) in lines[..body_len].iter().enumerate() {
        if line.is_empty() {
            continue;
        }
        let mcmod_id = (i + 1) as u32; // 行号 = MC百科条目 id
        for entry_data in line.split('¨') {
            let mut parts = entry_data.split('|');
            let slugs = parts.next().unwrap_or("");
            let chinese_raw = parts
                .next()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string);
            let (curseforge_slug, modrinth_slug) = split_slugs(slugs);
            let idx = entries.len();
            entries.push(WikiEntry {
                mcmod_id,
                chinese_raw,
                curseforge_slug,
                modrinth_slug,
            });
            if let Some(s) = entries[idx].curseforge_slug.clone() {
                curseforge.insert(s, idx);
            }
            if let Some(s) = entries[idx].modrinth_slug.clone() {
                modrinth.insert(s, idx);
            }
            // 中文搜索索引：清洗后仍含中文才算译名（与 chinese_name 同一判据）
            if let Some(clean) = entries[idx].chinese_raw.as_deref().and_then(chinese_name) {
                chinese.push((clean, idx));
            }
        }
    }
    WikiIndex {
        curseforge,
        modrinth,
        entries,
        chinese,
    }
}

/// 全局索引（惰性初始化，进程内只解析一次）。
pub static WIKI: LazyLock<WikiIndex> = LazyLock::new(parse);

impl WikiIndex {
    /// 按 slug 查百科条目（platform: "modrinth" | "curseforge"）。
    pub fn lookup(&self, platform: &str, slug: &str) -> Option<&WikiEntry> {
        let map = if platform == "modrinth" {
            &self.modrinth
        } else {
            &self.curseforge
        };
        map.get(slug).map(|&i| &self.entries[i])
    }

    /// 中文反查：译名子串匹配，前缀命中优先、译名短者优先；同一百科条目只保留首个命中。零网络。
    pub fn search_chinese(&self, platform: &str, q: &str, limit: usize) -> Vec<ChineseHit> {
        let q = q.trim();
        if q.is_empty() || limit == 0 {
            return Vec::new();
        }
        // (前缀命中, 译名字符数, chinese 下标)
        let mut hits: Vec<(bool, usize, usize)> = self
            .chinese
            .iter()
            .enumerate()
            .filter_map(|(ci, (name, _))| {
                let pos = name.find(q)?;
                Some((pos == 0, name.chars().count(), ci))
            })
            .collect();
        hits.sort_by_key(|(prefix, len, ci)| (!*prefix, *len, *ci));

        let mut out: Vec<ChineseHit> = Vec::new();
        let mut seen: Vec<u32> = Vec::new();
        for (_, _, ci) in hits {
            let (_, idx) = &self.chinese[ci];
            let entry = &self.entries[*idx];
            if seen.contains(&entry.mcmod_id) {
                continue; // 同一百科条目的多平台条目只取一个
            }
            seen.push(entry.mcmod_id);
            out.push(ChineseHit {
                query: entry.derived_query(platform),
            });
            if out.len() >= limit {
                break;
            }
        }
        out
    }

    /// 中文查询 → 该平台可用的英文查询词（词频策略）：取命中英文名里的**高频词**（如 entity）交平台按热度排序，
    /// 一次请求覆盖整类；只按译名长度取首个命中会因译名长短漏掉目标。无高频词退回首个候选完整英文名；非中文或零命中为 None。
    pub fn query_for_chinese(&self, platform: &str, q: &str) -> Option<String> {
        let q = q.trim();
        if q.is_empty() || !q.chars().any(|c| ('\u{4E00}'..='\u{9FFF}').contains(&c)) {
            return None;
        }
        let queries: Vec<String> = self
            .search_chinese(platform, q, CHINESE_MATCH_LIMIT)
            .into_iter()
            .filter_map(|h| h.query)
            .collect();
        let first = queries.first()?.clone();

        // 只统计长度达标且非停用词的英文单词（小写归一后比较）
        let mut freq: Vec<(String, usize)> = Vec::new();
        for word in queries.iter().flat_map(|s| s.split_whitespace()) {
            let w = word
                .trim_matches(|c: char| !c.is_ascii_alphanumeric())
                .to_lowercase();
            if w.len() < MIN_WORD_LEN || QUERY_STOP_WORDS.contains(&w.as_str()) {
                continue;
            }
            match freq.iter_mut().find(|(k, _)| *k == w) {
                Some((_, n)) => *n += 1,
                None => freq.push((w, 1)),
            }
        }
        // 高频优先，同频取词长者（更具区分度）
        freq.sort_by_key(|(w, n)| (std::cmp::Reverse(*n), std::cmp::Reverse(w.len())));
        match freq.first() {
            Some((w, n)) if *n >= MIN_WORD_HITS => Some(w.clone()),
            _ => Some(first),
        }
    }
}

/// 处理译名：丢弃 `*`（原名用搜索 title）、剥离末尾 " (英文原名)"；结果含中文才算译名（避免 "X | X" 假翻译）。
pub fn chinese_name(raw: &str) -> Option<String> {
    let cleaned = raw.replace('*', "").trim().to_string();
    if cleaned.is_empty() {
        return None;
    }
    // 括号内以 ASCII 字母开头才剥离，防误伤中文括号
    let stripped = match cleaned.rfind(" (") {
        Some(open) if cleaned.ends_with(')') => {
            let tail = &cleaned[open + 2..cleaned.len() - 1];
            if tail.starts_with(|c: char| c.is_ascii_alphabetic()) {
                cleaned[..open].trim().to_string()
            } else {
                cleaned.clone()
            }
        }
        _ => cleaned.clone(),
    };
    let has_cjk = stripped
        .chars()
        .any(|c| ('\u{4E00}'..='\u{9FFF}').contains(&c));
    if has_cjk {
        Some(stripped)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 解析：行号即百科 id；`@` 前后缀/`¨` 多条目共享行号。
    #[test]
    fn parse_assigns_line_number_as_id() {
        // 对真实数据做抽样断言（parse() 已全量执行）
        assert!(!WIKI.entries.is_empty(), "应解析出条目");
        assert!(
            WIKI.entries.len() > 20000,
            "应有 2 万+ 条目，实际 {}",
            WIKI.entries.len()
        );
    }

    /// slug 匹配：两平台独立索引，命中返回条目与行号 id。
    #[test]
    fn lookup_matches_known_slugs() {
        // industrial-craft@|工业时代2 (Industrial Craft 2) —— CF（MR 同名）
        let cf = WIKI.lookup("curseforge", "industrial-craft");
        assert!(cf.is_some(), "industrial-craft 应命中 CF 索引");
        let cf = cf.unwrap();
        assert_eq!(
            cf.mcmod_id, 2,
            "文件首行为空行也占行号：第 2 行 → 百科 id 2"
        );
        assert!(cf
            .chinese_raw
            .as_deref()
            .unwrap_or("")
            .contains("工业时代2"));

        // 同 slug 应同时命中 Modrinth（`@` 结尾 = 双平台）
        assert!(
            WIKI.lookup("modrinth", "industrial-craft").is_some(),
            "CF 结尾 @ 的条目应同时注册到 Modrinth 索引"
        );
    }

    /// 中文反查应能定位含该词的条目并为 entity-culling 派生英文词（中文直发两平台只得到无关结果）。
    #[test]
    fn search_chinese_maps_to_english_query() {
        let hits = WIKI.search_chinese("modrinth", "实体", 200);
        assert!(!hits.is_empty(), "「实体」应至少命中一个条目");
        let queries: Vec<String> = hits.iter().filter_map(|h| h.query.clone()).collect();
        assert!(
            queries
                .iter()
                .any(|q| q.eq_ignore_ascii_case("entity culling")),
            "应能为 entity-culling 派生英文名 Entity Culling（共 {} 项候选）",
            queries.len()
        );
        // 前缀命中 + 短译名优先：首个能派生英文词的是「实体距离」(Entity Distance)；译名更长的 (Entity Culling) 往后靠（前面的短译名只有 CF slug，Modrinth 侧派生不出词）
        assert_eq!(
            hits.iter().find_map(|h| h.query.as_deref()),
            Some("Entity Distance"),
            "首个能派生英文查询词的命中应为前缀匹配且译名最短者"
        );
    }

    /// 无匹配的中文查询返回空（调用方据此保持原查询直发平台）。
    #[test]
    fn search_chinese_returns_empty_when_no_match() {
        assert!(
            WIKI.search_chinese("modrinth", "绝不存在的中文译名词", 5)
                .is_empty(),
            "未命中应返回空"
        );
    }

    /// 中文查询 → 英文词（词频策略）：只取首个候选会漏掉译名长的 (Entity Culling)，故取高频词。
    #[test]
    fn query_for_chinese_prefers_common_word() {
        let q = WIKI.query_for_chinese("modrinth", "实体");
        assert_eq!(
            q.as_deref(),
            Some("entity"),
            "「实体」的命中里 entity 是高频词，应作为宽泛查询词"
        );
    }

    /// 无高频词退回首个候选完整英文名（见实现）；非中文查询不参与反查。
    #[test]
    fn query_for_chinese_ignores_ascii() {
        assert!(WIKI.query_for_chinese("modrinth", "sodium").is_none());
        assert!(WIKI.query_for_chinese("modrinth", "").is_none());
    }

    #[test]
    fn chinese_name_cleanup_rules() {
        assert_eq!(
            chinese_name("工业时代2 (Industrial Craft 2)").as_deref(),
            Some("工业时代2")
        );
        assert_eq!(chinese_name("林业*").as_deref(), Some("林业"), "* 标记丢弃");
        assert_eq!(chinese_name("Fabric API"), None, "全英文视为无翻译");
        assert_eq!(chinese_name(""), None, "空译名返回 None");
        // 括号内是中文不剥离
        assert_eq!(
            chinese_name("苹果（红色）").as_deref(),
            Some("苹果（红色）")
        );
    }

    /// 真实抽样：sodium（Modrinth slug）应命中并给出中文名。
    #[test]
    fn lookup_real_sodium() {
        let entry = WIKI.lookup("modrinth", "sodium");
        assert!(entry.is_some(), "sodium slug 应命中");
        let entry = entry.unwrap();
        assert!(entry.mcmod_id > 0);
        let zh = entry.chinese_raw.as_deref().and_then(chinese_name);
        assert_eq!(zh.as_deref(), Some("钠"), "sodium 中文译名为「钠」");
    }
}
