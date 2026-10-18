//! 制定経緯の結合: パブコメ → 結果公示 → 公布 → 施行 を法令ごとの 1 本の連鎖にする。
//!
//! 入力 (すべて `public/` 配下。新規取得はしない):
//! - `links/law-to-pubcomment/{law_id}.json` (改正に関係するパブコメ。`link-laws-and-pubcomment` の出力)
//! - `pubcomment/{case_id}.json` (結果公示日の取得用)
//! - `laws/{law_id}/timeline.json` の `events` (公布日・施行日・官報リンク)
//!
//! 出力: `links/law-to-rulemaking/{law_id}.json`。突合できなかったパブコメは単独の連鎖
//! (`promulgation = null`) として残し、`unresolved` の件数を `links/law-to-rulemaking/_summary.json`
//! に出す。
//!
//! ## 突合規則
//!
//! 1. **題名一致 (confidence 0.9)**: 改正イベントの改正法の題名・法令番号がパブコメ題名に含まれる。
//! 2. **日付近接 (confidence 0.5)**: 題名では決まらないとき、結果公示日の前 [`WINDOW_BEFORE_DAYS`] 日
//!    〜後 [`WINDOW_AFTER_DAYS`] 日に公布された改正が **ちょうど 1 件** なら結ぶ。複数あれば
//!    曖昧なので結ばない (誤結合より未解決を選ぶ)。
//!
//! 1 つの改正イベントに複数のパブコメが結ばれることがある (政省令と法律の同時改正など)。
//! LLM は呼ばない。

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

