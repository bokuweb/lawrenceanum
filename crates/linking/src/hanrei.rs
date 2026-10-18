//! 判例 → 法令 クロスリンク。
//!
//! 判例の「参照法条」「判示事項」「裁判要旨」から `法令名 + 第N条` の形の参照を抽出し、
//! `public/links/law-to-hanrei/{law_id}.json` を書き出す。
//!
//! 民法・刑法のような短い法令名は一般語と衝突しうるため、法令名の直後に条番号が
//! 続く場合だけ採用する (長い法令名でも同じ規則で統一)。LLM は呼ばない。

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinkedCase {
    pub case_id: u64,
    pub case_number: String,
    pub case_name: String,
    pub judged_on: String,
    pub court: String,
    pub holding: String,
    /// 参照された条番号 (例: "709", "14", "5_2")。
    pub articles: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LawToHanrei {
    pub schema_version: u32,
    pub law_id: String,
    pub linked_cases: Vec<LinkedCase>,
}

fn kanji_digit(c: char) -> Option<u32> {
    "〇一二三四五六七八九".chars().position(|k| k == c).map(|i| i as u32)
}

/// `１４` / `14` / `七百九` / `三百八十二` → 数値。解釈できなければ None。
fn parse_number(s: &str) -> Option<u32> {
    if s.is_empty() {
        return None;
    }
    if s.chars().all(|c| c.is_ascii_digit() || ('０'..='９').contains(&c)) {
        let ascii: String = s
            .chars()
            .map(|c| match c {
                '０'..='９' => char::from_u32(c as u32 - '０' as u32 + '0' as u32).unwrap(),
                c => c,
            })
            .collect();
        return ascii.parse().ok();
    }
    let (mut total, mut cur) = (0u32, 0u32);
    for c in s.chars() {
        if let Some(d) = kanji_digit(c) {
            cur = cur * 10 + d; // 〇一二… の位取り表記
        } else {
            let unit = match c {
                '十' => 10,
                '百' => 100,
                '千' => 1000,
                _ => return None,
            };
            total += if cur == 0 { 1 } else { cur } * unit;
            cur = 0;
        }
    }
    Some(total + cur)
}

fn is_num_char(c: char) -> bool {
    c.is_ascii_digit()
        || ('０'..='９').contains(&c)
        || "〇一二三四五六七八九十百千".contains(c)
}

/// `text` 先頭の `第?N条(のM)*` を読み、(条番号文字列, 消費バイト数) を返す。
fn parse_article_ref(text: &str) -> Option<(String, usize)> {
    let mut rest = text.strip_prefix('第').unwrap_or(text);
    let take_num = |s: &str| -> String { s.chars().take_while(|c| is_num_char(*c)).collect() };
    let n = take_num(rest);
    let mut art = parse_number(&n)?.to_string();
    rest = &rest[n.len()..];
    rest = rest.strip_prefix('条')?;
    // 枝番「条の二」「条の2の3」
    while let Some(r) = rest.strip_prefix('の') {
        let n = take_num(r);
        match parse_number(&n) {
            Some(v) => {
                art.push('_');
                art.push_str(&v.to_string());
                rest = &r[n.len()..];
            }
            None => break,
        }
    }
    Some((art, text.len() - rest.len()))
}

/// 本文から (law_id, 条番号) の組を抽出する。
fn extract_refs(
    text: &str,
    ac: &aho_corasick::AhoCorasick,
    law_ids: &[&str],
) -> BTreeSet<(String, String)> {
    let mut out = BTreeSet::new();
    for m in ac.find_iter(text) {
        let mut after = &text[m.end()..];
        // 参照法条は「条」が並列に続くことがある (民法709条, 710条)。先頭の 1 つに加え、
        // 区切り (、,・) で続く条番号も同じ法令に属するものとして拾う。
        let mut first = true;
        loop {
            match parse_article_ref(after) {
                Some((art, used)) => {
                    out.insert((law_ids[m.pattern().as_usize()].to_string(), art));
                    after = &after[used..];
                    first = false;
                }
                None if first => break,
                None => break,
            }
            match after.strip_prefix(['、', ',', '，', '・']) {
                Some(r) => after = r,
                None => break,
            }
        }
    }
    out
}

pub fn run_link_hanrei(public: &Path) -> Result<()> {
    let index_path = public.join("laws").join("index.json");
    let v: serde_json::Value = serde_json::from_slice(
        &std::fs::read(&index_path).with_context(|| format!("read {}", index_path.display()))?,
    )?;
    let mut titles: Vec<String> = Vec::new();
    let mut ids: Vec<String> = Vec::new();
    for law in v["laws"].as_array().unwrap_or(&vec![]) {
        let (id, title) = (
            law["law_id"].as_str().unwrap_or(""),
            law["title"].as_str().unwrap_or(""),
        );
        if !id.is_empty() && title.chars().count() >= 2 {
            ids.push(id.to_string());
            titles.push(title.to_string());
        }
    }
    let ac = aho_corasick::AhoCorasick::builder()
        .match_kind(aho_corasick::MatchKind::LeftmostLongest)
        .build(&titles)?;
    let id_refs: Vec<&str> = ids.iter().map(String::as_str).collect();

    let dir = public.join("hanrei");
    if !dir.exists() {
        tracing::warn!("no hanrei dir at {}; skipping", dir.display());
        return Ok(());
    }
    let mut by_law: HashMap<String, Vec<LinkedCase>> = HashMap::new();
    for entry in std::fs::read_dir(&dir)? {
        let path = entry?.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json")
            || path.file_name().and_then(|n| n.to_str()) == Some("index.json")
        {
            continue;
        }
        let c: serde_json::Value = serde_json::from_slice(&std::fs::read(&path)?)?;
        let s = |k: &str| c[k].as_str().unwrap_or("").to_string();
        let text = format!("{}\n{}\n{}", s("referenced_laws"), s("holding"), s("summary"));
        let mut per_law: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for (law_id, art) in extract_refs(&text, &ac, &id_refs) {
            per_law.entry(law_id).or_default().push(art);
        }
        for (law_id, articles) in per_law {
            by_law.entry(law_id).or_default().push(LinkedCase {
                case_id: c["case_id"].as_u64().unwrap_or(0),
                case_number: s("case_number"),
                case_name: s("case_name"),
                judged_on: s("judged_on"),
                court: s("court"),
                holding: s("holding"),
                articles,
            });
        }
    }

    let out_dir = public.join("links").join("law-to-hanrei");
    std::fs::create_dir_all(&out_dir)?;
    let mut written = 0usize;
    for (law_id, mut cases) in by_law {
        cases.sort_by(|a, b| b.case_id.cmp(&a.case_id));
        let doc = LawToHanrei { schema_version: 1, law_id: law_id.clone(), linked_cases: cases };
        std::fs::write(
            out_dir.join(format!("{law_id}.json")),
            serde_json::to_string_pretty(&doc)?,
        )?;
        written += 1;
    }
    tracing::info!("link-hanrei: {written} files written");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers() {
        assert_eq!(parse_number("１４"), Some(14));
        assert_eq!(parse_number("709"), Some(709));
        assert_eq!(parse_number("七百九"), Some(709));
        assert_eq!(parse_number("三百八十二"), Some(382));
        assert_eq!(parse_number("十"), Some(10));
        assert_eq!(parse_number("二〇"), Some(20));
    }

    #[test]
    fn article_refs() {
        assert_eq!(parse_article_ref("第七百九条により"), Some(("709".into(), "第七百九条".len())));
        assert_eq!(parse_article_ref("14条"), Some(("14".into(), "14条".len())));
        assert_eq!(parse_article_ref("５条の２の３、"), Some(("5_2_3".into(), "５条の２の３".len())));
        assert_eq!(parse_article_ref("について"), None);
    }

    #[test]
    fn extracts_short_titles_only_with_article() {
        let titles = ["民法", "借地借家法", "刑事訴訟法"].map(String::from);
        let ac = aho_corasick::AhoCorasick::builder()
            .match_kind(aho_corasick::MatchKind::LeftmostLongest)
            .build(&titles)
            .unwrap();
        let ids = ["M", "S", "K"];
        let refs = extract_refs(
            "借地借家法14条、民法709条、710条 民法の趣旨 刑事訴訟法３８２条",
            &ac,
            &ids,
        );
        let got: Vec<_> = refs.into_iter().collect();
        assert_eq!(
            got,
            vec![
                ("K".to_string(), "382".to_string()),
                ("M".to_string(), "709".to_string()),
                ("M".to_string(), "710".to_string()),
                ("S".to_string(), "14".to_string()),
            ]
        );
    }
}
