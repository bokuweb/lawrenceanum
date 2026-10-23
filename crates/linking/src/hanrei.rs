//! 判例 → 法令 クロスリンク。
//!
//! 判例の参照法条・判示事項・裁判要旨・抽出本文から条文参照と法令版候補を抽出し、
//! `public/links/hanrei-to-laws/{case_id}.json` と逆引き
//! `public/links/law-to-hanrei/{law_id}.json` を書き出す。
//!
//! 民法・刑法のような短い法令名は一般語と衝突しうるため、法令名の直後に条番号が
//! 続く場合だけ採用する (長い法令名でも同じ規則で統一)。LLM は呼ばない。

use crate::{resolve_hanrei_laws, LawRefResolver};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
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

pub fn run_link_hanrei(public: &Path) -> Result<()> {
    let matcher = LawRefResolver::from_index(public)?;
    let index: serde_json::Value =
        serde_json::from_slice(&std::fs::read(public.join("laws/index.json"))?)?;
    let titles: BTreeMap<String, String> = index["laws"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|v| Some((v["law_id"].as_str()?.into(), v["title"].as_str()?.into())))
        .collect();
    let mut histories: HashMap<String, Option<serde_json::Value>> = HashMap::new();
    let forward_dir = public.join("links/hanrei-to-laws");
    std::fs::create_dir_all(&forward_dir)?;

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
        let links = resolve_hanrei_laws(&c, &matcher, &titles, |law| {
            if !histories.contains_key(law) {
                let path = public.join(format!("laws/{law}/versions.json"));
                let value = if path.exists() {
                    Some(serde_json::from_slice(&std::fs::read(path)?)?)
                } else {
                    None
                };
                histories.insert(law.into(), value);
            }
            Ok(histories[law].clone())
        })?;
        std::fs::write(
            forward_dir.join(format!("{}.json", links.case_id)),
            serde_json::to_vec(&links)?,
        )?;
        let per_law: BTreeMap<_, _> = links
            .laws
            .into_iter()
            .map(|l| (l.law_id, l.articles.into_iter().collect()))
            .collect();
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
        let doc = LawToHanrei {
            schema_version: 1,
            law_id: law_id.clone(),
            linked_cases: cases,
        };
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
    fn links_aliases_and_body_citations_end_to_end() {
        let dir = std::env::temp_dir().join(format!("lawpub_hanrei_link_{}", std::process::id()));
        std::fs::create_dir_all(dir.join("laws")).unwrap();
        std::fs::create_dir_all(dir.join("hanrei")).unwrap();
        std::fs::write(
            dir.join("laws").join("index.json"),
            serde_json::to_vec(&serde_json::json!({"laws": [
                {"law_id": "KEISO", "title": "刑事訴訟法"},
                {"law_id": "KENPO", "title": "日本国憲法"},
            ]}))
            .unwrap(),
        )
        .unwrap();
        // 参照法条は空。PDF 本文由来の引用に略称 (刑訴法) と通称 (憲法) が入っている。
        std::fs::write(
            dir.join("hanrei").join("1.json"),
            serde_json::to_vec(&serde_json::json!({
                "case_id": 1, "case_number": "令和7(あ)1", "case_name": "X事件",
                "judged_on": "令和8年1月1日", "court": "最高裁判所第一小法廷",
                "holding": "", "summary": "", "referenced_laws": "",
                "body_citations": ["いずれも刑訴法４０５条", "憲法３１条"]
            }))
            .unwrap(),
        )
        .unwrap();

        run_link_hanrei(&dir).unwrap();

        let read = |id: &str| -> LawToHanrei {
            let p = dir
                .join("links")
                .join("law-to-hanrei")
                .join(format!("{id}.json"));
            serde_json::from_slice(&std::fs::read(p).unwrap()).unwrap()
        };
        assert_eq!(read("KEISO").linked_cases[0].articles, vec!["405"]);
        assert_eq!(read("KENPO").linked_cases[0].articles, vec!["31"]);
        std::fs::remove_dir_all(&dir).ok();
    }
}