/// 結果公示日より何日前までの公布を同じ改正とみなすか (結果公示が公布の後になる政省令向け)。
const WINDOW_BEFORE_DAYS: i64 = 60;
/// 結果公示日より何日後までの公布を同じ改正とみなすか (法案提出前に意見募集する法律向け)。
const WINDOW_AFTER_DAYS: i64 = 365;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Step {
    /// "pubcomment_start" | "pubcomment_end" | "pubcomment_result" | "promulgation" | "effective"
    pub kind: String,
    pub date: String,
    pub label: String,
    /// 参照先 (パブコメ案件番号 / 改正イベント ID)。
    pub ref_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Chain {
    /// 連鎖の識別子 (改正イベント ID、なければ `pubcomment:{case_id}`)。
    pub chain_id: String,
    pub confidence: f32,
    pub match_reasons: Vec<String>,
    /// 日付昇順。
    pub steps: Vec<Step>,
    pub pubcomment_case_ids: Vec<String>,
    /// 改正イベント (公布) が見つかっているか。
    pub resolved: bool,
    /// 官報 PDF (改正イベントに官報リンクがあれば)。
    pub kanpo_pdf_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LawToRulemaking {
    pub schema_version: u32,
    pub law_id: String,
    pub chains: Vec<Chain>,
}

#[derive(Debug, Clone)]
struct Event {
    id: String,
    title: String,
    num: String,
    promulgation: String,
    effective: Option<String>,
    kanpo_pdf_url: Option<String>,
}

#[derive(Debug, Clone)]
struct Case {
    id: String,
    title: String,
    start: String,
    end: String,
    result: Option<String>,
}

/// `YYYY-MM-DD` → 1970-01-01 からの通日。不正な形式は None。
fn days(date: &str) -> Option<i64> {
    let mut it = date.get(..10)?.split('-');
    let (y, m, d): (i64, i64, i64) = (
        it.next()?.parse().ok()?,
        it.next()?.parse().ok()?,
        it.next()?.parse().ok()?,
    );
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    // Howard Hinnant の days_from_civil。
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    Some(era * 146097 + doe - 719468)
}

fn str_of(v: &serde_json::Value, k: &str) -> String {
    v[k].as_str().unwrap_or("").to_string()
}

fn load_events(timeline: &Path) -> Vec<Event> {
    let Ok(bytes) = std::fs::read(timeline) else { return Vec::new() };
    let Ok(v) = serde_json::from_slice::<serde_json::Value>(&bytes) else { return Vec::new() };
    v["events"]
        .as_array()
        .map(|a| a.as_slice())
        .unwrap_or(&[])
        .iter()
        .filter(|e| matches!(e["event_type"].as_str(), Some("enactment") | Some("amendment")))
        .filter_map(|e| {
            let promulgation = e["promulgation_date"].as_str()?.to_string();
            days(&promulgation)?;
            Some(Event {
                id: str_of(e, "event_id"),
                title: str_of(e, "amending_law_title"),
                num: str_of(e, "amending_law_num"),
                promulgation,
                effective: e["effective_date"].as_str().map(String::from),
                kanpo_pdf_url: e["kanpo"]["pdf_url"].as_str().map(String::from),
            })
        })
        .collect()
}

/// 題名一致か。短すぎる題名・番号は誤一致を避けるため使わない。
fn title_matches(case_title: &str, ev: &Event) -> Option<String> {
    if ev.title.chars().count() >= 6 && case_title.contains(&ev.title) {
        return Some(format!("title:{}", ev.title));
    }
    if ev.num.chars().count() >= 6 && case_title.contains(&ev.num) {
        return Some(format!("law_num:{}", ev.num));
    }
    None
}

/// パブコメ 1 件に対する改正イベントの選択。(イベント添字, confidence, 理由)
fn pick_event(case: &Case, events: &[Event]) -> Option<(usize, f32, String)> {
    let strong: Vec<(usize, String)> = events
        .iter()
        .enumerate()
        .filter_map(|(i, e)| title_matches(&case.title, e).map(|r| (i, r)))
        .collect();
    if let [(i, r)] = strong.as_slice() {
        return Some((*i, 0.9, r.clone()));
    }
    // 題名一致が複数あれば、結果公示に最も近い公布を採る。
    let anchor = case.result.as_deref().or(Some(case.end.as_str())).and_then(days);
    if strong.len() > 1 {
        let a = anchor?;
        let (i, r) = strong
            .iter()
            .min_by_key(|(i, _)| (days(&events[*i].promulgation).unwrap_or(i64::MAX) - a).abs())?;
        return Some((*i, 0.9, r.clone()));
    }
    let a = anchor?;
    let near: Vec<usize> = events
        .iter()
        .enumerate()
        .filter(|(_, e)| {
            days(&e.promulgation)
                .map(|p| p - a >= -WINDOW_BEFORE_DAYS && p - a <= WINDOW_AFTER_DAYS)
                .unwrap_or(false)
        })
        .map(|(i, _)| i)
        .collect();
    match near.as_slice() {
        [i] => Some((*i, 0.5, "date_proximity".to_string())),
        _ => None,
    }
}

fn case_steps(c: &Case) -> Vec<Step> {
    let mut out = Vec::new();
    let mut push = |kind: &str, date: &str, label: &str| {
        if days(date).is_some() {
            out.push(Step {
                kind: kind.into(),
                date: date[..10].to_string(),
                label: label.into(),
                ref_id: c.id.clone(),
            });
        }
    };
    push("pubcomment_start", &c.start, "意見募集開始");
    push("pubcomment_end", &c.end, "意見募集終了");
    if let Some(r) = &c.result {
        push("pubcomment_result", r, "結果公示");
    }
    out
}

fn sort_steps(steps: &mut [Step]) {
    // 同日なら 意見募集 → 結果公示 → 公布 → 施行 の順。
    let rank = |k: &str| match k {
        "pubcomment_start" => 0,
        "pubcomment_end" => 1,
        "pubcomment_result" => 2,
        "promulgation" => 3,
        _ => 4,
    };
    steps.sort_by(|a, b| (a.date.as_str(), rank(&a.kind)).cmp(&(b.date.as_str(), rank(&b.kind))));
}

fn build_chains(cases: &[Case], events: &[Event]) -> Vec<Chain> {
    // イベント添字 → (結ばれたパブコメ, 最小 confidence, 理由)
    let mut by_event: BTreeMap<usize, (Vec<&Case>, f32, Vec<String>)> = BTreeMap::new();
    let mut loose: Vec<&Case> = Vec::new();
    for c in cases {
        match pick_event(c, events) {
            Some((i, conf, reason)) => {
                let e = by_event.entry(i).or_insert((Vec::new(), 1.0, Vec::new()));
                e.0.push(c);
                e.1 = e.1.min(conf);
                if !e.2.contains(&reason) {
                    e.2.push(reason);
                }
            }
            None => loose.push(c),
        }
    }
    let mut chains = Vec::new();
    for (i, (cs, conf, reasons)) in by_event {
        let ev = &events[i];
        let mut steps: Vec<Step> = cs.iter().flat_map(|c| case_steps(c)).collect();
        let label = if ev.title.is_empty() { "公布".to_string() } else { format!("公布: {}", ev.title) };
        steps.push(Step {
            kind: "promulgation".into(),
            date: ev.promulgation[..10].to_string(),
            label,
            ref_id: ev.id.clone(),
        });
        if let Some(eff) = ev.effective.as_deref().filter(|d| days(d).is_some()) {
            steps.push(Step {
                kind: "effective".into(),
                date: eff[..10].to_string(),
                label: "施行".into(),
                ref_id: ev.id.clone(),
            });
        }
        sort_steps(&mut steps);
        chains.push(Chain {
            chain_id: ev.id.clone(),
            confidence: conf,
            match_reasons: reasons,
            steps,
            pubcomment_case_ids: cs.iter().map(|c| c.id.clone()).collect(),
            resolved: true,
            kanpo_pdf_url: ev.kanpo_pdf_url.clone(),
        });
    }
    for c in loose {
        let mut steps = case_steps(c);
        sort_steps(&mut steps);
        chains.push(Chain {
            chain_id: format!("pubcomment:{}", c.id),
            confidence: 0.0,
            match_reasons: Vec::new(),
            steps,
            pubcomment_case_ids: vec![c.id.clone()],
            resolved: false,
            kanpo_pdf_url: None,
        });
    }
    // 新しい連鎖 (最終ステップの日付が新しい順) から。
    chains.sort_by(|a, b| {
        let last = |c: &Chain| c.steps.last().map(|s| s.date.clone()).unwrap_or_default();
        last(b).cmp(&last(a))
    });
    chains
}

pub fn run_link_rulemaking(public: &Path) -> Result<()> {
    let links = public.join("links").join("law-to-pubcomment");
    if !links.is_dir() {
        tracing::warn!("no {}; skipping rulemaking", links.display());
        return Ok(());
    }
    let out_dir = public.join("links").join("law-to-rulemaking");
    std::fs::create_dir_all(&out_dir)?;

    let mut written = 0usize;
    let (mut resolved, mut unresolved) = (0usize, 0usize);
    for entry in std::fs::read_dir(&links)? {
        let path = entry?.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let v: serde_json::Value = serde_json::from_slice(&std::fs::read(&path)?)
            .with_context(|| format!("parse {}", path.display()))?;
        let law_id = str_of(&v, "law_id");
        if law_id.is_empty() {
            continue;
        }
        // 改正に関係するパブコメだけを対象にする (言及のみは除く)。
        let mut cases: Vec<Case> = Vec::new();
        for lp in v["linked_pubcomments"].as_array().map(|a| a.as_slice()).unwrap_or(&[]) {
            if lp["relevance"].as_str() != Some("amendment_comment") {
                continue;
            }
            let id = str_of(lp, "case_id");
            let detail: serde_json::Value = std::fs::read(public.join("pubcomment").join(format!("{id}.json")))
                .ok()
                .and_then(|b| serde_json::from_slice(&b).ok())
                .unwrap_or(serde_json::Value::Null);
            cases.push(Case {
                title: str_of(lp, "title"),
                start: str_of(lp, "start_date"),
                end: str_of(lp, "end_date"),
                result: detail["result_published"].as_str().map(String::from),
                id,
            });
        }
        if cases.is_empty() {
            continue;
        }
        let events = load_events(&public.join("laws").join(&law_id).join("timeline.json"));
        let chains = build_chains(&cases, &events);
        resolved += chains.iter().filter(|c| c.resolved).map(|c| c.pubcomment_case_ids.len()).sum::<usize>();
        unresolved += chains.iter().filter(|c| !c.resolved).count();
        std::fs::write(
            out_dir.join(format!("{law_id}.json")),
            serde_json::to_string_pretty(&LawToRulemaking { schema_version: 1, law_id, chains })?,
        )?;
        written += 1;
    }
    std::fs::write(
        out_dir.join("_summary.json"),
        serde_json::to_string_pretty(&serde_json::json!({
            "schema_version": 1,
            "laws": written,
            "pubcomments_resolved": resolved,
            "pubcomments_unresolved": unresolved,
        }))?,
    )?;
    tracing::info!("link-rulemaking: {written} laws, resolved={resolved} unresolved={unresolved}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(id: &str, title: &str, prom: &str, eff: Option<&str>) -> Event {
        Event {
            id: id.into(),
            title: title.into(),
            num: String::new(),
            promulgation: prom.into(),
            effective: eff.map(String::from),
            kanpo_pdf_url: None,
        }
    }
    fn case(id: &str, title: &str, result: Option<&str>) -> Case {
        Case {
            id: id.into(),
            title: title.into(),
            start: "2024-01-10".into(),
            end: "2024-02-08".into(),
            result: result.map(String::from),
        }
    }

    #[test]
    fn days_is_monotonic_and_correct() {
        assert_eq!(days("1970-01-01"), Some(0));
        assert_eq!(days("2024-03-01").unwrap() - days("2024-02-28").unwrap(), 2); // 閏年
        assert_eq!(days("2024-13-01"), None);
        assert_eq!(days("bad"), None);
    }

    #[test]
    fn title_match_beats_proximity() {
        let events = vec![
            ev("A", "テスト法の一部を改正する法律", "2024-06-01", Some("2024-10-01")),
            ev("B", "別の法の一部を改正する法律", "2024-04-01", None),
        ];
        let c = case("1", "テスト法の一部を改正する法律案に関する意見募集", Some("2024-03-20"));
        let (i, conf, reason) = pick_event(&c, &events).unwrap();
        assert_eq!((i, conf), (0, 0.9));
        assert!(reason.starts_with("title:"));
    }

    #[test]
    fn proximity_only_when_unique() {
        let one = vec![ev("A", "", "2024-04-01", None)];
        let c = case("1", "施行規則の改正について", Some("2024-03-20"));
        assert_eq!(pick_event(&c, &one).map(|(i, c, _)| (i, c)), Some((0, 0.5)));

        let two = vec![ev("A", "", "2024-04-01", None), ev("B", "", "2024-05-01", None)];
        assert!(pick_event(&c, &two).is_none());

        // 窓の外 (結果公示の 2 年後) は結ばない。
        let far = vec![ev("A", "", "2026-04-01", None)];
        assert!(pick_event(&c, &far).is_none());
    }

    #[test]
    fn chain_is_ordered_and_unresolved_kept() {
        let events = vec![ev("A", "テスト法の一部を改正する法律", "2024-06-01", Some("2024-10-01"))];
        let cases = vec![
            case("1", "テスト法の一部を改正する法律案への意見募集", Some("2024-03-20")),
            case("2", "全く関係のない案件", Some("2020-01-01")),
        ];
        let chains = build_chains(&cases, &events);
        assert_eq!(chains.len(), 2);
        let c = &chains[0];
        assert!(c.resolved);
        let kinds: Vec<_> = c.steps.iter().map(|s| s.kind.as_str()).collect();
        assert_eq!(
            kinds,
            ["pubcomment_start", "pubcomment_end", "pubcomment_result", "promulgation", "effective"]
        );
        assert!(!chains[1].resolved);
        assert_eq!(chains[1].chain_id, "pubcomment:2");
    }

    #[test]
    fn end_to_end_writes_files_and_summary() {
        let dir = std::env::temp_dir().join(format!("lawpub_rulemaking_{}", std::process::id()));
        let w = |p: &str, v: serde_json::Value| {
            let path = dir.join(p);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, serde_json::to_vec(&v).unwrap()).unwrap();
        };
        w(
            "links/law-to-pubcomment/L1.json",
            serde_json::json!({"law_id": "L1", "linked_pubcomments": [
                {"case_id": "C1", "title": "テスト法の一部を改正する法律案への意見募集",
                 "start_date": "2024-01-10", "end_date": "2024-02-08", "relevance": "amendment_comment"},
                {"case_id": "C2", "title": "言及のみ", "start_date": "", "end_date": "", "relevance": "reference_only"}
            ]}),
        );
        w("pubcomment/C1.json", serde_json::json!({"result_published": "2024-03-20"}));
        w(
            "laws/L1/timeline.json",
            serde_json::json!({"events": [
                {"event_id": "E1", "event_type": "amendment", "amending_law_title": "テスト法の一部を改正する法律",
                 "promulgation_date": "2024-06-01", "effective_date": "2024-10-01",
                 "kanpo": {"linked": true, "pdf_url": "https://example.com/k.pdf"}}
            ]}),
        );
        run_link_rulemaking(&dir).unwrap();
        let out: LawToRulemaking =
            serde_json::from_slice(&std::fs::read(dir.join("links/law-to-rulemaking/L1.json")).unwrap()).unwrap();
        assert_eq!(out.chains.len(), 1); // 言及のみは対象外
        assert_eq!(out.chains[0].kanpo_pdf_url.as_deref(), Some("https://example.com/k.pdf"));
        let s: serde_json::Value =
            serde_json::from_slice(&std::fs::read(dir.join("links/law-to-rulemaking/_summary.json")).unwrap()).unwrap();
        assert_eq!(s["pubcomments_resolved"], 1);
        assert_eq!(s["pubcomments_unresolved"], 0);
        std::fs::remove_dir_all(&dir).ok();
    }
}
