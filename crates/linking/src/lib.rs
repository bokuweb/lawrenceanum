//! 法令 ↔ 国会会議録・パブコメ・調達情報 クロスリンク生成。
//!
//! ## アルゴリズム
//!
//! 1. `public/laws/index.json` から全法令の (law_id, title, amending_law_num リスト) を読む。
//! 2. aho-corasick で改正法番号・法令タイトルの辞書を構築する。
//! 3. `public/proceedings/*.json` の各 speech.speech テキストにマッチを走らせる。
//! 4. マッチが 1 件以上ある会議を `LinkedProceeding` として収集し
//!    `public/links/law-to-proceedings/{law_id}.json` に書き出す。
//!
//! ## 信頼度
//!
//! - `amending_law_num` マッチ → `relevance = "amendment_debate"`, confidence 高
//! - タイトルのみマッチ  → `relevance = "reference_only"`, confidence 低
//!
//! LLM は呼ばない。抽出は aho-corasick のみ。

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

// ── 公開型 ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinkedLaw {
    pub law_id: String,
    pub title: String,
    pub relevance: String,
    pub confidence: f32,
    pub match_reasons: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeetingToLaws {
    pub schema_version: u32,
    pub meeting_id: String,
    pub linked_laws: Vec<LinkedLaw>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinkedProceeding {
    pub meeting_id: String,
    pub date: String,
    pub house: String,
    pub committee: Option<String>,
    pub relevance: String,
    pub speech_count_mentioning: usize,
    pub confidence: f32,
    pub match_reasons: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LawToProceedings {
    pub schema_version: u32,
    pub law_id: String,
    pub linked_proceedings: Vec<LinkedProceeding>,
}

// ── 内部型 ────────────────────────────────────────────────────────

struct LawEntry {
    law_id: String,
    title: String,
    amending_law_nums: Vec<String>,
}

// aho-corasick パターンに対するメタ情報
#[derive(Clone)]
struct PatternMeta {
    law_id: String,
    pattern_text: String,
    is_law_num: bool, // true = 改正法番号マッチ、false = タイトルマッチ
}

// ── 公開エントリポイント ──────────────────────────────────────────

/// `public/` ディレクトリを受け取り、`links/law-to-proceedings/` を生成する。
pub fn run_link(public: &Path) -> Result<()> {
    let laws = load_law_entries(public)?;
    tracing::info!("linking: {} laws loaded", laws.len());

    let (ac, metas) = build_automaton(&laws)?;
    tracing::info!("linking: {} patterns in automaton", metas.len());

    let proceedings_dir = public.join("proceedings");
    if !proceedings_dir.exists() {
        tracing::warn!("no proceedings dir at {}; skipping link generation", proceedings_dir.display());
        return Ok(());
    }

    // law_id → Vec<LinkedProceeding>
    let mut result: HashMap<String, Vec<LinkedProceeding>> = HashMap::new();

    for entry in walkdir::WalkDir::new(&proceedings_dir)
        .min_depth(1)
        .max_depth(1)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().and_then(|x| x.to_str()) == Some("json"))
        .filter(|e| e.file_name().to_str() != Some("index.json"))
    {
        let path = entry.path();
        let bytes = std::fs::read(path)
            .with_context(|| format!("read {}", path.display()))?;
        let meeting: serde_json::Value = serde_json::from_slice(&bytes)?;

        let meeting_id = meeting["meeting_id"].as_str().unwrap_or("").to_string();
        let date = meeting["date"].as_str().unwrap_or("").to_string();
        let house = meeting["house"].as_str().unwrap_or("").to_string();
        let committee = meeting["committee"].as_str().map(String::from);

        // 全発言テキストを連結してマッチング
        let full_text: String = meeting["speeches"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|s| s["speech"].as_str())
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .unwrap_or_default();

        // law_id → (マッチ理由セット, 言及発言数カウント)
        let mut law_hits: HashMap<String, (std::collections::HashSet<String>, bool)> =
            HashMap::new();

        for mat in ac.find_overlapping_iter(&full_text) {
            let meta = &metas[mat.pattern().as_usize()];
            let entry = law_hits.entry(meta.law_id.clone()).or_default();
            entry.0.insert(meta.pattern_text.clone());
            if meta.is_law_num {
                entry.1 = true; // 改正法番号マッチあり
            }
        }

        // 発言単位のカウント（同一 law_id に言及した speech の数）
        for (law_id, (reasons, has_law_num)) in &law_hits {
            let speech_count = meeting["speeches"]
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .filter(|s| {
                            let text = s["speech"].as_str().unwrap_or("");
                            reasons.iter().any(|r| text.contains(r.as_str()))
                        })
                        .count()
                })
                .unwrap_or(0);

            let (relevance, confidence) = if *has_law_num {
                ("amendment_debate".to_string(), 0.92f32)
            } else {
                ("reference_only".to_string(), 0.55f32)
            };

            result.entry(law_id.clone()).or_default().push(LinkedProceeding {
                meeting_id: meeting_id.clone(),
                date: date.clone(),
                house: house.clone(),
                committee: committee.clone(),
                relevance,
                speech_count_mentioning: speech_count,
                confidence,
                match_reasons: reasons.iter().cloned().collect(),
            });
        }
    }

    // 書き出し: law-to-proceedings
    let links_dir = public.join("links").join("law-to-proceedings");
    std::fs::create_dir_all(&links_dir)?;

    // 書き出し: meeting-to-laws (逆引き)
    let m2l_dir = public.join("links").join("meeting-to-laws");
    std::fs::create_dir_all(&m2l_dir)?;

    // 法令タイトルの逆引き用マップ
    let law_title_map: HashMap<String, String> = laws
        .iter()
        .map(|l| (l.law_id.clone(), l.title.clone()))
        .collect();

    // meeting_id → Vec<LinkedLaw>
    let mut m2l: HashMap<String, Vec<LinkedLaw>> = HashMap::new();

    let mut written = 0usize;
    for (law_id, mut proceedings) in result {
        // law-to-proceedings: 日付降順
        proceedings.sort_by(|a, b| b.date.cmp(&a.date));

        // meeting-to-laws に逆登録
        for p in &proceedings {
            m2l.entry(p.meeting_id.clone()).or_default().push(LinkedLaw {
                law_id: law_id.clone(),
                title: law_title_map.get(&law_id).cloned().unwrap_or_default(),
                relevance: p.relevance.clone(),
                confidence: p.confidence,
                match_reasons: p.match_reasons.clone(),
            });
        }

        let output = LawToProceedings {
            schema_version: 1,
            law_id: law_id.clone(),
            linked_proceedings: proceedings,
        };
        let path = links_dir.join(format!("{law_id}.json"));
        std::fs::write(&path, serde_json::to_string_pretty(&output)?)
            .with_context(|| format!("write {}", path.display()))?;
        written += 1;
    }
    tracing::info!("linking: {written} law-to-proceedings files written");

    let mut m2l_written = 0usize;
    for (meeting_id, mut linked_laws) in m2l {
        // confidence 降順
        linked_laws.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap_or(std::cmp::Ordering::Equal));
        let output = MeetingToLaws {
            schema_version: 1,
            meeting_id: meeting_id.clone(),
            linked_laws,
        };
        let path = m2l_dir.join(format!("{meeting_id}.json"));
        std::fs::write(&path, serde_json::to_string_pretty(&output)?)
            .with_context(|| format!("write {}", path.display()))?;
        m2l_written += 1;
    }
    tracing::info!("linking: {m2l_written} meeting-to-laws files written");
    Ok(())
}

