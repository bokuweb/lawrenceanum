//! 質問主意書・答弁書 → 法令 クロスリンク。
//!
//! 件名・質問本文・答弁本文から法令参照を抽出し、
//! `public/links/law-to-shitsumon/{law_id}.json` を書き出す。
//!
//! 法令名の解決は [`crate::law_refs`] (判例と共通)。短い法令名は条番号が続く場合だけ採用し、
//! [`BARE_MIN_CHARS`] 文字以上の法令名は条番号が無くても法令単位の言及として拾う
//! (質問主意書は「〇〇法に基づく…」のように条を挙げずに法令を論じることが多いため)。

use crate::law_refs::LawMatcher;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

/// 条番号を伴わない言及を採用する法令名の最小文字数 (「労働基準法」「地方自治法」以上)。
pub const BARE_MIN_CHARS: usize = 5;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinkedQuestion {
    /// `{house}-{session}-{number}` (例: `shugiin-219-1`)。
    pub id: String,
    pub house: String,
    pub session: u32,
    pub number: u32,
    pub title: String,
    pub submitter: String,
    pub submitted_on: Option<String>,
    pub answered_on: Option<String>,
    /// 件名に法令名が出てくるなら true (その法令を主題にした質問)。
    pub in_title: bool,
    /// 参照された条番号 (例: "74", "5_2")。条を挙げない言及だけなら空。
    pub articles: Vec<String>,
    pub detail_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LawToShitsumon {
    pub schema_version: u32,
    pub law_id: String,
    pub linked_questions: Vec<LinkedQuestion>,
}

pub fn run_link_shitsumon(public: &Path) -> Result<()> {
    let matcher = LawMatcher::from_index(public)?;
    let dir = public.join("shitsumon");
    if !dir.exists() {
        tracing::warn!("no shitsumon dir at {}; skipping", dir.display());
        return Ok(());
    }
    let mut by_law: HashMap<String, Vec<LinkedQuestion>> = HashMap::new();
    for entry in std::fs::read_dir(&dir)? {
        let path = entry?.path();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if path.extension().and_then(|e| e.to_str()) != Some("json")
            || name == "index.json"
            || name.starts_with("._")
        {
            continue;
        }
        let q: serde_json::Value = serde_json::from_slice(&std::fs::read(&path)?)?;
        let s = |k: &str| q[k].as_str().unwrap_or("").to_string();
        let opt = |k: &str| q[k].as_str().map(str::to_string);
        let title = s("title");
        let text = format!("{title}\n{}\n{}", s("question_text"), s("answer_text"));
        let in_title = matcher.scan(&title, Some(BARE_MIN_CHARS));
        for (law_id, articles) in matcher.scan(&text, Some(BARE_MIN_CHARS)) {
            by_law
                .entry(law_id.clone())
                .or_default()
                .push(LinkedQuestion {
                    id: s("id"),
                    house: s("house"),
                    session: q["session"].as_u64().unwrap_or(0) as u32,
                    number: q["number"].as_u64().unwrap_or(0) as u32,
                    title: title.clone(),
                    submitter: s("submitter"),
                    submitted_on: opt("submitted_on"),
                    answered_on: opt("answered_on"),
                    in_title: in_title.contains_key(&law_id),
                    articles: articles.into_iter().collect(),
                    detail_url: s("detail_url"),
                });
        }
    }

    let out_dir = public.join("links").join("law-to-shitsumon");
    std::fs::create_dir_all(&out_dir)?;
    let mut written = 0usize;
    for (law_id, mut qs) in by_law {
        // 新しい順。同日は回次・番号の降順。
        qs.sort_by(|a, b| {
            (&b.submitted_on, b.session, b.number).cmp(&(&a.submitted_on, a.session, a.number))
        });
        let doc = LawToShitsumon {
            schema_version: 1,
            law_id: law_id.clone(),
            linked_questions: qs,
        };
        std::fs::write(
            out_dir.join(format!("{law_id}.json")),
            serde_json::to_string_pretty(&doc)?,
        )?;
        written += 1;
    }
    tracing::info!("link-shitsumon: {written} files written");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn links_questions_end_to_end() {
        let dir =
            std::env::temp_dir().join(format!("lawpub_shitsumon_link_{}", std::process::id()));
        std::fs::create_dir_all(dir.join("laws")).unwrap();
        std::fs::create_dir_all(dir.join("shitsumon")).unwrap();
        std::fs::write(
            dir.join("laws").join("index.json"),
            serde_json::to_vec(&serde_json::json!({"laws": [
                {"law_id": "NYUKAN", "title": "出入国管理及び難民認定法"},
                {"law_id": "MINPO", "title": "民法"},
                {"law_id": "SABETSU", "title": "障害を理由とする差別の解消の推進に関する法律"},
            ]}))
            .unwrap(),
        )
        .unwrap();
        std::fs::write(
            dir.join("shitsumon").join("shugiin-222-9.json"),
            serde_json::to_vec(&serde_json::json!({
                "id": "shugiin-222-9", "house": "shugiin", "session": 222, "number": 9,
                "title": "出入国管理及び難民認定法に基づく入国拒否理由に関する質問主意書",
                "submitter": "某", "submitted_on": "2026-10-14", "answered_on": null,
                "question_text": "同法第五条第一項第九号の運用について問う。民法の趣旨にも反する。",
                "answer_text": null,
                "detail_url": "https://example.invalid/222009.htm"
            }))
            .unwrap(),
        )
        .unwrap();
        std::fs::write(
            dir.join("shitsumon").join("sangiin-219-3.json"),
            serde_json::to_vec(&serde_json::json!({
                "id": "sangiin-219-3", "house": "sangiin", "session": 219, "number": 3,
                "title": "在宅支援に関する質問主意書",
                "submitter": "某", "submitted_on": "2025-10-21", "answered_on": "2025-10-31",
                "question_text": "障害者差別解消法の趣旨に反するおそれがある。",
                "answer_text": "民法第七百九条の適用は個別に判断される。",
                "detail_url": "https://example.invalid/m219003.htm"
            }))
            .unwrap(),
        )
        .unwrap();
        std::fs::write(dir.join("shitsumon").join("index.json"), b"{}").unwrap();

        run_link_shitsumon(&dir).unwrap();

        let read = |id: &str| -> LawToShitsumon {
            let p = dir
                .join("links")
                .join("law-to-shitsumon")
                .join(format!("{id}.json"));
            serde_json::from_slice(&std::fs::read(p).unwrap()).unwrap()
        };
        let nyukan = read("NYUKAN");
        assert_eq!(nyukan.linked_questions.len(), 1);
        assert!(nyukan.linked_questions[0].in_title);
        // 「同法第五条」は件名の入管法に掛かる。
        assert_eq!(nyukan.linked_questions[0].articles, vec!["5"]);
        let sabetsu = read("SABETSU");
        assert_eq!(sabetsu.linked_questions[0].id, "sangiin-219-3");
        assert!(sabetsu.linked_questions[0].articles.is_empty());
        assert!(!sabetsu.linked_questions[0].in_title);
        // 民法は条番号付きの答弁 (sangiin-219-3) だけ。「民法の趣旨」は拾わない。
        let minpo = read("MINPO");
        assert_eq!(minpo.linked_questions.len(), 1);
        assert_eq!(minpo.linked_questions[0].articles, vec!["709"]);
        std::fs::remove_dir_all(&dir).ok();
    }
}
