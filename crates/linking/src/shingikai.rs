use super::{build_automaton, load_law_entries, LawEntry, LinkedLaw};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinkedShingikai {
    pub minutes_id: String,
    pub title: String,
    pub ministry: String,
    pub committee: String,
    pub date: String,
    pub relevance: String,
    pub confidence: f32,
    pub match_reasons: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LawToShingikai {
    pub schema_version: u32,
    pub law_id: String,
    pub linked_meetings: Vec<LinkedShingikai>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShingikaiToLaws {
    pub schema_version: u32,
    pub minutes_id: String,
    pub linked_laws: Vec<LinkedLaw>,
}

/// 議題・概要・議事録・配布資料全文を法令辞書と照合し、双方向リンクを生成する。
pub fn run_link_shingikai(public: &Path) -> Result<()> {
    let laws = load_law_entries(public)?;
    tracing::info!("link-shingikai: {} laws loaded", laws.len());
    let (automaton, metas) = build_automaton(&laws)?;

    let shingikai_dir = public.join("shingikai");
    if !shingikai_dir.exists() {
        tracing::warn!("no shingikai dir at {}; skipping", shingikai_dir.display());
        return Ok(());
    }

    // 通常の辞書は誤検出防止で4文字未満を除く。民法・刑法・商法・会社法を落とさないよう、
    // 「法」で終わる2〜3文字の正式名称だけを低めの confidence で補完する。
    let short_laws: Vec<&LawEntry> = laws
        .iter()
        .filter(|law| {
            let len = law.title.chars().count();
            (2..=3).contains(&len) && law.title.ends_with('法')
        })
        .collect();

    let mut result: HashMap<String, Vec<LinkedShingikai>> = HashMap::new();
    for entry in walkdir::WalkDir::new(&shingikai_dir)
        .min_depth(2)
        .into_iter()
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_type().is_file())
        .filter(|entry| entry.path().extension().and_then(|value| value.to_str()) == Some("json"))
        .filter(|entry| entry.file_name().to_str() != Some("index.json"))
    {
        let path = entry.path();
        let doc: serde_json::Value = serde_json::from_slice(
            &std::fs::read(path).with_context(|| format!("read {}", path.display()))?,
        )?;
        let minutes_id = doc["minutes_id"].as_str().unwrap_or("").to_string();
        if minutes_id.is_empty() {
            continue;
        }

        let title = doc["title"].as_str().unwrap_or("").to_string();
        let ministry = doc["ministry"].as_str().unwrap_or("").to_string();
        let committee = doc["committee"].as_str().unwrap_or("").to_string();
        let date = doc["date"].as_str().unwrap_or("").to_string();
        let mut text_parts = vec![
            title.as_str(),
            doc["agenda"].as_str().unwrap_or(""),
            doc["summary"].as_str().unwrap_or(""),
            doc["body_text"].as_str().unwrap_or(""),
            doc["minutes_text"].as_str().unwrap_or(""),
        ];
        // minutes_text は議事録添付の抽出結果を複製済みなので、配布資料だけを追加する。
        if let Some(attachments) = doc["attachments"].as_array() {
            text_parts.extend(
                attachments
                    .iter()
                    .filter(|attachment| attachment["kind"].as_str() == Some("material"))
                    .filter_map(|attachment| attachment["extracted_text"].as_str()),
            );
        }
        let search_text = text_parts.join("\n");

        // law_id → (根拠, 法令番号一致, 短い正式名称での補完一致)
        let mut hits: HashMap<String, (HashSet<String>, bool, bool)> = HashMap::new();
        for matched in automaton.find_overlapping_iter(&search_text) {
            let meta = &metas[matched.pattern().as_usize()];
            let hit = hits.entry(meta.law_id.clone()).or_default();
            hit.0.insert(meta.pattern_text.clone());
            if meta.is_law_num {
                hit.1 = true;
            }
        }
        for law in &short_laws {
            if search_text.contains(&law.title) {
                let hit = hits.entry(law.law_id.clone()).or_default();
                hit.0.insert(law.title.clone());
                hit.2 = true;
            }
        }

        for (law_id, (reasons, has_law_num, short_title_match)) in hits {
            let (relevance, confidence) = if has_law_num {
                ("law_cited", 0.92)
            } else if short_title_match {
                ("title_reference", 0.60)
            } else {
                ("deliberation_reference", 0.72)
            };
            let mut match_reasons: Vec<String> = reasons.into_iter().collect();
            match_reasons.sort();
            result.entry(law_id).or_default().push(LinkedShingikai {
                minutes_id: minutes_id.clone(),
                title: title.clone(),
                ministry: ministry.clone(),
                committee: committee.clone(),
                date: date.clone(),
                relevance: relevance.to_string(),
                confidence,
                match_reasons,
            });
        }
    }

    write_links(public, &laws, result)
}

fn write_links(
    public: &Path,
    laws: &[LawEntry],
    result: HashMap<String, Vec<LinkedShingikai>>,
) -> Result<()> {
    let law_links_dir = public.join("links").join("law-to-shingikai");
    let meeting_links_dir = public.join("links").join("shingikai-to-laws");
    std::fs::create_dir_all(&law_links_dir)?;
    std::fs::create_dir_all(&meeting_links_dir)?;
    let law_titles: HashMap<&str, &str> = laws
        .iter()
        .map(|law| (law.law_id.as_str(), law.title.as_str()))
        .collect();
    let mut reverse: HashMap<String, Vec<LinkedLaw>> = HashMap::new();

    let mut law_files = 0usize;
    for (law_id, mut meetings) in result {
        meetings.sort_by(|a, b| {
            b.date
                .cmp(&a.date)
                .then_with(|| a.minutes_id.cmp(&b.minutes_id))
        });
        for meeting in &meetings {
            reverse
                .entry(meeting.minutes_id.clone())
                .or_default()
                .push(LinkedLaw {
                    law_id: law_id.clone(),
                    title: law_titles
                        .get(law_id.as_str())
                        .copied()
                        .unwrap_or("")
                        .to_string(),
                    relevance: meeting.relevance.clone(),
                    confidence: meeting.confidence,
                    match_reasons: meeting.match_reasons.clone(),
                });
        }
        let output = LawToShingikai {
            schema_version: 1,
            law_id: law_id.clone(),
            linked_meetings: meetings,
        };
        let path = law_links_dir.join(format!("{law_id}.json"));
        std::fs::write(&path, serde_json::to_string_pretty(&output)?)
            .with_context(|| format!("write {}", path.display()))?;
        law_files += 1;
    }

    let mut meeting_files = 0usize;
    for (minutes_id, mut linked_laws) in reverse {
        linked_laws.sort_by(|a, b| {
            b.confidence
                .partial_cmp(&a.confidence)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.law_id.cmp(&b.law_id))
        });
        let output = ShingikaiToLaws {
            schema_version: 1,
            minutes_id: minutes_id.clone(),
            linked_laws,
        };
        let path = meeting_links_dir.join(format!("{minutes_id}.json"));
        std::fs::write(&path, serde_json::to_string_pretty(&output)?)
            .with_context(|| format!("write {}", path.display()))?;
        meeting_files += 1;
    }
    tracing::info!("link-shingikai: {law_files} law links / {meeting_files} meeting links written");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn links_minutes_and_materials_in_both_directions() {
        let dir =
            std::env::temp_dir().join(format!("lawpub_shingikai_link_{}", std::process::id()));
        let laws_dir = dir.join("laws");
        let shingikai_dir = dir.join("shingikai").join("moj");
        std::fs::create_dir_all(&laws_dir).unwrap();
        std::fs::create_dir_all(&shingikai_dir).unwrap();
        std::fs::write(
            laws_dir.join("index.json"),
            serde_json::to_vec(&serde_json::json!({
                "laws": [
                    {"law_id": "417AC0000000086", "title": "会社法", "timeline": "missing-1.json"},
                    {"law_id": "415AC0000000057", "title": "個人情報の保護に関する法律", "timeline": "missing-2.json"}
                ]
            }))
            .unwrap(),
        )
        .unwrap();
        std::fs::write(
            shingikai_dir.join("meeting-1.json"),
            serde_json::to_vec(&serde_json::json!({
                "minutes_id": "meeting-1",
                "ministry": "moj",
                "committee": "法制審議会会社法制部会",
                "date": "2026-08-01",
                "title": "第1回会議",
                "agenda": "会社法制の見直し",
                "summary": "",
                "body_text": "",
                "minutes_text": "会社法の改正について審議した。",
                "attachments": [{
                    "kind": "material",
                    "extracted_text": "個人情報の保護に関する法律との関係を整理する。"
                }]
            }))
            .unwrap(),
        )
        .unwrap();

        run_link_shingikai(&dir).unwrap();

        let companies: LawToShingikai = serde_json::from_slice(
            &std::fs::read(dir.join("links/law-to-shingikai/417AC0000000086.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(companies.linked_meetings.len(), 1);
        assert_eq!(companies.linked_meetings[0].relevance, "title_reference");

        let reverse: ShingikaiToLaws = serde_json::from_slice(
            &std::fs::read(dir.join("links/shingikai-to-laws/meeting-1.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(reverse.linked_laws.len(), 2);
        assert!(reverse
            .linked_laws
            .iter()
            .any(|law| law.law_id == "415AC0000000057"));
        std::fs::remove_dir_all(&dir).ok();
    }
}
