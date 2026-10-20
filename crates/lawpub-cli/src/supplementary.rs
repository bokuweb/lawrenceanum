//! 附則の構造化 (#202) の配信: `laws/{law_id}/supplementary.json` と集計。
//!
//! 抽出そのものは [`law_normalizer::supplementary`]。ここでは build の中で
//! - 現行版の附則を解析して `laws/{law_id}/supplementary.json` を書き、
//! - timeline の改正イベントに、その改正法の附則から読んだ施行期日を `supplementary_enforcement`
//!   として添え (e-Gov の施行日 `effective_date` は書き換えない)、e-Gov に施行日が無い制定イベント
//!   (build が公布日から合成するもの) だけは制定時の附則の施行日を `effective_date` に入れ
//!   (`effective_date_source = "supplementary"`)、
//! - 施行期日を読み取れなかった附則の件数と一覧を `laws/supplementary-summary.json` に、
//!   件数を `health.json` に出す (#196 の未解決件数と同じ出し方)。

use anyhow::Result;
use chrono::NaiveDate;
use egov_client::RevisionMeta;
use law_normalizer::supplementary::{self, SupplementaryDocument, SupplementaryStats};
use law_normalizer::LawDocument;
use serde::Deserialize;
use std::collections::{BTreeMap, HashMap};
use std::path::Path;

pub(crate) const SUMMARY_FILE: &str = "supplementary-summary.json";
/// 未解決一覧に載せる附則の上限 (件数は別に全数を出す)。
const MAX_UNRESOLVED_LISTED: usize = 1000;

/// 改正履歴の公布日 (改正法令番号のキー → 公布日)。
fn amendment_promulgation_dates(meta: &[RevisionMeta]) -> HashMap<String, NaiveDate> {
    meta.iter()
        .filter_map(|m| {
            let key = supplementary::law_num_key(m.amendment_law_num.as_deref()?)?;
            let date =
                NaiveDate::parse_from_str(m.amendment_promulgate_date.as_deref()?, "%Y-%m-%d")
                    .ok()?;
            Some((key, date))
        })
        .collect()
}

/// 現行版の附則を解析する。
pub(crate) fn analyze(doc: &LawDocument, meta: &[RevisionMeta]) -> SupplementaryDocument {
    supplementary::analyze_with_dates(doc, &amendment_promulgation_dates(meta))
}

/// timeline のイベントに `supplementary_enforcement` を添える。
pub(crate) fn attach_to_timeline(events: &mut [serde_json::Value], doc: &SupplementaryDocument) {
    for ev in events {
        let num = ev
            .get("amending_law_num")
            .and_then(|v| v.as_str())
            .map(String::from);
        let is_enactment = ev.get("event_type").and_then(|v| v.as_str()) == Some("enactment");
        if num.is_none() && !is_enactment {
            continue;
        }
        let Some(v) = supplementary::timeline_enforcement(doc, num.as_deref()) else {
            continue;
        };
        let Some(obj) = ev.as_object_mut() else {
            continue;
        };
        // 制定イベントの施行日は e-Gov の改正履歴に無い (null) ので、制定時の附則に施行日が
        // 書かれている (又は公布日から決まる) ときだけ埋める。政令委任など日付の無いものは null のまま。
        let missing = obj.get("effective_date").map_or(true, |d| d.is_null());
        if is_enactment && num.is_none() && missing {
            if let Some(date) = v.get("date").filter(|d| d.is_string()).cloned() {
                obj.insert("effective_date".to_string(), date);
                obj.insert(
                    "effective_date_source".to_string(),
                    serde_json::json!("supplementary"),
                );
            }
        }
        obj.insert("supplementary_enforcement".to_string(), v);
    }
}

#[derive(Deserialize)]
struct LightProvision {
    index: u32,
    amend_law_num: Option<String>,
    label: Option<String>,
    enforcement_unresolved: bool,
}

#[derive(Deserialize)]
struct LightDocument {
    law_id: String,
    stats: SupplementaryStats,
    provisions: Vec<LightProvision>,
}

