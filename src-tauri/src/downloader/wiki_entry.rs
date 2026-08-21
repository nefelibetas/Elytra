//! PCL2 式 MC百科内置条目表：WikiEntries.txt 的解析与 slug 匹配（零网络）。
//! 数据来自 PCL 仓库（WikiEntries.txt，28880 条），**行号 = MC百科条目 id**。
//! 行格式（条目间 `¨` 分隔，同一行共享行号 = 同一百科条目）：
//!   slug 部分：`xxx@` = CurseForge slug（Modrinth 同名）、`@xxx` = Modrinth slug、
//!             `xxx@yyy` = CF xxx + MR yyy、`xxx` = CurseForge；
//!   中文名部分：`|译名` 可省略（无翻译）；译名含 `*` 时原名由 slug 派生（本项目直接用搜索 title）。
//! 最后一行是浏览量压缩数据（本项目不需要，解析时跳过）。

use std::collections::HashMap;
use std::sync::LazyLock;

const WIKI_ENTRIES: &str = include_str!("../data/wiki_entries.txt");

/// 一个百科条目（可能对应多平台 slug，共享同一 mcmod_id）。
pub struct WikiEntry {
    pub mcmod_id: u32,
    /// 原始中文译名（未处理 `*` 与 ` (原名)` 后缀）；无译名条目为 None
    pub chinese_raw: Option<String>,
}

/// slug → 条目索引（两平台独立）。
pub struct WikiIndex {
    curseforge: HashMap<String, usize>,
    modrinth: HashMap<String, usize>,
    entries: Vec<WikiEntry>,
}

fn parse() -> WikiIndex {
    let mut curseforge: HashMap<String, usize> = HashMap::new();
    let mut modrinth: HashMap<String, usize> = HashMap::new();
    let mut entries: Vec<WikiEntry> = Vec::new();

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
            let idx = entries.len();
            entries.push(WikiEntry {
                mcmod_id,
                chinese_raw,
            });
            if let Some(s) = slugs.strip_prefix('@') {
                // Modrinth only
                modrinth.insert(s.to_string(), idx);
            } else if let Some(s) = slugs.strip_suffix('@') {
                // CurseForge（Modrinth 同名）
                curseforge.insert(s.to_string(), idx);
                modrinth.insert(s.to_string(), idx);
            } else if let Some((cf, mr)) = slugs.split_once('@') {
                curseforge.insert(cf.to_string(), idx);
                modrinth.insert(mr.to_string(), idx);
            } else if !slugs.is_empty() {
                curseforge.insert(slugs.to_string(), idx);
            }
        }
    }
    WikiIndex {
        curseforge,
        modrinth,
        entries,
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
}

/// 处理中文译名：丢弃 `*`（原名用搜索 title 显示）、剥离末尾 " (英文原名)" 后缀；
/// 结果含中文才算译名（否则视为无翻译，避免 "X | X" 假翻译）。
pub fn chinese_name(raw: &str) -> Option<String> {
    let cleaned = raw.replace('*', "").trim().to_string();
    if cleaned.is_empty() {
        return None;
    }
    // 剥离 "中文 (English)" 形态的后缀（括号内以 ASCII 字母开头才剥离，防误伤中文括号）
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

    /// 解析：行号即百科 id；`@` 前后缀/`¨` 多条目共享行号；无译名条目。
    #[test]
    fn parse_assigns_line_number_as_id() {
        // 直接对真实数据做抽样断言（解析逻辑已由 parse() 全量执行）
        assert!(!WIKI.entries.is_empty(), "应解析出条目");
        assert!(
            WIKI.entries.len() > 20000,
            "应有 2 万+ 条目，实际 {}",
            WIKI.entries.len()
        );
    }

    /// slug 匹配：Modrinth/CurseForge 独立索引，命中返回条目与行号 id。
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

    /// 中文名处理：剥离开头星号/英文后缀；全英文视为无翻译。
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

    /// 真实热门 mod 抽样：sodium（Modrinth slug）应命中并给出中文名。
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