// ── 内部関数 ──────────────────────────────────────────────────────

fn load_law_entries(public: &Path) -> Result<Vec<LawEntry>> {
    let index_path = public.join("laws").join("index.json");
    let bytes = std::fs::read(&index_path)
        .with_context(|| format!("read {}", index_path.display()))?;
    let v: serde_json::Value = serde_json::from_slice(&bytes)?;

    let mut entries = Vec::new();
    for law in v["laws"].as_array().unwrap_or(&vec![]) {
        let law_id = law["law_id"].as_str().unwrap_or("").to_string();
        let title = law["title"].as_str().unwrap_or("").to_string();
        if law_id.is_empty() || title.is_empty() {
            continue;
        }

        // timeline から改正法番号を収集する
        let timeline_path = public.join(&law["timeline"].as_str().unwrap_or(""));
        let amending_law_nums = if timeline_path.exists() {
            load_amending_law_nums(&timeline_path).unwrap_or_default()
        } else {
            vec![]
        };

        entries.push(LawEntry { law_id, title, amending_law_nums });
    }
    Ok(entries)
}

fn load_amending_law_nums(timeline_path: &Path) -> Result<Vec<String>> {
    let bytes = std::fs::read(timeline_path)?;
    let v: serde_json::Value = serde_json::from_slice(&bytes)?;
    let nums = v["events"]
        .as_array()
        .unwrap_or(&vec![])
        .iter()
        .filter_map(|e| e["amending_law_num"].as_str().map(String::from))
        .filter(|s| !s.is_empty())
        .collect();
    Ok(nums)
}

