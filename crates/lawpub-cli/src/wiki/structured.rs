//! 議案 (gian) と法令の改正履歴 (laws/{id}/timeline.json) を、LLM を使わずに wiki へ取り込む。
//!
//! 「どうなったか」(提出 → 衆参の採決 → 公布 → 施行) は構造化データにあるので、要約させずに
//! そのまま事実として載せる。`wiki-plan` から毎回呼ばれ、ページは丸ごと再生成する。
//!
//! - 議案ページ `bills/{session}/{bill_id}.md` (type: bill): 審議経過、会派の賛否、対象法令。
//!   対象法令は件名から決める (`X の一部を改正する法律案` → X、`A及びB` は分割、制定法は `…法律案` → `…法律`)。
//! - 法令ページの frontmatter `revisions`: 直近 10 年の公布・施行 (予定) の一覧。
//!   finalize がこれと会議・議案を合わせて 1 本の時系列表にする。

use super::*;
use serde_json::json;
use std::collections::{BTreeSet, HashMap};

const LAW_BILL_TYPES: [&str; 3] = ["閣法", "衆法", "参法"];
const REVISION_YEARS: i64 = 10;
const MAX_REVISIONS: usize = 30;
/// 議案ページの描画形式。変えたら上げると、経過に変化が無い議案も描き直す。
const BILL_RENDER_VERSION: u64 = 2;

#[derive(Debug, Default)]
pub struct SyncStats {
    pub bills: usize,
    pub bills_fetched: usize,
    pub laws_created: usize,
    pub laws_with_revisions: usize,
}

pub fn sync(source: &Source, wiki: &Path, today: chrono::NaiveDate) -> Result<SyncStats> {
    let mut stats = SyncStats::default();
    let Some(laws_index) = source.get_json("laws/index.json")? else {
        bail!("laws/index.json がありません");
    };
    let mut by_title: HashMap<String, Vec<String>> = HashMap::new();
    let mut titles: HashMap<String, String> = HashMap::new();
    for l in laws_index["laws"].as_array().into_iter().flatten() {
        let (Some(id), Some(title)) = (l["law_id"].as_str(), l["title"].as_str()) else { continue };
        by_title.entry(title.to_string()).or_default().push(id.to_string());
        titles.insert(id.to_string(), title.to_string());
    }
    // 同名の法令が複数あるタイトルは対応付けに使わない。
    let unique: HashMap<String, String> = by_title
        .into_iter()
        .filter_map(|(t, ids)| (ids.len() == 1).then(|| (t, ids[0].clone())))
        .collect();

    sync_bills(source, wiki, &unique, &titles, &mut stats)?;
    sync_revisions(source, wiki, today, &mut stats)?;
    Ok(stats)
}

// ── 議案 ───────────────────────────────────────────────────────

fn sync_bills(
    source: &Source,
    wiki: &Path,
    unique: &HashMap<String, String>,
    titles: &HashMap<String, String>,
    stats: &mut SyncStats,
) -> Result<()> {
    let Some(index) = source.get_json("gian/index.json")? else {
        return Ok(());
    };
    for b in index["bills"].as_array().into_iter().flatten() {
        let bill_type = b["bill_type"].as_str().unwrap_or("");
        if !LAW_BILL_TYPES.contains(&bill_type) {
            continue;
        }
        let (Some(session), Some(bill_id), Some(title)) =
            (b["session"].as_u64(), b["bill_id"].as_str(), b["title"].as_str())
        else {
            continue;
        };
        let targets = bill_targets(title, unique);
        if targets.is_empty() {
            continue;
        }
        stats.bills += 1;
        let rel = bill_page(session, bill_id);
        let path = wiki.join(&rel);
        let latest = b["latest_date"].as_str().unwrap_or("");
        // 経過に変化が無ければ詳細を引き直さない。
        if path.exists() {
            if let Ok(p) = Page::read(&path) {
                let same_render = p.get("render_version").and_then(Value::as_u64) == Some(BILL_RENDER_VERSION);
                if same_render && p.get_str("latest_date") == latest && !latest.is_empty() {
                    continue;
                }
            }
        }
        let detail = match source.get_json(&format!("gian/{session}/{bill_id}.json")) {
            Ok(Some(d)) => d,
            Ok(None) => continue,
            Err(e) => {
                tracing::warn!("wiki-plan: 議案 {session}/{bill_id}: {e:#}");
                continue;
            }
        };
        stats.bills_fetched += 1;
        for law_id in &targets {
            let title = titles.get(law_id).map(String::as_str).unwrap_or(law_id);
            if ensure_law_page(wiki, source.base(), law_id, title)? {
                stats.laws_created += 1;
            }
        }
        // LLM の書いた「概要と経緯」と、その時点の経過 (llm_latest_date) は描き直しても残す。
        let previous = if path.exists() { Page::read(&path).ok() } else { None };
        let mut page = render_bill(&detail, b, &rel, &targets, titles)?;
        if let Some(prev) = previous {
            let narrative = first_llm_block(&prev.body);
            page.body = page.body.replacen(&format!("{LLM_BEGIN}\n{LLM_END}"), &if narrative.is_empty() {
                format!("{LLM_BEGIN}\n{LLM_END}")
            } else {
                format!("{LLM_BEGIN}\n{narrative}\n{LLM_END}")
            }, 1);
            if let Some(v) = prev.get("llm_latest_date") {
                page.set("llm_latest_date", v.clone());
            }
            if let Some(v) = prev.get("tags") {
                page.set("tags", v.clone());
            }
        }
        page.write(&path)?;
    }
    Ok(())
}

