//! Shared, conservative version candidates for case links and the Wiki source bundle.
use crate::{law_refs::parse_number, ArticleCitation, LawRefResolver};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

pub const HANREI_LAW_NOTICE: &str = "条文への言及を機械抽出し、本文の出来事の日付・改正前の記載、なければ裁判日から法令版の候補を選んでいます。実際の適用法条・適用時点を確定するものではありません。経過措置・遡及適用、引用や比較のための言及は原文で確認してください。過去版が未収録の場合、現行条文で代用しません。";

#[derive(Debug, Serialize, Deserialize)]
pub struct CaseCitation {
    pub field: String,
    #[serde(flatten)]
    pub citation: ArticleCitation,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DateEvidence {
    pub from: String,
    pub to: String,
    pub text: String,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct VersionCandidate {
    pub revision_id: String,
    pub effective_date: String,
    pub body_available: bool,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct CaseLaw {
    pub law_id: String,
    pub title: String,
    pub articles: BTreeSet<String>,
    pub wiki_path: String,
    /// event_date_candidate / before_amendment_candidate / judgment_date_candidate / unresolved
    pub basis: String,
    pub evidence: Vec<DateEvidence>,
    pub versions: Vec<VersionCandidate>,
    /// Browsing aids only; never used as historical evidence by the Wiki planner.
    #[serde(default)]
    pub reference_versions: Vec<VersionCandidate>,
    pub incomplete: bool,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct HanreiLawLinks {
    pub schema_version: u32,
    pub case_id: u64,
    pub notice: String,
    pub wiki_path: String,
    pub citations: Vec<CaseCitation>,
    pub laws: Vec<CaseLaw>,
}

// Japanese dates as printed by the courts, including fullwidth and kanji numbers.
fn number(s: &str) -> Option<(u32, &str)> {
    if let Some(rest) = s.strip_prefix('元') {
        return Some((1, rest));
    }
    let end = s
        .char_indices()
        .find(|(_, c)| {
            !c.is_ascii_digit()
                && !('０'..='９').contains(c)
                && !"〇一二三四五六七八九十百千".contains(*c)
        })
        .map(|(i, _)| i)
        .unwrap_or(s.len());
    let n = &s[..end];
    if n.chars().count() > 8 {
        return None;
    }
    Some((parse_number(n)?, &s[end..]))
}
fn year(s: &str) -> Option<(i32, &str)> {
    for (era, base, last) in [
        ("令和", 2018, 182),
        ("平成", 1988, 31),
        ("昭和", 1925, 64),
        ("大正", 1911, 15),
        ("明治", 1867, 45),
        ("", 0, 2200),
    ] {
        if let Some(rest) = s.strip_prefix(era) {
            if let Some((n, rest)) = number(rest) {
                if let Some(rest) = rest.strip_prefix('年') {
                    let y = base + i32::try_from(n).ok()?;
                    if n > 0 && n <= last && (1868..=2200).contains(&y) {
                        return Some((y, rest));
                    }
                }
            }
        }
    }
    None
}
fn date(s: &str) -> Option<(String, &str)> {
    let (y, rest) = year(s)?;
    let (m, rest) = number(rest)?;
    let (d, rest) = number(rest.strip_prefix('月')?)?;
    let rest = rest.strip_prefix('日')?;
    let parsed = chrono::NaiveDate::from_ymd_opt(y, m, d)?.to_string();
    for (era, first, last) in [
        ("令和", "2019-05-01", "2200-12-31"),
        ("平成", "1989-01-08", "2019-04-30"),
        ("昭和", "1926-12-25", "1989-01-07"),
        ("大正", "1912-07-30", "1926-12-24"),
        ("明治", "1868-01-25", "1912-07-29"),
    ] {
        if s.starts_with(era) && !(first..=last).contains(&parsed.as_str()) {
            return None;
        }
    }
    Some((parsed, rest))
}
fn judgment_date(s: &str) -> Option<String> {
    date(s).map(|(d, _)| d).or_else(|| {
        chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d")
            .ok()
            .map(|d| d.to_string())
    })
}
fn event_dates(text: &str) -> Vec<DateEvidence> {
    let text: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    let mut out = Vec::new();
    for (i, _) in text.char_indices() {
        // Do not parse the suffix of an era date again as a Gregorian year.
        if i > 0
            && text[..i]
                .chars()
                .next_back()
                .is_some_and(|c| c.is_ascii_digit() || ('０'..='９').contains(&c))
        {
            continue;
        }
        let s = &text[i..];
        let parsed = if let Some((d, rest)) = date(s) {
            Some((d.clone(), d, rest))
        } else if let Some((from, rest)) = year(s) {
            rest.strip_prefix("から")
                .and_then(year)
                .and_then(|(to, rest)| {
                    ((from..=from + 30).contains(&to)
                        && (rest.starts_with("にかけて") || rest.starts_with("まで")))
                    .then(|| (format!("{from:04}-01-01"), format!("{to:04}-12-31"), rest))
                })
        } else {
            None
        };
        let Some((from, to, rest)) = parsed else {
            continue;
        };
        // A statute's parenthesized number can contain a full stop before the event.
        let mut depth = 0u32;
        let context: String = rest
            .chars()
            .take(160)
            .take_while(|c| {
                match c {
                    '（' | '(' | '「' | '『' => depth += 1,
                    '）' | ')' | '」' | '』' => depth = depth.saturating_sub(1),
                    '。' | '；' if depth == 0 => return false,
                    _ => {}
                }
                true
            })
            .collect();
        if ![
            "処分",
            "保護変更決定",
            "契約",
            "事故",
            "相続開始",
            "不法行為",
            "本件改定",
        ]
        .iter()
        .any(|w| context.contains(w))
        {
            continue;
        }
        let consumed = s.len() - rest.len();
        let evidence = DateEvidence {
            from,
            to,
            text: format!("{}{}", &s[..consumed], context),
        };
        if !out
            .iter()
            .any(|e: &DateEvidence| e.from == evidence.from && e.to == evidence.to)
        {
            out.push(evidence);
        }
    }
    out
}
fn normalized(s: &str) -> String {
    // Amendment numbers use kanji in one source and Arabic/fullwidth in another.
    let mut out = String::new();
    let mut rest = s;
    while !rest.is_empty() {
        if let Some((n, tail)) = number(rest) {
            out.push_str(&n.to_string());
            rest = tail;
        } else {
            let c = rest.chars().next().unwrap();
            if !c.is_whitespace() {
                out.push(c);
            }
            rest = &rest[c.len_utf8()..];
        }
    }
    out
}

/// No LLM assertions and no fallback to current.json. Both callers use this exact result.
pub fn resolve_hanrei_laws(
    doc: &Value,
    resolver: &LawRefResolver,
    titles: &BTreeMap<String, String>,
    mut get_versions: impl FnMut(&str) -> Result<Option<Value>>,
) -> Result<HanreiLawLinks> {
    let mut citations = Vec::new();
    for field in ["referenced_laws", "holding", "summary", "body_text"] {
        for citation in resolver.citations(doc[field].as_str().unwrap_or("")) {
            citations.push(CaseCitation {
                field: field.into(),
                citation,
            });
        }
    }
    for (i, text) in doc["body_citations"]
        .as_array()
        .into_iter()
        .flatten()
        .enumerate()
    {
        for citation in resolver.citations(text.as_str().unwrap_or("")) {
            citations.push(CaseCitation {
                field: format!("body_citations.{i}"),
                citation,
            });
        }
    }
    let mut references: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for c in &citations {
        references
            .entry(c.citation.law_id.clone())
            .or_default()
            .insert(c.citation.article.clone());
    }
    let body = doc["body_text"].as_str().unwrap_or("");
    let source_text = [
        body,
        doc["holding"].as_str().unwrap_or(""),
        doc["summary"].as_str().unwrap_or(""),
        doc["referenced_laws"].as_str().unwrap_or(""),
    ]
    .join(
        "。
",
    );
    let dates = event_dates(&source_text);
    let mut laws = Vec::new();
    for (law, articles) in references {
        let title = titles.get(&law).cloned().unwrap_or_else(|| law.clone());
        let history = get_versions(&law)?.unwrap_or(Value::Null);
        let mut rows: Vec<&Value> = history["versions"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|v| {
                v["revision_id"]
                    .as_str()
                    .is_some_and(|id| id.starts_with(&format!("{law}_")))
                    && v["effective_date"]
                        .as_str()
                        .is_some_and(|d| chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d").is_ok())
            })
            .collect();
        rows.sort_by_key(|v| {
            (
                v["effective_date"].as_str().unwrap(),
                v["revision_id"].as_str().unwrap(),
            )
        });
        // Only consider an explicit old-law phrase in the sentence containing this law's name.
        let old_context: Vec<_> = source_text
            .split('。')
            .filter(|s| s.contains(&title) && s.contains("改正前"))
            .map(|s| (normalized(s), s.trim().to_string()))
            .collect();
        let mut evidence = dates.clone();
        let mut basis = "event_date_candidate";
        let mut incomplete = false;
        if !old_context.is_empty() {
            basis = "before_amendment_candidate";
            let amendments: BTreeSet<_> = rows
                .iter()
                .filter(|v| {
                    let num = v["amendment_law_num"].as_str().unwrap_or("");
                    !num.is_empty()
                        && old_context
                            .iter()
                            .any(|(s, _)| s.contains(&format!("{}による改正前", normalized(num))))
                })
                .filter_map(|v| v["effective_date"].as_str())
                .collect();
            evidence = amendments
                .iter()
                .filter_map(|d| {
                    let before = chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d")
                        .ok()?
                        .pred_opt()?
                        .to_string();
                    Some(DateEvidence {
                        from: before.clone(),
                        to: before,
                        text: format!(
                            "改正前の記載: {} / 収録改正法の施行日 {d} の前日を候補に使用",
                            old_context
                                .iter()
                                .map(|(_, raw)| raw.as_str())
                                .collect::<Vec<_>>()
                                .join(" / ")
                        ),
                    })
                })
                .collect();
            if evidence.is_empty() {
                incomplete = true;
            }
        } else if evidence.is_empty() {
            basis = "judgment_date_candidate";
            if let Some(d) = judgment_date(doc["judged_on"].as_str().unwrap_or("")) {
                evidence.push(DateEvidence {
                    from: d.clone(),
                    to: d,
                    text: "本文から適用時点を絞れないため裁判日を参照（実際の適用版は未確認）"
                        .into(),
                });
            }
        }
        let mut chosen: BTreeMap<String, VersionCandidate> = BTreeMap::new();
        for e in &evidence {
            let active_date = rows
                .iter()
                .filter_map(|v| v["effective_date"].as_str())
                .filter(|d| *d <= e.from.as_str())
                .max();
            if active_date.is_none() {
                incomplete = true;
            }
            for v in rows.iter().copied().filter(|v| {
                let d = v["effective_date"].as_str().unwrap();
                Some(d) == active_date || (d > e.from.as_str() && d <= e.to.as_str())
            }) {
                let id = v["revision_id"].as_str().unwrap().to_string();
                chosen.insert(
                    id.clone(),
                    VersionCandidate {
                        revision_id: id,
                        effective_date: v["effective_date"].as_str().unwrap().into(),
                        body_available: v["body_available"].as_bool() == Some(true)
                            && v["path"].as_str().is_some(),
                    },
                );
            }
        }
        // Keep out-of-period browsing aids separate from date-selected candidates.
        // In particular, the Wiki planner must not quote these as applicable law.
        let mut reference_versions = Vec::new();
        if chosen.is_empty() {
            let mut seen = BTreeSet::new();
            reference_versions = rows
                .iter()
                .filter_map(|v| {
                    let id = v["revision_id"].as_str().unwrap();
                    if !seen.insert(id) {
                        return None;
                    }
                    Some(VersionCandidate {
                        revision_id: id.into(),
                        effective_date: v["effective_date"].as_str().unwrap().into(),
                        body_available: v["body_available"].as_bool() == Some(true)
                            && v["path"].as_str().is_some(),
                    })
                })
                .take(3)
                .collect();
            incomplete = true;
            basis = "unresolved";
        }
        laws.push(CaseLaw {
            wiki_path: format!("laws/{law}"),
            law_id: law,
            title,
            articles,
            basis: basis.into(),
            evidence,
            versions: chosen.into_values().collect(),
            reference_versions,
            incomplete,
        });
    }
    let id = doc["case_id"].as_u64().unwrap_or(0);
    Ok(HanreiLawLinks {
        schema_version: 1,
        case_id: id,
        notice: HANREI_LAW_NOTICE.into(),
        wiki_path: format!("hanrei/{id}"),
        citations,
        laws,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn resolve(body: &str, holding: &str, versions: Value) -> HanreiLawLinks {
        resolve_hanrei_laws(&json!({"case_id":97208,"judged_on":"令和８年１０月２３日","body_text":body,"holding":holding}), &LawRefResolver::new(&[("L1".into(),"生活保護法".into())]).unwrap(), &BTreeMap::from([("L1".into(), "生活保護法".into())]), |_| Ok(Some(versions.clone()))).unwrap()
    }
    fn history() -> Value {
        json!({"versions":[
            {"revision_id":"L1_20160401_A","effective_date":"2016-04-01","body_available":true,"path":"old.json"},
            {"revision_id":"L1_20200401_B","effective_date":"2020-04-01","body_available":false,"path":null,"amendment_law_num":"平成三十年法律第十号"},
            {"revision_id":"L1_20261001_C","effective_date":"2026-10-01","body_available":true,"path":"new.json"}
        ]})
    }
    #[test]
    fn links_lists_paragraphs_aliases_and_utf16_without_rewriting_text() {
        let text = "📚生活保護法３条、８条２項及び第十条の二。同法第９条";
        let links = resolve("", text, history());
        let refs = &links.citations;
        assert_eq!(refs.len(), 4);
        assert_eq!(refs[0].citation.start, 2);
        assert_eq!(refs[1].citation.text, "８条２項");
        assert_eq!(refs[1].citation.article, "8");
        assert_eq!(refs[1].citation.paragraph, Some(2));
        assert_eq!(refs[2].citation.article, "10_2");
        assert_eq!(refs[3].citation.law_id, "L1");
        for r in refs {
            let utf16: Vec<_> = text.encode_utf16().collect();
            assert_eq!(
                String::from_utf16(&utf16[r.citation.start..r.citation.end]).unwrap(),
                r.citation.text
            );
        }
    }
    #[test]
    fn missing_applicable_body_never_substitutes_an_older_or_current_version() {
        let links = resolve(
            "令和二年四月一日付けの処分について争う。",
            "生活保護法８条２項",
            history(),
        );
        let law = &links.laws[0];
        assert_eq!(law.basis, "event_date_candidate");
        assert_eq!(law.versions.len(), 1);
        assert_eq!(law.versions[0].revision_id, "L1_20200401_B");
        assert!(!law.versions[0].body_available);
        assert!(law.reference_versions.is_empty());
    }
    #[test]
    fn welfare_case_before_coverage_offers_separate_reference_versions() {
        let links = resolve("平成２５年から平成２７年にかけて生活扶助基準を改定したことに伴う保護変更決定について争う。", "生活保護法３条、８条２項", history());
        let law = &links.laws[0];
        assert_eq!(law.evidence[0].from, "2013-01-01");
        assert_eq!(law.evidence[0].to, "2015-12-31");
        assert_eq!(law.basis, "unresolved");
        assert!(law.incomplete);
        assert!(law.versions.is_empty());
        assert_eq!(law.reference_versions.len(), 3);
        assert_eq!(law.reference_versions[0].revision_id, "L1_20160401_A");
        assert!(!law.reference_versions[1].body_available);
    }
    #[test]
    fn reference_versions_are_bounded_sorted_deduplicated_and_validated() {
        let mut versions = history();
        let rows = versions["versions"].as_array_mut().unwrap();
        rows.reverse();
        rows.push(rows[0].clone());
        rows.push(json!({"revision_id":"L1_20300101_D","effective_date":"2030-01-01"}));
        rows.push(json!({"revision_id":"OTHER_20000101","effective_date":"2000-01-01"}));
        rows.push(json!({"revision_id":"L1_INVALID","effective_date":"2000-99-99"}));
        let links = resolve(
            "生活保護法（平成１５年法律第５号による改正前）８条。",
            "",
            versions,
        );
        let law = &links.laws[0];
        assert_eq!(law.basis, "unresolved");
        assert!(law.evidence.is_empty());
        assert!(law.versions.is_empty());
        assert_eq!(
            law.reference_versions
                .iter()
                .map(|v| v.revision_id.as_str())
                .collect::<Vec<_>>(),
            vec!["L1_20160401_A", "L1_20200401_B", "L1_20261001_C"]
        );
    }
    #[test]
    fn welfare_case_finds_event_after_parenthetical_statute_and_ignores_price_statistics() {
        let links = resolve(
            "平成２５年から平成２７年にかけて行われ\nた、厚生労働大臣による「生活保護法による保護の基準」（昭和３８年厚生省告示第１５８号。以下「保護基準」という。）中の生活扶助基準（別表第１）の改定（以下「本件改定」という。）を理由として、保護変更決定を受けた。平成２０年から平成２３年までの下落率が４．７８％であることから、これを生活扶助基準額に反映させるものである。",
            "生活保護法３条、８条２項", history(),
        );
        let evidence = &links.laws[0].evidence;
        assert_eq!(evidence.len(), 1);
        assert_eq!(evidence[0].from, "2013-01-01");
        assert_eq!(evidence[0].to, "2015-12-31");
        assert!(evidence[0].text.contains("本件改定"));
    }
    #[test]
    fn range_retains_all_versions_and_reports_partial_coverage() {
        let links = resolve(
            "平成２７年から令和３年にかけて本件改定による処分がされた。",
            "生活保護法８条",
            history(),
        );
        let law = &links.laws[0];
        assert_eq!(law.versions.len(), 2);
        assert!(law.incomplete);
        assert_eq!(law.basis, "event_date_candidate");
    }
    #[test]
    fn explicit_old_law_precedes_judgment_date_and_unknown_amendment_is_not_guessed() {
        let links = resolve(
            "生活保護法（平成３０年法律第１０号による改正前）８条を参照した。",
            "生活保護法８条",
            history(),
        );
        assert_eq!(links.laws[0].basis, "before_amendment_candidate");
        assert_eq!(links.laws[0].versions[0].revision_id, "L1_20160401_A");
        let links = resolve(
            "生活保護法（平成１５年法律第５号による改正前）８条。",
            "生活保護法８条",
            history(),
        );
        assert_eq!(links.laws[0].basis, "unresolved");
        assert!(links.laws[0].versions.is_empty());
    }
    #[test]
    fn judgment_date_is_only_an_explicitly_labelled_fallback() {
        let links = resolve("適用時点は不明。", "生活保護法８条", history());
        assert_eq!(links.laws[0].basis, "judgment_date_candidate");
        assert_eq!(links.laws[0].versions[0].revision_id, "L1_20261001_C");
        let links = resolve("", "生活保護法８条", json!({"versions":[]}));
        assert_eq!(links.laws[0].basis, "unresolved");
        assert!(links.laws[0].reference_versions.is_empty());
    }
}