fn build_automaton(
    laws: &[LawEntry],
) -> Result<(aho_corasick::AhoCorasick, Vec<PatternMeta>)> {
    let mut patterns: Vec<String> = Vec::new();
    let mut metas: Vec<PatternMeta> = Vec::new();

    for law in laws {
        // タイトルが短すぎると誤マッチが多い（「法」など）
        if law.title.chars().count() >= 4 {
            patterns.push(law.title.clone());
            metas.push(PatternMeta {
                law_id: law.law_id.clone(),
                pattern_text: law.title.clone(),
                is_law_num: false,
            });
        }
        for num in &law.amending_law_nums {
            if num.chars().count() >= 6 {
                patterns.push(num.clone());
                metas.push(PatternMeta {
                    law_id: law.law_id.clone(),
                    pattern_text: num.clone(),
                    is_law_num: true,
                });
            }
        }
    }

    let ac = aho_corasick::AhoCorasick::new(&patterns)
        .context("build aho-corasick automaton")?;
    Ok((ac, metas))
}

// ── パブコメ クロスリンク ─────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinkedPubcomment {
    pub case_id: String,
    pub title: String,
    pub ministry: String,
    pub start_date: String,
    pub end_date: String,
    pub relevance: String,
    pub confidence: f32,
    pub match_reasons: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LawToPubcomments {
    pub schema_version: u32,
    pub law_id: String,
    pub linked_pubcomments: Vec<LinkedPubcomment>,
}