/// `laws/*/supplementary.json` を集計して `laws/supplementary-summary.json` を書く。
pub(crate) fn write_summary(public: &Path) -> Result<serde_json::Value> {
    let mut total = SupplementaryStats::default();
    let mut laws = 0usize;
    let mut unresolved: Vec<serde_json::Value> = Vec::new();
    let laws_dir = public.join("laws");
    let mut dirs: Vec<_> = std::fs::read_dir(&laws_dir)?
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .map(|e| e.path())
        .collect();
    dirs.sort();
    for dir in dirs {
        let Ok(bytes) = std::fs::read(dir.join("supplementary.json")) else {
            continue;
        };
        let Ok(doc) = serde_json::from_slice::<LightDocument>(&bytes) else {
            continue;
        };
        laws += 1;
        supplementary::add_stats(&mut total, &doc.stats);
        for p in doc.provisions.iter().filter(|p| p.enforcement_unresolved) {
            if unresolved.len() < MAX_UNRESOLVED_LISTED {
                unresolved.push(serde_json::json!({
                    "law_id": doc.law_id,
                    "index": p.index,
                    "amend_law_num": p.amend_law_num,
                    "label": p.label,
                }));
            }
        }
    }
    let rate = |a: usize, b: usize| {
        if b == 0 {
            serde_json::Value::Null
        } else {
            serde_json::json!(((a as f64 / b as f64) * 10000.0).round() / 10000.0)
        }
    };
    let summary = serde_json::json!({
        "schema_version": supplementary::SUPPLEMENTARY_SCHEMA_VERSION,
        "laws": laws,
        "stats": total,
        "enforcement_rate": rate(total.with_enforcement, total.provisions),
        "refs_resolved_rate": rate(total.refs_resolved, total.refs),
        "unresolved": unresolved,
    });
    crate::build::write_json_pretty(&laws_dir.join(SUMMARY_FILE), &summary)?;
    tracing::info!(
        "supplementary: {laws} laws, {} provisions, enforcement unresolved={}",
        total.provisions,
        total.enforcement_unresolved
    );
    Ok(summary)
}

/// health.json に載せる件数 (summary が無ければ null)。
pub(crate) fn health(public: &Path) -> serde_json::Value {
    let Some(v) = std::fs::read(public.join("laws").join(SUMMARY_FILE))
        .ok()
        .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok())
    else {
        return serde_json::Value::Null;
    };
    serde_json::json!({
        "laws": v["laws"],
        "provisions": v["stats"]["provisions"],
        "with_enforcement": v["stats"]["with_enforcement"],
        "enforcement_unresolved": v["stats"]["enforcement_unresolved"],
        "enforcement_rate": v["enforcement_rate"],
        "summary": format!("laws/{SUMMARY_FILE}"),
    })
}

/// 施行予定 (enforcement/upcoming.json) に足す、附則から読んだ今後の施行日。
/// timeline に同じ日の施行が無いものだけ (ただし書・各号による一部施行が主)。
pub(crate) struct UpcomingFromSuppl {
    pub date: String,
    pub amend_law_num: Option<String>,
    pub targets: Option<String>,
}