/// 議案件名から対象法令 ID を決める。対応付けられなければ空。
pub(crate) fn bill_targets(title: &str, unique: &HashMap<String, String>) -> Vec<String> {
    if let Some(core) = title.strip_suffix("の一部を改正する法律案") {
        let core = core.strip_suffix('等').unwrap_or(core);
        return split_laws(core, unique).unwrap_or_default();
    }
    // 制定法: 成立・公布後は「…法律案」の「案」を落とした名前で法令になる。
    title
        .strip_suffix('案')
        .and_then(|t| unique.get(t))
        .map(|id| vec![id.clone()])
        .unwrap_or_default()
}

/// 「A及びB」を法令名の並びに分ける。法令名自体に「及び」を含むことがあるため、
/// まず全体を、次に左側が法令名として成り立つ位置で分割を試す。
fn split_laws(s: &str, unique: &HashMap<String, String>) -> Option<Vec<String>> {
    if let Some(id) = unique.get(s) {
        return Some(vec![id.clone()]);
    }
    for (i, _) in s.match_indices("及び") {
        let (left, right) = (&s[..i], &s[i + "及び".len()..]);
        if let (Some(l), Some(mut r)) = (unique.get(left), split_laws(right, unique)) {
            r.insert(0, l.clone());
            return Some(r);
        }
    }
    None
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Stage {
    pub date: String,
    /// `received` / `referred` / `committee` / `plenary` / `promulgated`。
    pub kind: &'static str,
    pub label: String,
}

/// 議案の経過 (fields) を日付付きの段階に変換する。予備審査は除く。
pub(crate) fn bill_stages(fields: &[(String, String)]) -> Vec<Stage> {
    let mut out = Vec::new();
    for (key, value) in fields {
        if key.contains("予備") {
            continue;
        }
        let (date_part, extra) = match value.split_once('／') {
            Some((d, e)) => (d.trim(), e.trim()),
            None => (value.trim(), ""),
        };
        let Some(date) = parse_wareki(date_part) else { continue };
        let date = date.format("%Y-%m-%d").to_string();
        let house = if key.starts_with("衆議院") {
            "衆議院"
        } else if key.starts_with("参議院") {
            "参議院"
        } else {
            ""
        };
        let (kind, label) = if key.starts_with("公布年月日") {
            ("promulgated", if extra.is_empty() { "公布".to_string() } else { format!("公布（法律第{extra}号）") })
        } else if key.ends_with("議案受理年月日") {
            ("received", format!("{house}で受理"))
        } else if key.contains("付託年月日") {
            ("referred", format!("{house} {extra}に付託"))
        } else if key.contains("審査終了年月日") {
            ("committee", format!("{house} 委員会で{extra}"))
        } else if key.contains("審議終了年月日") {
            ("plenary", format!("{house} 本会議で{extra}"))
        } else {
            continue;
        };
        out.push(Stage { date, kind, label: label.trim().to_string() });
    }
    out.sort_by(|a, b| a.date.cmp(&b.date));
    out
}

fn render_bill(
    detail: &Value,
    summary: &Value,
    rel: &str,
    targets: &[String],
    titles: &HashMap<String, String>,
) -> Result<Page> {
    let fields: Vec<(String, String)> = detail["fields"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|f| Some((f["key"].as_str()?.to_string(), f["value"].as_str()?.to_string())))
        .collect();
    let field = |k: &str| fields.iter().find(|(key, _)| key == k).map(|(_, v)| v.trim()).unwrap_or("");
    let stages = bill_stages(&fields);
    let title = detail["title"].as_str().or(summary["title"].as_str()).unwrap_or("");
    let session = summary["session"].as_u64().unwrap_or(0);
    let bill_type = summary["bill_type"].as_str().unwrap_or("");
    let number = detail["number"].as_str().or(summary["number"].as_str()).unwrap_or("");
    let result = detail["result"].as_str().or(summary["result"].as_str()).unwrap_or("");
    let url = summary["detail_url"].as_str().unwrap_or("");
    let law_num_text = match (detail["promulgation_date"].as_str(), detail["law_num"].as_str()) {
        (Some(d), Some(n)) => law_num_text(d, n),
        _ => None,
    };
    let latest = stages.last();
    let description = match latest {
        Some(s) => format!("{bill_type}（第{session}回国会）: {} {}", s.date, s.label),
        None => format!("{bill_type}（第{session}回国会）: {result}"),
    };

    let law_links: Vec<String> = targets
        .iter()
        .map(|id| format!("[{}]({})", titles.get(id).map(String::as_str).unwrap_or(id), rel_link(rel, &law_page(id))))
        .collect();
    let mut body = format!(
        "\n# {title}\n\n## 概要と経緯\n\n{LLM_BEGIN}\n{LLM_END}\n\n<!-- lawpub:begin meta -->\n## 議案の情報（一覧）\n\n| 項目 | 内容 |\n|---|---|\n"
    );
    let number_part = if number.is_empty() { String::new() } else { format!(" 第{number}号") };
    body.push_str(&format!("| 種類 | {bill_type}（第{session}回国会{number_part}） |\n"));
    for (label, key) in [("提出者", "議案提出者"), ("提出会派", "議案提出会派")] {
        let v = field(key);
        if !v.is_empty() {
            body.push_str(&format!("| {label} | {} |\n", cell(&v.replace("; ", "、"))));
        }
    }
    if !result.is_empty() {
        body.push_str(&format!("| 結果 | {} |\n", cell(result)));
    }
    if let Some(n) = &law_num_text {
        body.push_str(&format!("| 法律番号 | {n} |\n"));
    }
    body.push_str(&format!("| 対象法令 | {} |\n", law_links.join("、")));
    if !url.is_empty() {
        body.push_str(&format!("| 原文 | [衆議院 議案審議経過]({url}) |\n"));
    }
    body.push_str("\n## 審議経過\n\n| 日付 | 経過 |\n|---|---|\n");
    for s in &stages {
        body.push_str(&format!("| {} | {} |\n", s.date, cell(&s.label)));
    }
    let mut votes = String::new();
    for house in ["衆議院", "参議院"] {
        for (side, key) in [("賛成", "審議時賛成会派"), ("反対", "審議時反対会派")] {
            let v = field(&format!("{house}{key}"));
            if !v.is_empty() {
                votes.push_str(&format!("- {house} {side}: {}\n", v.replace("; ", "、")));
            }
        }
    }
    if !votes.is_empty() {
        body.push_str("\n## 会派の賛否\n\n");
        body.push_str(&votes);
    }
    body.push_str("<!-- lawpub:end meta -->\n");

    Ok(Page {
        frontmatter: vec![
            ("type".into(), json!("bill")),
            ("title".into(), json!(title)),
            ("description".into(), json!(description)),
            ("resource".into(), json!(url)),
            ("timestamp".into(), json!(now_rfc3339())),
            ("date".into(), json!(latest.map(|s| s.date.clone()))),
            ("session".into(), json!(session)),
            ("bill_id".into(), json!(summary["bill_id"])),
            ("bill_type".into(), json!(bill_type)),
            ("result".into(), json!(result)),
            ("latest_date".into(), json!(summary["latest_date"])),
            ("law_num_text".into(), json!(law_num_text)),
            ("laws".into(), json!(targets)),
            (
                "stages".into(),
                Value::Array(stages.iter().map(|s| json!({"date": s.date, "kind": s.kind, "label": s.label})).collect()),
            ),
            ("tags".into(), json!([])),
            ("render_version".into(), json!(BILL_RENDER_VERSION)),
        ],
        body,
    })
}

// ── 改正履歴 ────────────────────────────────────────────────────

fn sync_revisions(source: &Source, wiki: &Path, today: chrono::NaiveDate, stats: &mut SyncStats) -> Result<()> {
    let since = (today - chrono::Duration::days(365 * REVISION_YEARS)).format("%Y-%m-%d").to_string();
    for path in walk_md(wiki, "laws") {
        let mut page = Page::read(&path)?;
        let law_id = page.get_str("law_id").to_string();
        let timeline = match source.get_json(&format!("laws/{law_id}/timeline.json")) {
            Ok(Some(t)) => t,
            Ok(None) => continue,
            Err(e) => {
                tracing::warn!("wiki-plan: 改正履歴 {law_id}: {e:#}");
                continue;
            }
        };
        let revisions = revisions_from_timeline(&timeline, &since);
        if !revisions.is_empty() {
            stats.laws_with_revisions += 1;
        }
        let value = Value::Array(revisions);
        if page.get("revisions") != Some(&value) {
            page.set("revisions", value);
            page.write(&path)?;
        }
    }
    Ok(())
}

pub(crate) fn revisions_from_timeline(timeline: &Value, since: &str) -> Vec<Value> {
    let mut seen = BTreeSet::new();
    let mut out: Vec<(String, Value)> = Vec::new();
    for e in timeline["events"].as_array().into_iter().flatten() {
        let ty = e["event_type"].as_str().unwrap_or("");
        if ty != "enactment" && ty != "amendment" {
            continue;
        }
        let name = e["amending_law_title"].as_str().filter(|t| !t.is_empty());
        let num = e["amending_law_num"].as_str().filter(|t| !t.is_empty());
        let what = match (ty, name, num) {
            ("enactment", _, _) => "制定".to_string(),
            (_, Some(n), Some(num)) => format!("{n}（{num}）"),
            (_, Some(n), None) => n.to_string(),
            (_, None, Some(num)) => num.to_string(),
            _ => "改正".to_string(),
        };
        let mut push = |date: Option<&str>, kind: &str, label: String| {
            let Some(date) = date.filter(|d| *d >= since) else { return };
            if seen.insert((date.to_string(), kind.to_string(), label.clone())) {
                out.push((date.to_string(), json!({"date": date, "kind": kind, "label": label, "law_num": num})));
            }
        };
        push(e["promulgation_date"].as_str(), "promulgated", format!("公布: {what}"));
        match (e["effective_date"].as_str(), e["scheduled_enforcement_date"].as_str()) {
            (Some(d), _) => push(Some(d), "enforced", format!("施行: {what}")),
            (None, Some(d)) => push(Some(d), "scheduled", format!("施行予定: {what}")),
            _ => {}
        }
    }
    out.sort_by(|a, b| b.0.cmp(&a.0));
    out.into_iter().take(MAX_REVISIONS).map(|(_, v)| v).collect()
}

// ── 和暦・漢数字 ────────────────────────────────────────────────

const ERAS: [(&str, i32); 3] = [("令和", 2018), ("平成", 1988), ("昭和", 1925)];

fn ascii_digits(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '０'..='９' => char::from_u32(c as u32 - '０' as u32 + '0' as u32).unwrap_or(c),
            _ => c,
        })
        .collect()
}