/// `public/` を受け取り、`links/law-to-pubcomment/` を生成する。
pub fn run_link_pubcomment(public: &Path) -> Result<()> {
    let laws = load_law_entries(public)?;
    tracing::info!("link-pubcomment: {} laws loaded", laws.len());

    let (ac, metas) = build_automaton(&laws)?;

    // 関連法令名の完全一致用。automaton は誤マッチ防止で 4 文字未満の短いタイトルを
    // 除外するため、民法・刑法など重要な短 title 法令を取りこぼす。related_law_name は
    // 正確な法令名なので、完全一致なら短 title でも安全にリンクできる。
    let title_to_law: HashMap<&str, &str> =
        laws.iter().map(|l| (l.title.as_str(), l.law_id.as_str())).collect();

    let pubcomment_dir = public.join("pubcomment");
    if !pubcomment_dir.exists() {
        tracing::warn!("no pubcomment dir; skipping");
        return Ok(());
    }

    let mut result: HashMap<String, Vec<LinkedPubcomment>> = HashMap::new();

    for entry in walkdir::WalkDir::new(&pubcomment_dir)
        .min_depth(1)
        .max_depth(1)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().and_then(|x| x.to_str()) == Some("json"))
        .filter(|e| e.file_name().to_str() != Some("index.json"))
    {
        let path = entry.path();
        let bytes = std::fs::read(path)
            .with_context(|| format!("read {}", path.display()))?;
        let doc: serde_json::Value = serde_json::from_slice(&bytes)?;

        let case_id = doc["case_id"].as_str().unwrap_or("").to_string();
        let title = doc["title"].as_str().unwrap_or("").to_string();
        let ministry = doc["ministry"].as_str().unwrap_or("").to_string();
        // パブコメ JSON のフィールドは reception_start / reception_end
        // (旧スキーマの start_date / end_date は存在しない)。
        let start_date = doc["reception_start"].as_str().unwrap_or("").to_string();
        let end_date = doc["reception_end"].as_str().unwrap_or("").to_string();

        // 「関連法令名」での一致は改正パブコメの強いシグナル。タイトル・意見本文での
        // 一致は言及扱い。意見と府省の考え方 (opinions) も検索対象に含める。
        let related_law_name = doc["related_law_name"].as_str().unwrap_or("");
        let opinions_text: String = doc["opinions"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .map(|o| {
                        format!(
                            "{} {} {}",
                            o["item"].as_str().unwrap_or(""),
                            o["opinion"].as_str().unwrap_or(""),
                            o["ministry_response"].as_str().unwrap_or("")
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .unwrap_or_default();
        let weak_text = format!("{title}\n{opinions_text}");

        // (reasons, has_law_num, matched_in_related_law_name)
        let mut law_hits: HashMap<String, (std::collections::HashSet<String>, bool, bool)> =
            HashMap::new();
        for mat in ac.find_overlapping_iter(related_law_name) {
            let meta = &metas[mat.pattern().as_usize()];
            let entry = law_hits.entry(meta.law_id.clone()).or_default();
            entry.0.insert(meta.pattern_text.clone());
            entry.2 = true;
            if meta.is_law_num {
                entry.1 = true;
            }
        }
        // related_law_name の完全一致 (automaton が落とす短 title 法令の救済)。
        let rln_trim = related_law_name.trim();
        if !rln_trim.is_empty() {
            if let Some(&law_id) = title_to_law.get(rln_trim) {
                let entry = law_hits.entry(law_id.to_string()).or_default();
                entry.0.insert(rln_trim.to_string());
                entry.2 = true;
            }
        }
        for mat in ac.find_overlapping_iter(&weak_text) {
            let meta = &metas[mat.pattern().as_usize()];
            let entry = law_hits.entry(meta.law_id.clone()).or_default();
            entry.0.insert(meta.pattern_text.clone());
            if meta.is_law_num {
                entry.1 = true;
            }
        }

        for (law_id, (reasons, has_law_num, in_related)) in &law_hits {
            let (relevance, confidence) = if *has_law_num || *in_related {
                ("amendment_comment".to_string(), 0.90f32)
            } else {
                ("reference_only".to_string(), 0.55f32)
            };
            result.entry(law_id.clone()).or_default().push(LinkedPubcomment {
                case_id: case_id.clone(),
                title: title.clone(),
                ministry: ministry.clone(),
                start_date: start_date.clone(),
                end_date: end_date.clone(),
                relevance,
                confidence,
                match_reasons: reasons.iter().cloned().collect(),
            });
        }
    }

    let links_dir = public.join("links").join("law-to-pubcomment");
    std::fs::create_dir_all(&links_dir)?;

    let mut written = 0usize;
    for (law_id, mut items) in result {
        items.sort_by(|a, b| b.end_date.cmp(&a.end_date));
        let output = LawToPubcomments {
            schema_version: 1,
            law_id: law_id.clone(),
            linked_pubcomments: items,
        };
        let path = links_dir.join(format!("{law_id}.json"));
        std::fs::write(&path, serde_json::to_string_pretty(&output)?)
            .with_context(|| format!("write {}", path.display()))?;
        written += 1;
    }
    tracing::info!("link-pubcomment: {written} files written");
    Ok(())
}

// ── 調達情報 クロスリンク ─────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinkedProcurement {
    pub item_id: String,
    pub title: String,
    pub organization: String,
    pub notice_date: String,
    pub relevance: String,
    pub confidence: f32,
    pub match_reasons: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LawToProcurements {
    pub schema_version: u32,
    pub law_id: String,
    pub linked_procurements: Vec<LinkedProcurement>,
}

/// `public/` を受け取り、`links/law-to-procurement/` を生成する。
pub fn run_link_procurement(public: &Path) -> Result<()> {
    let laws = load_law_entries(public)?;
    tracing::info!("link-procurement: {} laws loaded", laws.len());

    let (ac, metas) = build_automaton(&laws)?;

    let procurement_dir = public.join("procurement");
    if !procurement_dir.exists() {
        tracing::warn!("no procurement dir; skipping");
        return Ok(());
    }

    let mut result: HashMap<String, Vec<LinkedProcurement>> = HashMap::new();

    for entry in walkdir::WalkDir::new(&procurement_dir)
        .min_depth(1)
        .max_depth(1)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().and_then(|x| x.to_str()) == Some("json"))
        .filter(|e| e.file_name().to_str() != Some("index.json"))
    {
        let path = entry.path();
        let bytes = std::fs::read(path)
            .with_context(|| format!("read {}", path.display()))?;
        let doc: serde_json::Value = serde_json::from_slice(&bytes)?;

        let item_id = doc["item_id"].as_str().unwrap_or("").to_string();
        let title = doc["title"].as_str().unwrap_or("").to_string();
        let organization = doc["organization"].as_str().unwrap_or("").to_string();
        let notice_date = doc["notice_date"].as_str().unwrap_or("").to_string();

        let search_text = format!("{title}\n{organization}");

        let mut law_hits: HashMap<String, (std::collections::HashSet<String>, bool)> =
            HashMap::new();
        for mat in ac.find_overlapping_iter(&search_text) {
            let meta = &metas[mat.pattern().as_usize()];
            let entry = law_hits.entry(meta.law_id.clone()).or_default();
            entry.0.insert(meta.pattern_text.clone());
            if meta.is_law_num {
                entry.1 = true;
            }
        }

        for (law_id, (reasons, has_law_num)) in &law_hits {
            let (relevance, confidence) = if *has_law_num {
                ("law_cited".to_string(), 0.88f32)
            } else {
                ("reference_only".to_string(), 0.50f32)
            };
            result.entry(law_id.clone()).or_default().push(LinkedProcurement {
                item_id: item_id.clone(),
                title: title.clone(),
                organization: organization.clone(),
                notice_date: notice_date.clone(),
                relevance,
                confidence,
                match_reasons: reasons.iter().cloned().collect(),
            });
        }
    }

    let links_dir = public.join("links").join("law-to-procurement");
    std::fs::create_dir_all(&links_dir)?;

    let mut written = 0usize;
    for (law_id, mut items) in result {
        items.sort_by(|a, b| b.notice_date.cmp(&a.notice_date));
        let output = LawToProcurements {
            schema_version: 1,
            law_id: law_id.clone(),
            linked_procurements: items,
        };
        let path = links_dir.join(format!("{law_id}.json"));
        std::fs::write(&path, serde_json::to_string_pretty(&output)?)
            .with_context(|| format!("write {}", path.display()))?;
        written += 1;
    }
    tracing::info!("link-procurement: {written} files written");
    Ok(())
}

// ── テスト ────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_law(id: &str, title: &str, nums: &[&str]) -> LawEntry {
        LawEntry {
            law_id: id.to_string(),
            title: title.to_string(),
            amending_law_nums: nums.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn automaton_matches_law_num() {
        let laws = vec![make_law(
            "129AC0000000089",
            "民法",
            &["令和四年法律第百二号"],
        )];
        let (ac, metas) = build_automaton(&laws).unwrap();
        let text = "令和四年法律第百二号に基づく改正について審議します。";
        let hits: Vec<_> = ac.find_overlapping_iter(text).collect();
        assert!(!hits.is_empty());
        assert!(metas[hits[0].pattern().as_usize()].is_law_num);
        assert_eq!(metas[hits[0].pattern().as_usize()].law_id, "129AC0000000089");
    }

    #[test]
    fn automaton_matches_title() {
        let laws = vec![make_law("140AC0000000045", "刑事訴訟法", &[])];
        let (ac, metas) = build_automaton(&laws).unwrap();
        let text = "刑事訴訟法の改正に関する質問をします。";
        let hits: Vec<_> = ac.find_overlapping_iter(text).collect();
        assert!(!hits.is_empty());
        assert!(!metas[hits[0].pattern().as_usize()].is_law_num);
    }

    #[test]
    fn short_title_is_excluded() {
        let laws = vec![make_law("X", "法", &[])];
        let (ac, _metas) = build_automaton(&laws).unwrap();
        let hits: Vec<_> = ac.find_overlapping_iter("法について").collect();
        assert!(hits.is_empty());
    }
}