pub(crate) fn upcoming(law_dir: &Path, today: &str) -> Vec<UpcomingFromSuppl> {
    let Some(doc) = std::fs::read(law_dir.join("supplementary.json"))
        .ok()
        .and_then(|b| serde_json::from_slice::<SupplementaryDocument>(&b).ok())
    else {
        return Vec::new();
    };
    // 同じ日・同じ改正の重複 (各号が同じ日) は 1 件にまとめる。
    let mut seen: BTreeMap<(String, Option<String>), usize> = BTreeMap::new();
    let mut out: Vec<UpcomingFromSuppl> = Vec::new();
    for p in &doc.provisions {
        for e in &p.enforcement {
            let Some(date) = e.date.as_deref().filter(|d| *d > today) else {
                continue;
            };
            let key = (date.to_string(), p.amend_law_num.clone());
            if let Some(&i) = seen.get(&key) {
                if let (Some(t), Some(more)) = (out[i].targets.as_mut(), e.targets.as_deref()) {
                    t.push('、');
                    t.push_str(more);
                }
                continue;
            }
            seen.insert(key, out.len());
            out.push(UpcomingFromSuppl {
                date: date.to_string(),
                amend_law_num: p.amend_law_num.clone(),
                targets: e.targets.clone(),
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc() -> SupplementaryDocument {
        let xml = r#"<?xml version="1.0"?>
<Law Era="Reiwa" Year="6" PromulgateMonth="05" PromulgateDay="24">
  <LawNum>令和六年法律第一号</LawNum>
  <LawBody>
    <LawTitle>テスト法</LawTitle>
    <MainProvision><Article Num="1"><ArticleTitle>第一条</ArticleTitle><Paragraph Num="1"><ParagraphSentence><Sentence>本則。</Sentence></ParagraphSentence></Paragraph></Article></MainProvision>
    <SupplProvision>
      <SupplProvisionLabel>附　則</SupplProvisionLabel>
      <Paragraph Num="1"><ParagraphSentence><Sentence>この法律は、令和六年六月一日から施行する。</Sentence></ParagraphSentence></Paragraph>
    </SupplProvision>
    <SupplProvision AmendLawNum="令和七年三月三一日法律第九号">
      <SupplProvisionLabel>附　則</SupplProvisionLabel>
      <Paragraph Num="1"><ParagraphSentence>
        <Sentence Function="main">この法律は、令和七年四月一日から施行する。</Sentence>
        <Sentence Function="proviso">ただし、第三条の規定は、令和九年四月一日から施行する。</Sentence>
      </ParagraphSentence></Paragraph>
    </SupplProvision>
  </LawBody>
</Law>"#;
        let law = law_normalizer::parse_law_xml(xml.as_bytes(), "506AC0000000001").unwrap();
        analyze(&law, &[])
    }

    #[test]
    fn attaches_enforcement_to_matching_events_only() {
        let d = doc();
        let mut events = vec![
            serde_json::json!({ "event_type": "enactment", "amending_law_num": null, "effective_date": null }),
            serde_json::json!({ "event_type": "amendment", "amending_law_num": "令和七年法律第九号" }),
            serde_json::json!({ "event_type": "amendment", "amending_law_num": "令和七年法律第十号" }),
        ];
        attach_to_timeline(&mut events, &d);
        assert_eq!(events[0]["supplementary_enforcement"]["date"], "2024-06-01");
        // 制定イベントは施行日が null なので制定時の附則の施行日で埋める。改正イベントは触らない。
        assert_eq!(events[0]["effective_date"], "2024-06-01");
        assert_eq!(events[0]["effective_date_source"], "supplementary");
        assert!(events[1].get("effective_date").is_none());
        assert_eq!(events[1]["supplementary_enforcement"]["date"], "2025-04-01");
        assert_eq!(
            events[1]["supplementary_enforcement"]["partial"][0]["date"],
            "2027-04-01"
        );
        assert!(events[2].get("supplementary_enforcement").is_none());
    }

    #[test]
    fn summary_counts_and_upcoming_partial_dates() {
        let root = std::env::temp_dir().join(format!("lawpub_suppl_{}", std::process::id()));
        let law_dir = root.join("laws").join("506AC0000000001");
        std::fs::create_dir_all(&law_dir).unwrap();
        crate::build::write_json_pretty(&law_dir.join("supplementary.json"), &doc()).unwrap();

        let s = write_summary(&root).unwrap();
        assert_eq!(s["laws"], 1);
        assert_eq!(s["stats"]["provisions"], 2);
        assert_eq!(s["stats"]["enforcement_unresolved"], 0);
        assert_eq!(s["enforcement_rate"], 1.0);
        assert_eq!(health(&root)["provisions"], 2);

        let up = upcoming(&law_dir, "2026-10-19");
        assert_eq!(up.len(), 1);
        assert_eq!(up[0].date, "2027-04-01");
        assert_eq!(up[0].targets.as_deref(), Some("第三条の規定"));
        std::fs::remove_dir_all(&root).ok();
    }
}