/// `令和 8年 7月31日` → 2026-07-31。
pub(crate) fn parse_wareki(s: &str) -> Option<chrono::NaiveDate> {
    let s: String = ascii_digits(s).chars().filter(|c| !c.is_whitespace()).collect();
    let (era, base) = ERAS.iter().find(|(e, _)| s.starts_with(e))?;
    let rest = &s[era.len()..];
    let (y, rest) = rest.split_once('年')?;
    let (m, rest) = rest.split_once('月')?;
    let d = rest.strip_suffix('日')?;
    let y: i32 = if y == "元" { 1 } else { y.parse().ok()? };
    chrono::NaiveDate::from_ymd_opt(base + y, m.parse().ok()?, d.parse().ok()?)
}

/// 法令番号の漢数字 (`73` → `七十三`)。
pub(crate) fn kanji_number(mut n: u32) -> String {
    const D: [&str; 10] = ["", "一", "二", "三", "四", "五", "六", "七", "八", "九"];
    if n == 0 {
        return "〇".into();
    }
    let mut out = String::new();
    for (unit, name) in [(1000, "千"), (100, "百"), (10, "十")] {
        let q = n / unit;
        if q > 0 {
            if q > 1 {
                out.push_str(D[q as usize]);
            }
            out.push_str(name);
        }
        n %= unit;
    }
    out.push_str(D[n as usize]);
    out
}

/// 議案の公布日と法律番号から e-Gov 形式の法令番号 (`令和八年法律第七十三号`) を作る。
pub(crate) fn law_num_text(promulgation: &str, num: &str) -> Option<String> {
    let s: String = ascii_digits(promulgation).chars().filter(|c| !c.is_whitespace()).collect();
    let (era, _) = ERAS.iter().find(|(e, _)| s.starts_with(e))?;
    let y = s[era.len()..].split_once('年')?.0;
    let year = if y == "元" { "元".to_string() } else { kanji_number(y.parse().ok()?) };
    let year = if year == "一" { "元".to_string() } else { year };
    let n: u32 = ascii_digits(num).trim().parse().ok()?;
    Some(format!("{era}{year}年法律第{}号", kanji_number(n)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn titles(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs.iter().map(|(t, id)| (t.to_string(), id.to_string())).collect()
    }

    #[test]
    fn wareki_and_kanji_law_numbers() {
        assert_eq!(parse_wareki("令和 8年 7月31日").unwrap().to_string(), "2026-07-31");
        assert_eq!(parse_wareki("令和元年５月１日").unwrap().to_string(), "2019-05-01");
        assert!(parse_wareki("").is_none());
        assert_eq!(kanji_number(73), "七十三");
        assert_eq!(kanji_number(110), "百十");
        assert_eq!(kanji_number(1203), "千二百三");
        assert_eq!(law_num_text("令和 8年 7月31日", "73").as_deref(), Some("令和八年法律第七十三号"));
        assert_eq!(law_num_text("令和 1年 6月 1日", "5").as_deref(), Some("令和元年法律第五号"));
    }

    #[test]
    fn bill_titles_map_to_target_laws() {
        let m = titles(&[
            ("日本国憲法の改正手続に関する法律", "L1"),
            ("組織的な犯罪の処罰及び犯罪収益の規制等に関する法律", "L2"),
            ("刑事訴訟法", "L3"),
            ("自動車産業における脱炭素化の推進に関する法律", "L4"),
        ]);
        assert_eq!(bill_targets("日本国憲法の改正手続に関する法律の一部を改正する法律案", &m), vec!["L1"]);
        // 法令名の中の「及び」で誤って分割しない。
        assert_eq!(
            bill_targets("組織的な犯罪の処罰及び犯罪収益の規制等に関する法律及び刑事訴訟法の一部を改正する法律案", &m),
            vec!["L2", "L3"]
        );
        assert_eq!(bill_targets("刑事訴訟法等の一部を改正する法律案", &m), vec!["L3"]);
        assert_eq!(bill_targets("自動車産業における脱炭素化の推進に関する法律案", &m), vec!["L4"]);
        assert!(bill_targets("未成立の新法案", &m).is_empty());
    }

    #[test]
    fn bill_fields_become_dated_stages() {
        let f = |k: &str, v: &str| (k.to_string(), v.to_string());
        let fields = vec![
            f("議案件名", "x"),
            f("参議院予備審査議案受理年月日", "令和 8年 6月 8日"),
            f("衆議院議案受理年月日", "令和 8年 6月 5日"),
            f("衆議院付託年月日／衆議院付託委員会", "令和 8年 6月10日 ／ 憲法審査会"),
            f("衆議院審議終了年月日／衆議院審議結果", "令和 8年 6月19日 ／ 可決"),
            f("参議院審議終了年月日／参議院審議結果", "令和 8年 7月24日 ／ 可決"),
            f("公布年月日／法律番号", "令和 8年 7月31日 ／ 73"),
            f("衆議院予備付託年月日／衆議院予備付託委員会", "／"),
        ];
        let stages = bill_stages(&fields);
        let got: Vec<(&str, &str, &str)> = stages.iter().map(|s| (s.date.as_str(), s.kind, s.label.as_str())).collect();
        assert_eq!(
            got,
            vec![
                ("2026-06-05", "received", "衆議院で受理"),
                ("2026-06-10", "referred", "衆議院 憲法審査会に付託"),
                ("2026-06-19", "plenary", "衆議院 本会議で可決"),
                ("2026-07-24", "plenary", "参議院 本会議で可決"),
                ("2026-07-31", "promulgated", "公布（法律第73号）"),
            ]
        );
    }

    #[test]
    fn revisions_keep_recent_promulgation_and_enforcement() {
        let t = json!({"events": [
            {"event_type": "enactment", "promulgation_date": "2014-05-30", "effective_date": null},
            {"event_type": "amendment", "amending_law_title": "難病法の一部を改正する法律", "amending_law_num": "令和八年法律第十号",
             "promulgation_date": "2026-03-31", "effective_date": null, "scheduled_enforcement_date": "2027-04-01"},
            {"event_type": "snapshot", "promulgation_date": "2026-01-01"}
        ]});
        let r = revisions_from_timeline(&t, "2016-10-08");
        assert_eq!(r.len(), 2);
        assert_eq!(r[0]["label"], "施行予定: 難病法の一部を改正する法律（令和八年法律第十号）");
        assert_eq!(r[1]["kind"], "promulgated");
        assert_eq!(r[1]["law_num"], "令和八年法律第十号");
    }

    #[test]
    fn sync_writes_bill_pages_law_pages_and_revisions() {
        let root = temp_dir("structured");
        let public = root.join("public");
        let wiki = root.join("wiki");
        let write = |rel: &str, v: Value| {
            let p = public.join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, serde_json::to_vec(&v).unwrap()).unwrap();
        };
        write("laws/index.json", json!({"laws": [{"law_id": "L1", "title": "日本国憲法の改正手続に関する法律"}]}));
        write("gian/index.json", json!({"bills": [
            {"bill_id": "B1", "session": 221, "bill_type": "衆法", "title": "日本国憲法の改正手続に関する法律の一部を改正する法律案",
             "latest_date": "2026-07-31", "detail_url": "https://www.shugiin.go.jp/x/B1.htm", "result": "可決"},
            {"bill_id": "T1", "session": 221, "bill_type": "条約", "title": "条約", "latest_date": "2026-07-01"}
        ]}));
        write("gian/221/B1.json", json!({
            "title": "日本国憲法の改正手続に関する法律の一部を改正する法律案", "number": "11", "result": "可決",
            "law_num": "73", "promulgation_date": "令和 8年 7月31日",
            "fields": [
                {"key": "議案提出会派", "value": "自由民主党・無所属の会; 日本維新の会"},
                {"key": "衆議院審議終了年月日／衆議院審議結果", "value": "令和 8年 6月19日 ／ 可決"},
                {"key": "衆議院審議時反対会派", "value": "日本共産党"},
                {"key": "公布年月日／法律番号", "value": "令和 8年 7月31日 ／ 73"}
            ]
        }));
        write("laws/L1/timeline.json", json!({"events": [
            {"event_type": "amendment", "amending_law_title": "日本国憲法の改正手続に関する法律の一部を改正する法律",
             "amending_law_num": "令和八年法律第七十三号", "promulgation_date": "2026-07-31", "effective_date": null}
        ]}));

        let source = Source::new(&public.display().to_string()).unwrap();
        let stats = sync(&source, &wiki, chrono::NaiveDate::from_ymd_opt(2026, 10, 8).unwrap()).unwrap();
        assert_eq!((stats.bills, stats.bills_fetched, stats.laws_created), (1, 1, 1));

        let bill = Page::read(&wiki.join("bills/221/B1.md")).unwrap();
        assert_eq!(bill.get_str("type"), "bill");
        assert_eq!(bill.get_str("law_num_text"), "令和八年法律第七十三号");
        assert_eq!(bill.get_str("description"), "衆法（第221回国会）: 2026-07-31 公布（法律第73号）");
        assert!(bill.body.contains("[日本国憲法の改正手続に関する法律](../../laws/L1.md)"));
        assert!(bill.body.contains("- 衆議院 反対: 日本共産党"));
        let law = Page::read(&wiki.join("laws/L1.md")).unwrap();
        assert_eq!(law.get("revisions").unwrap()[0]["law_num"], "令和八年法律第七十三号");

        // 経過に変化が無ければ詳細を取り直さない。
        let again = sync(&source, &wiki, chrono::NaiveDate::from_ymd_opt(2026, 10, 9).unwrap()).unwrap();
        assert_eq!(again.bills_fetched, 0);
        std::fs::remove_dir_all(root).ok();
    }
}
