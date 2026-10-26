//! `lawpub wiki-finalize`: LLM の書いた後に決定的な部分を再生成する。
//!
//! - 未完了タスク (description が空、または要点に引用が 1 つも無い) は、この plan で新規作成した
//!   ページを削除して未処理に戻す。翌日の run で再挑戦される。
//! - 完了タスクを state に `linked` として記録する。
//! - 会議ページの frontmatter (`laws` / `speakers` / `description`) から、法令ページの時系列表、
//!   人物ページ、`index.md` を作り直し、`log.md` に今日の分を追記する。

use super::check::citations_in;
use super::*;
use serde_json::json;
use std::collections::BTreeSet;


pub struct FinalizeArgs {
    pub wiki: PathBuf,
    pub work: PathBuf,
}

/// 人物ページに載せる発言の抜粋の数 (新しい順)。残りは「発言した会議（一覧）」からたどる。
const MAX_PERSON_QUOTES: usize = 12;

/// 法令の時系列に載せる議案の段階 (付託・委員会採決は議案ページだけに載せる)。
const TIMELINE_STAGES: [&str; 3] = ["received", "plenary", "promulgated"];

#[derive(Debug, Clone)]
struct BillInfo {
    page: String,
    title: String,
    date: String,
    description: String,
    law_num_text: Option<String>,
    laws: Vec<String>,
    /// (日付, kind, ラベル)。
    stages: Vec<(String, String, String)>,
    /// (院, 賛成|反対, 会派)。会派 (政党) ページの元。
    votes: Vec<(String, String, String)>,
    /// 提出会派 (議員立法)。
    proposer_groups: Vec<String>,
}

fn load_bills(wiki: &Path) -> Result<Vec<BillInfo>> {
    let mut out = Vec::new();
    for path in walk_md(wiki, "bills") {
        let page = Page::read(&path)?;
        let strs = |k: &str| -> Vec<String> {
            page.get(k)
                .and_then(Value::as_array)
                .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                .unwrap_or_default()
        };
        let stages = page
            .get("stages")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|s| {
                        Some((s["date"].as_str()?.to_string(), s["kind"].as_str()?.to_string(), s["label"].as_str()?.to_string()))
                    })
                    .collect()
            })
            .unwrap_or_default();
        let votes = page
            .get("votes")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .flat_map(|v| {
                        let house = v["house"].as_str().unwrap_or("").to_string();
                        let side = v["side"].as_str().unwrap_or("").to_string();
                        v["groups"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .filter_map(|g| g.as_str())
                            .map(|g| (house.clone(), side.clone(), g.to_string()))
                            .collect::<Vec<_>>()
                    })
                    .collect()
            })
            .unwrap_or_default();
        out.push(BillInfo {
            page: rel_path(wiki, &path),
            title: page.get_str("title").to_string(),
            date: page.get_str("date").to_string(),
            description: page.get_str("description").to_string(),
            law_num_text: page.get("law_num_text").and_then(Value::as_str).map(String::from),
            laws: strs("laws"),
            stages,
            votes,
            proposer_groups: strs("proposer_groups"),
        });
    }
    out.sort_by(|a, b| b.date.cmp(&a.date).then_with(|| a.page.cmp(&b.page)));
    Ok(out)
}

#[derive(Debug, Clone)]
struct PubcommentInfo {
    page: String,
    title: String,
    date: String,
    description: String,
    /// LLM が書いた 1 行 (募集状況の機械的な文と異なる場合だけ)。
    summary: Option<String>,
    laws: Vec<String>,
    reception_start: Option<String>,
    result_date: Option<String>,
    opinion_count: Option<u64>,
}

/// 法令ページの時系列から官報の項目へのリンク先と、LLM が書いた 1 行 (あれば)。
/// 項目ページ (`item_page`) があればそれを、無ければ官報の PDF を指す。
fn kanpo_target(wiki: &Path, from: &str, entry: &Value) -> (String, Option<String>) {
    if let Some(item) = entry["item_page"].as_str().filter(|p| wiki.join(p).exists()) {
        let summary = Page::read(&wiki.join(item)).ok().and_then(|p| {
            let d = p.get_str("description");
            (!d.is_empty() && d != p.get_str("stats")).then(|| d.to_string())
        });
        return (rel_link(from, item), summary);
    }
    (entry["pdf_url"].as_str().unwrap_or("").to_string(), None)
}

fn load_pubcomments(wiki: &Path) -> Result<Vec<PubcommentInfo>> {
    let mut out = Vec::new();
    for path in walk_md(wiki, "pubcomments") {
        let page = Page::read(&path)?;
        let description = page.get_str("description").to_string();
        let summary = (!description.is_empty() && description != page.get_str("stats")).then(|| description.clone());
        out.push(PubcommentInfo {
            page: rel_path(wiki, &path),
            title: page.get_str("title").to_string(),
            date: page.get_str("date").to_string(),
            description,
            summary,
            laws: page
                .get("laws")
                .and_then(Value::as_array)
                .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                .unwrap_or_default(),
            reception_start: page.get("reception_start").and_then(Value::as_str).map(String::from),
            result_date: page.get("result_date").and_then(Value::as_str).map(String::from),
            opinion_count: page.get("opinion_count").and_then(Value::as_u64),
        });
    }
    Ok(out)
}

/// 人物ページの 1 行: (会議, 会派, 言及した法令 ID)。
type Remark<'a> = (&'a MeetingInfo, &'a Speaker);

#[derive(Debug, Clone)]
struct Speaker {
    name: String,
    group: Option<String>,
    position: Option<String>,
    /// `kokkai` (国会議員・政府側・職員) / `official` (審議会の官職者) / `member` (審議会の委員)。
    role: String,
    laws: Vec<String>,
    /// その会議で法令に言及した発言の抜粋 (原文のまま) と、その発言の原文 URL。
    excerpt: Option<String>,
    excerpt_url: Option<String>,
}

#[derive(Debug, Clone)]
struct MeetingInfo {
    /// wiki の会議ページ。LLM がまだ要点を書いていない会議は空 (名簿だけ) で、原文へ直接つなぐ。
    page: String,
    /// 原文 (国会会議録・審議会の議事録ページ) の URL。
    url: String,
    title: String,
    date: String,
    /// `kokkai` / `shingikai`。
    corpus: String,
    description: String,
    laws: Vec<String>,
    organization: String,
    committee: String,
    speakers: Vec<Speaker>,
}

pub fn run_finalize(args: &FinalizeArgs) -> Result<()> {
    let wiki = &args.wiki;
    let plan = Plan::load(&args.work)?.unwrap_or_default();
    let mut state = State::load(wiki)?;
    let today = if plan.date.is_empty() {
        today_jst().format("%Y-%m-%d").to_string()
    } else {
        plan.date.clone()
    };

    // 1. 未完了タスクの巻き戻し。
    let mut completed = Vec::new();
    let mut rolled_back = 0usize;
    let mut created_laws: BTreeSet<String> = BTreeSet::new();
    for task in &plan.tasks {
        let path = wiki.join(&task.page);
        let done = path.exists()
            && Page::read(&path)
                .map(|p| {
                    !p.get_str("description").trim().is_empty()
                        && (p.get("evidence_version").is_none() || evidence::claim_errors(&p).is_empty())
                        && (task.kind != enforcements::KIND || enforcements::explanation_errors(&p).is_empty())
                        && (task.kind != hanrei::KIND
                            || (hanrei::summary_errors(&p).is_empty()
                                && hanrei::source_matches(wiki, &p).unwrap_or(false)
                                && hanrei_rag::planned_source_matches(&args.work, task, &p)
                                && hanrei_rag::page_record(wiki, &p).is_ok()))
                        && (!matches!(task.kind.as_str(), KIND_KOKKAI | KIND_SHINGIKAI)
                            || std::fs::read(args.work.join(&task.source)).ok()
                                .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
                                .map(|bundle| if bundle.get("source_fingerprint").is_some() {
                                    evidence::source_fields_match(&p, &bundle, state.is_processed(&task.key))
                                } else { p.get("source_fingerprint").is_none() })
                                .unwrap_or(p.get("source_fingerprint").is_none()))
                        && llm_blocks(&p.body)
                            .iter()
                            .any(|b| !citations_in(b).is_empty())
                })
                .unwrap_or(false);
        // 前の要約に引用があっても、今回のタスクで書き直していなければ未完了。
        // before のない旧 plan も引き続き読み込める。
        let done = done && plan.before.get(&task.page).is_none_or(|before| {
            Page::read(&path).is_ok_and(|p| llm_hash(&p.body) != *before)
        });
        // 人物ページは前の経緯に引用があるだけでは完了にならない (書き足されたかを見る)。
        let done = done
            && task.person.as_ref().is_none_or(|pt| {
                Page::read(&path).is_ok_and(|p| llm_hash(&p.body) != pt.before)
            });
        if let (true, Some(pt)) = (done, &task.person) {
            // 人物は、渡した最後の会議の日付まで要約済みとして記録する (翌日以降はその後の会議を渡す)。
            let mut page = Page::read(&path)?;
            page.set("llm_until", json!(pt.until));
            page.write(&path)?;
            completed.push(task.clone());
        } else if done && matches!(task.kind.as_str(), enforcements::KIND | hanrei::KIND) {
            let mut page = Page::read(&path)?;
            if task.kind == hanrei::KIND { hanrei_rag::annotate(wiki, &mut page)?; }
            page.set("llm_source_fingerprint", page.get("source_fingerprint").cloned().unwrap_or(Value::Null));
            page.write(&path)?;
            completed.push(task.clone());
        } else if done && task.kind == super::kanpo::KIND_KANPO {
            // 官報の項目は一度要約したら終わり (本文もページ内で固定する)。
            let mut page = Page::read(&path)?;
            page.set("llm_done", json!(true));
            page.write(&path)?;
            completed.push(task.clone());
        } else if done && task.kind == "pubcomment" {
            // パブコメは、要約した時点の結果公示日をページに記録する。
            let mut page = Page::read(&path)?;
            let result = page.get("result_date").cloned().unwrap_or(Value::Null);
            page.set("llm_result_date", result);
            page.write(&path)?;
            completed.push(task.clone());
        } else if done && task.kind == "bill" {
            // 議案は state ではなく、要約した時点の経過 (latest_date) をページに記録する。
            let mut page = Page::read(&path)?;
            let latest = page.get("latest_date").cloned().unwrap_or(Value::Null);
            page.set("llm_latest_date", latest);
            page.write(&path)?;
            completed.push(task.clone());
        } else if done {
            state.mark(&task.key, "linked", &today);
            // 経緯の形式を満たしたので、再投入の対象から外す。
            let mut page = Page::read(&path)?;
            page.set("llm_version", json!(NARRATIVE_VERSION));
            if matches!(task.kind.as_str(), KIND_KOKKAI | KIND_SHINGIKAI) && page.get("source_fingerprint").is_some() {
                page.set("llm_source_fingerprint", page.get("source_fingerprint").cloned().unwrap());
            }
            page.write(&path)?;
            completed.push(task.clone());
        } else {
            tracing::warn!("wiki-finalize: {} は未完了 — 次回に再試行", task.key);
            rolled_back += 1;
            let backup = args.work.join(format!("hanrei-backups/{}", task.id));
            if task.kind == hanrei::KIND && backup.join("page.md").exists() {
                std::fs::copy(backup.join("page.md"), &path)?;
                let snapshot = hanrei::snapshot_path(wiki, task.id.parse()?);
                if backup.join("source.json").exists() {
                    std::fs::copy(backup.join("source.json"), snapshot)?;
                } else { std::fs::remove_file(snapshot).ok(); }
            } else if task.created.contains(&task.page) {
                std::fs::remove_file(&path).ok();
            }
        }
        created_laws.extend(
            task.created
                .iter()
                .filter(|p| p.starts_with("laws/"))
                .cloned(),
        );
    }

    hanrei::finish(wiki, &completed)?;
    let intake_path = wiki.join(".lawpub/intake.json");
    if intake_path.exists() {
        let mut intake: Value = serde_json::from_slice(&std::fs::read(&intake_path)?)?;
        if let Some(entries) = intake["entries"].as_object_mut() {
            for task in &plan.tasks {
                if let Some(status) = entries.get_mut(&task.key) {
                    *status = json!(if completed.iter().any(|t| t.key == task.key) { "completed" } else { "summary_pending" });
                }
            }
        }
        std::fs::write(&intake_path, serde_json::to_vec_pretty(&intake)?)?;
    }

    // 2. 会議ページを集める。人物・会議体ページ用には、LLM がまだ要点を書いていない会議も
    //    名簿 (`.lawpub/roster`) から足す (コーパスの全会議が対象)。
    let mut meetings = load_meetings(wiki)?;
    let have_pages: BTreeSet<String> = meetings.iter().map(|m| m.page.clone()).collect();
    let (roster_meetings, roster_law_titles, roster_speakers) = load_roster_meetings(wiki, &have_pages);
    for m in &mut meetings {
        if let Some(speakers) = roster_speakers.get(&m.page) {
            m.speakers = speakers.clone();
        }
    }
    let meetings = meetings;
    let mut all_meetings = meetings.clone();
    all_meetings.extend(roster_meetings);
    all_meetings.sort_by(|a, b| b.date.cmp(&a.date).then_with(|| a.title.cmp(&b.title)));

    // 3. 法令ページの時系列 (会議・議案・公布/施行を 1 本の表に)。どの会議からも議案からも
    //    参照されず LLM 区間も空のまま、この plan で新規作成したページは消す。
    let bills = load_bills(wiki)?;
    // 時系列に載せる間接的な改正 (整理法など) は直近 3 年まで。
    let revision_cutoff = chrono::NaiveDate::parse_from_str(&today, "%Y-%m-%d")
        .map(|d| (d - chrono::Duration::days(365 * 3)).format("%Y-%m-%d").to_string())
        .unwrap_or_default();
    let pubcomments = load_pubcomments(wiki)?;
    let cases = hanrei::completed_pages(wiki)?;
    let mut law_titles: BTreeMap<String, String> = BTreeMap::new();
    for path in walk_md(wiki, "laws") {
        let rel = rel_path(wiki, &path);
        let mut page = Page::read(&path)?;
        let law_id = page.get_str("law_id").to_string();
        let law_meetings: Vec<&MeetingInfo> = meetings.iter().filter(|m| m.laws.contains(&law_id)).collect();
        let law_bills: Vec<&BillInfo> = bills.iter().filter(|b| b.laws.contains(&law_id)).collect();
        let law_pubcomments: Vec<&PubcommentInfo> = pubcomments.iter().filter(|p| p.laws.contains(&law_id)).collect();
        let has_cases = cases.iter().any(|(_, p)| hanrei::references_law(p, &law_id));
        let llm_empty = llm_blocks(&page.body).iter().all(|b| b.trim().is_empty());
        if law_meetings.is_empty() && law_bills.is_empty() && law_pubcomments.is_empty() && !has_cases && llm_empty && created_laws.contains(&rel) {
            std::fs::remove_file(&path)?;
            continue;
        }
        law_titles.insert(law_id.clone(), page.get_str("title").to_string());
        let law_title = page.get_str("title").to_string();

        // (日付, 並び順, 種別, 内容)。同じ日は 公布/施行 → 議案 → 会議 の順。
        let mut rows: Vec<(String, u8, &str, String)> = Vec::new();
        for m in &law_meetings {
            let kind = if m.corpus == KIND_SHINGIKAI { "審議会" } else { "国会" };
            let desc = if m.description.is_empty() { String::new() } else { format!(" — {}", m.description) };
            rows.push((m.date.clone(), 2, kind, format!("[{}]({}){desc}", m.title, rel_link(&rel, &m.page))));
        }
        let revision_nums: BTreeSet<&str> = page
            .get("revisions")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|r| r["law_num"].as_str())
            .collect();
        for b in &law_bills {
            // 改正履歴に同じ法律番号の公布があれば、議案側の「公布」は重ねて出さない。
            let promulgation_in_revisions =
                b.law_num_text.as_deref().is_some_and(|n| revision_nums.contains(n));
            for s in b.stages.iter().filter(|s| TIMELINE_STAGES.contains(&s.1.as_str())) {
                if s.1 == "promulgated" && promulgation_in_revisions {
                    continue;
                }
                rows.push((s.0.clone(), 1, "議案", format!("[{}]({}): {}", b.title, rel_link(&rel, &b.page), s.2)));
            }
        }
        // パブコメ: 意見募集の開始と結果公示 (意見数と、LLM の 1 行要約があれば添える)。
        for pc in &law_pubcomments {
            let link = format!("[{}]({})", pc.title, rel_link(&rel, &pc.page));
            if let Some(d) = &pc.reception_start {
                rows.push((d.clone(), 1, "パブコメ", format!("意見募集開始: {link}")));
            }
            if let Some(d) = &pc.result_date {
                let n = pc.opinion_count.map(|n| format!("（意見 {n} 件）")).unwrap_or_default();
                let summary = pc.summary.as_deref().map(|s| format!(" — {s}")).unwrap_or_default();
                rows.push((d.clone(), 1, "パブコメ", format!("結果公示: {link}{n}{summary}")));
            }
        }
        // 官報 (wiki-plan が frontmatter `kanpo` にためた項目)。改正対象の項目は、同じ日・同じ標題の
        // 公布の行があればその行に官報へのリンクを付け、無ければ (e-Gov 未反映・告示の改正) 行を足す。
        let kanpo: Vec<&Value> = page.get("kanpo").and_then(Value::as_array).into_iter().flatten().collect();
        let mut kanpo_used = vec![false; kanpo.len()];
        for r in page.get("revisions").and_then(Value::as_array).into_iter().flatten() {
            let (Some(date), Some(label)) = (r["date"].as_str(), r["label"].as_str()) else { continue };
            let kind = match r["kind"].as_str() {
                Some("promulgated") => "公布",
                Some("scheduled") => "施行予定",
                _ => "施行",
            };
            let kanpo_hit = (kind == "公布")
                .then(|| {
                    kanpo.iter().position(|k| {
                        k["relation"] == "amends"
                            && k["date"].as_str() == Some(date)
                            && k["title"].as_str().map(kanpo_linker::title::strip_agency).is_some_and(|t| !t.is_empty() && label.contains(t))
                    })
                })
                .flatten();
            let kanpo_link = kanpo_hit.map(|i| format!(" — [官報]({})", kanpo_target(wiki, &rel, kanpo[i]).0)).unwrap_or_default();
            // 改正法の法律番号で議案ページにつなぐ (国会審議 → 公布 → 施行)。
            let bill = r["law_num"]
                .as_str()
                .and_then(|n| bills.iter().find(|b| b.law_num_text.as_deref() == Some(n)))
                .map(|b| format!(" — [議案]({})", rel_link(&rel, &b.page)))
                .unwrap_or_default();
            // 関係法律の整理法などの間接的な改正は行が増えるだけなので、この法令を直接改める改正・
            // 議案ページにつながる改正・直近の改正だけを載せる。
            let direct = !law_title.is_empty() && label.contains(law_title.as_str());
            if !direct && bill.is_empty() && date < revision_cutoff.as_str() {
                continue;
            }
            if let Some(i) = kanpo_hit {
                kanpo_used[i] = true;
            }
            rows.push((date.to_string(), 0, kind, format!("{label}{bill}{kanpo_link}")));
        }
        for (k, _) in kanpo.iter().zip(&kanpo_used).filter(|(_, used)| !**used) {
            let (Some(date), Some(title)) = (k["date"].as_str(), k["title"].as_str()) else {
                continue;
            };
            let (target, summary) = kanpo_target(wiki, &rel, k);
            if target.is_empty() {
                continue;
            }
            let place = match k["page"].as_u64() {
                Some(p) => format!("{}・{p}ページ", k["issue"].as_str().unwrap_or("")),
                None => k["issue"].as_str().unwrap_or("").to_string(),
            };
            let note = if k["relation"] == "basis" { "。この法令に基づく告示等" } else { "" };
            let summary = summary.map(|s| format!(" — {s}")).unwrap_or_default();
            rows.push((date.to_string(), 0, "官報", format!("[{title}]({target})（{place}{note}）{summary}")));
        }
        rows.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)).then_with(|| a.3.cmp(&b.3)));
        let mut table = String::from("| 日付 | 種別 | 内容 |\n|---|---|---|\n");
        for (date, _, kind, text) in &rows {
            table.push_str(&format!("| {date} | {kind} | {} |\n", cell(text)));
        }
        // 自治体の対応 (例規): wiki-plan が週 1 回 frontmatter に書いた照合結果を描く。
        let reiki = super::reiki::render_section(page.get("reiki"));
        if let Some(body) = replace_block(&page.body, "reiki", &reiki) {
            page.body = body;
        } else if !reiki.is_empty() {
            let block = format!("<!-- lawpub:begin reiki -->\n{reiki}<!-- lawpub:end reiki -->\n\n");
            match page.body.find("## 関連する出来事").or_else(|| page.body.find("## 時系列")) {
                Some(i) => page.body.insert_str(i, &block),
                None => page.body.push_str(&format!("\n{block}")),
            }
        }
        if let Some(body) = replace_block(&page.body, "timeline", &table) {
            if body != page.body {
                page.body = body;
                page.set("timestamp", json!(now_rfc3339()));
                page.write(&path)?;
            }
        }
    }

    hanrei::write_backlinks(wiki, &cases)?;

    // 4. 人物ページ。国会は議事進行を除く発言者全員 (議員・政府側・職員)、審議会は官職者だけ
    //    (「森委員」のような姓だけの委員は同定できないので会議体ページに載せる)。
    let mut people: BTreeMap<String, Vec<Remark>> = BTreeMap::new();
    for m in &all_meetings {
        for sp in m.speakers.iter().filter(|sp| sp.role != "member") {
            people.entry(sp.name.clone()).or_default().push((m, sp));
        }
    }
    // 対象から外れた人物のページは、LLM の経緯が無ければ消す。
    for path in walk_md(wiki, "people") {
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        let narrative_empty = Page::read(&path).map(|p| llm_blocks(&p.body).iter().all(|b| b.trim().is_empty())).unwrap_or(true);
        if !people.keys().any(|n| file_safe(n) == stem) && narrative_empty {
            std::fs::remove_file(&path)?;
        }
    }
    for (name, remarks) in &people {
        let mut remarks = remarks.clone();
        remarks.sort_by(|a, b| b.0.date.cmp(&a.0.date));
        let rel = person_page(name);
        let path = wiki.join(&rel);
        let mut page = if path.exists() { Page::read(&path)? } else { Page::default() };
        let latest = remarks[0].1;
        let official = latest.role == "official";
        // 肩書き: 国会は役職 (無ければ会派)、審議会の官職者は所属府省。
        let affiliation = if official {
            remarks[0].0.organization.clone()
        } else {
            remarks.iter().find_map(|r| r.1.group.clone()).unwrap_or_default()
        };
        let position = remarks.iter().find_map(|r| r.1.position.clone()).unwrap_or_default();
        let role_line = [position.as_str(), affiliation.as_str()]
            .iter()
            .filter(|s| !s.is_empty())
            .copied()
            .collect::<Vec<_>>()
            .join("・");
        // description は LLM が書いた 1 行を優先し、無ければ肩書きにする。
        let existing = page.get_str("description").to_string();
        let description = if existing.is_empty() || existing == page.get_str("role") { role_line.clone() } else { existing };
        let mut table = String::from("| 日付 | 会議 | 言及した法令 |\n|---|---|---|\n");
        for (m, sp) in &remarks {
            let laws: Vec<String> = sp
                .laws
                .iter()
                .filter_map(|id| law_label(&rel, id, &law_titles, &roster_law_titles))
                .collect();
            table.push_str(&format!("| {} | {} | {} |\n", m.date, meeting_ref(&rel, m), laws.join("、")));
        }
        // 会派がわかる国会の発言者は、会派 (政党) ページへつなぐ。
        let intro = if official {
            "審議会の議事録の表記どおりのページです（同じ人が別の表記で載ることがあります）。".to_string()
        } else if affiliation.is_empty() {
            "国会会議録に基づくページです。".to_string()
        } else {
            format!(
                "国会会議録に基づくページです。会派: [{affiliation}]({})",
                rel_link(&rel, &party_page(&affiliation))
            )
        };
        // 要約前の発言: LLM (人物タスク) がまだ要約していない会議 (`llm_until` より後) だけ、法令に言及した
        // 発言の抜粋を原文のまま新しい順に載せる。要約済みの会議は「発言と経緯」の要約から発言へリンクする。
        let llm_until = page.get("llm_until").and_then(Value::as_str).unwrap_or("").to_string();
        let mut quotes = String::new();
        let unsummarized: Vec<&Remark> = remarks.iter().filter(|(m, _)| m.date.as_str() > llm_until.as_str()).collect();
        let quoted: Vec<&Remark> = unsummarized.iter().copied().filter(|(_, sp)| sp.excerpt.is_some()).take(MAX_PERSON_QUOTES).collect();
        if !quoted.is_empty() {
            let heading = if llm_until.is_empty() { "主な発言" } else { "要約前の発言" };
            quotes.push_str(&format!("## {heading}（抜粋・{}）\n\n", quoted.len()));
            for (m, sp) in &quoted {
                let excerpt = sp.excerpt.as_deref().unwrap_or_default();
                let source = match sp.excerpt_url.as_deref().filter(|u| !u.is_empty()) {
                    Some(url) => format!("[{}]({url})", cell(&m.title)),
                    None => meeting_ref(&rel, m),
                };
                // 引用は表のセルではないので `|` を escape しない。改行だけ畳む。
                let excerpt = excerpt.replace(['\n', '\r'], " ");
                quotes.push_str(&format!("**{}**　{source}\n\n> {excerpt}\n\n", m.date));
            }
            if unsummarized.len() > quoted.len() {
                quotes.push_str(&format!(
                    "ほかの発言は下の一覧から会議録をたどってください（全 {} 会議）。\n\n",
                    remarks.len()
                ));
            }
        }
        let tags = page.get("tags").cloned().unwrap_or(json!([]));
        let mut fm = vec![
            ("type".to_string(), json!("person")),
            ("title".to_string(), json!(name)),
            ("description".to_string(), json!(description)),
            ("affiliation".to_string(), json!(affiliation)),
            ("position".to_string(), json!(position)),
            ("role".to_string(), json!(role_line)),
            ("timestamp".to_string(), page.get("timestamp").cloned().unwrap_or(json!(now_rfc3339()))),
            ("tags".to_string(), tags),
        ];
        // 人物タスクで要約済みの会議の日付 (wiki-plan が次に渡す会議を決める)。
        if let Some(until) = page.get("llm_until").cloned() {
            fm.push(("llm_until".to_string(), until));
        }
        let (summary, narrative) = person_blocks(&page.body);
        let body = person_body(name, &intro, &summary, &narrative, &quotes, &table);
        if body != page.body {
            fm[6].1 = json!(now_rfc3339());
        }
        page.frontmatter = fm;
        page.body = body;
        page.write(&path)?;
    }

    let mut index_people: Vec<(String, usize)> =
        people.iter().map(|(name, remarks)| (name.clone(), remarks.len())).collect();
    index_people.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

    // 4b. 会議体ページ (委員会・審議会の部会など)。開催回・扱った法令・発言者をまとめる。
    let committees = write_committees(wiki, &all_meetings, &law_titles, &roster_law_titles, &people)?;

    // 4c. 会派 (政党) ページ。発言者の会派と、議案ページの「会派の賛否」をまとめる。
    let parties = write_parties(wiki, &all_meetings, &bills, &law_titles, &roster_law_titles, &people)?;

    // 5. index.md / log.md。
    write_index(
        wiki,
        &meetings,
        &committees,
        &parties,
        &bills,
        &pubcomments,
        &law_titles,
        &index_people,
    )?;
    if !completed.is_empty() {
        append_log(wiki, &today, &completed, &meetings)?;
    }
    state.save(wiki)?;
    println!(
        "wiki-finalize: {} completed, {rolled_back} rolled back, {} meeting page(s) (+{} from roster) / {} laws / {} people / {} committees / {} parties",
        completed.len(),
        meetings.len(),
        all_meetings.len() - meetings.len(),
        law_titles.len(),
        people.len(),
        committees.len(),
        parties.len()
    );
    Ok(())
}

/// 法令の表示: wiki に法令ページがあればリンク、無ければ名簿が持つ題名をそのまま出す。
fn law_label(
    from: &str,
    law_id: &str,
    law_titles: &BTreeMap<String, String>,
    roster_law_titles: &BTreeMap<String, String>,
) -> Option<String> {
    match law_titles.get(law_id) {
        Some(title) => Some(format!("[{title}]({})", rel_link(from, &law_page(law_id)))),
        None => roster_law_titles.get(law_id).cloned(),
    }
}

/// 会派 (政党) ページを丸ごと生成する。戻り値は (会派名, ページ, 発言した人数)。
///
/// 元になるのは、発言者名簿・会議ページが持つ発言者の会派と、議案ページの「会派の賛否」。
/// LLM は使わない。会派名は会議録・議案の表記のままにする (院や会期で変わるため、
/// 勝手に名寄せすると出典と食い違う)。
fn write_parties(
    wiki: &Path,
    meetings: &[MeetingInfo],
    bills: &[BillInfo],
    law_titles: &BTreeMap<String, String>,
    roster_law_titles: &BTreeMap<String, String>,
    people: &BTreeMap<String, Vec<Remark>>,
) -> Result<Vec<(String, String, usize)>> {
    // 会派 → (人物 → 発言した会議数)、会派 → 言及された法令の回数、会派 → 最新の発言日。
    let mut members: BTreeMap<String, BTreeMap<&str, usize>> = BTreeMap::new();
    // 法令 → 言及した人物の集合 (同じ人が何度言及しても 1 人と数える)。
    let mut law_people: BTreeMap<String, BTreeMap<&str, BTreeSet<&str>>> = BTreeMap::new();
    let mut latest: BTreeMap<String, String> = BTreeMap::new();
    for m in meetings {
        for sp in &m.speakers {
            let Some(group) = sp.group.as_deref().map(str::trim).filter(|g| !g.is_empty()) else {
                continue;
            };
            if !people.contains_key(&sp.name) {
                continue;
            }
            *members.entry(group.to_string()).or_default().entry(sp.name.as_str()).or_default() += 1;
            for id in &sp.laws {
                law_people
                    .entry(group.to_string())
                    .or_default()
                    .entry(id.as_str())
                    .or_default()
                    .insert(sp.name.as_str());
            }
            let e = latest.entry(group.to_string()).or_default();
            if m.date > *e {
                *e = m.date.clone();
            }
        }
    }
    // 議案の賛否・提出会派。発言が 1 件も無い会派 (院が違う等) もページにする。
    let mut votes: BTreeMap<String, Vec<(&BillInfo, &str, &str)>> = BTreeMap::new();
    let mut proposed: BTreeMap<String, Vec<&BillInfo>> = BTreeMap::new();
    for b in bills {
        for (house, side, group) in &b.votes {
            votes.entry(group.clone()).or_default().push((b, house.as_str(), side.as_str()));
        }
        for group in &b.proposer_groups {
            proposed.entry(group.clone()).or_default().push(b);
        }
    }

    let titles: BTreeSet<String> = members
        .keys()
        .chain(votes.keys())
        .chain(proposed.keys())
        .cloned()
        .collect();
    let keep: BTreeSet<String> = titles.iter().map(|t| party_page(t)).collect();
    for path in walk_md(wiki, "parties") {
        let narrative_empty = Page::read(&path).map(|p| first_llm_block(&p.body).is_empty()).unwrap_or(true);
        if !keep.contains(&rel_path(wiki, &path)) && narrative_empty {
            std::fs::remove_file(&path)?;
        }
    }

    let mut out = Vec::new();
    for title in &titles {
        let rel = party_page(title);
        let path = wiki.join(&rel);
        let mut body = String::new();

        let party_members = members.get(title);
        let member_count = party_members.map(BTreeMap::len).unwrap_or(0);
        if let Some(ms) = party_members {
            let mut rows: Vec<(&str, usize)> = ms.iter().map(|(n, c)| (*n, *c)).collect();
            rows.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));
            body.push_str(&format!("## この会派として発言した人物（{}）\n\n", rows.len()));
            for (name, n) in &rows {
                body.push_str(&format!("- [{name}]({})（{n} 会議）\n", rel_link(&rel, &person_page(name))));
            }
        }

        let party_votes = votes.get(title);
        if let Some(vs) = party_votes {
            let mut rows: Vec<&(&BillInfo, &str, &str)> = vs.iter().collect();
            rows.sort_by(|a, b| b.0.date.cmp(&a.0.date).then_with(|| a.0.title.cmp(&b.0.title)));
            body.push_str(&format!("\n## 議案への賛否（{}）\n\n| 日付 | 議案 | 院 | 賛否 |\n|---|---|---|---|\n", rows.len()));
            for (b, house, side) in rows {
                body.push_str(&format!(
                    "| {} | [{}]({}) | {house} | {side} |\n",
                    b.date,
                    cell(&b.title),
                    rel_link(&rel, &b.page)
                ));
            }
        }

        if let Some(ps) = proposed.get(title) {
            let mut rows: Vec<&&BillInfo> = ps.iter().collect();
            rows.sort_by(|a, b| b.date.cmp(&a.date).then_with(|| a.title.cmp(&b.title)));
            body.push_str(&format!("\n## 提出した議案（{}）\n\n", rows.len()));
            for b in rows {
                body.push_str(&format!("- {} [{}]({})\n", b.date, cell(&b.title), rel_link(&rel, &b.page)));
            }
        }

        if let Some(counts) = law_people.get(title) {
            let mut laws: Vec<(&str, usize)> = counts.iter().map(|(id, names)| (*id, names.len())).collect();
            laws.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));
            let rows: Vec<String> = laws
                .iter()
                .take(30)
                .filter_map(|(id, n)| law_label(&rel, id, law_titles, roster_law_titles).map(|l| format!("- {l}（{n} 人）\n")))
                .collect();
            if !rows.is_empty() {
                body.push_str("\n## よく言及した法令\n\n");
                body.extend(rows);
            }
        }

        let agree = party_votes.map(|vs| vs.iter().filter(|(_, _, side)| *side == "賛成").count()).unwrap_or(0);
        let against = party_votes.map(|vs| vs.iter().filter(|(_, _, side)| *side == "反対").count()).unwrap_or(0);
        let date = latest.get(title).cloned().unwrap_or_default();
        let stats = format!("発言した人物 {member_count} 人・議案に賛成 {agree} / 反対 {against}");

        let mut page = if path.exists() { Page::read(&path)? } else { Page::default() };
        let body = party_body(title, &first_llm_block(&page.body), &body);
        let existing = page.get_str("description").to_string();
        let description = if existing.is_empty() || existing == page.get_str("stats") { stats.clone() } else { existing };
        let changed = page.body != body;
        let timestamp = if changed || page.get("timestamp").is_none() {
            json!(now_rfc3339())
        } else {
            page.get("timestamp").cloned().unwrap_or(json!(now_rfc3339()))
        };
        page.frontmatter = vec![
            ("type".into(), json!("party")),
            ("title".into(), json!(title)),
            ("description".into(), json!(description)),
            ("stats".into(), json!(stats)),
            ("date".into(), json!(date)),
            ("timestamp".into(), timestamp),
            ("tags".into(), page.get("tags").cloned().unwrap_or(json!([]))),
        ];
        page.body = body;
        page.write(&path)?;
        out.push((title.clone(), rel, member_count));
    }
    Ok(out)
}

/// 会議体ページを丸ごと生成する。戻り値は (タイトル, ページ, 開催回数)。
fn write_committees(
    wiki: &Path,
    meetings: &[MeetingInfo],
    law_titles: &BTreeMap<String, String>,
    roster_law_titles: &BTreeMap<String, String>,
    people: &BTreeMap<String, Vec<Remark>>,
) -> Result<Vec<(String, String, usize)>> {
    let mut groups: BTreeMap<String, Vec<&MeetingInfo>> = BTreeMap::new();
    for m in meetings {
        if let Some(t) = committee_title(&m.corpus, &m.organization, &m.committee) {
            groups.entry(t).or_default().push(m);
        }
    }
    let keep: BTreeSet<String> = groups.keys().map(|t| committee_page(t)).collect();
    for path in walk_md(wiki, "committees") {
        let narrative_empty = Page::read(&path).map(|p| first_llm_block(&p.body).is_empty()).unwrap_or(true);
        if !keep.contains(&rel_path(wiki, &path)) && narrative_empty {
            std::fs::remove_file(&path)?;
        }
    }

    let mut out = Vec::new();
    for (title, ms) in &groups {
        let rel = committee_page(title);
        let path = wiki.join(&rel);
        let mut body = String::from("## 開催回（一覧）\n\n| 日付 | 会議 | 概要 |\n|---|---|---|\n");
        for m in ms {
            body.push_str(&format!("| {} | {} | {} |\n", m.date, meeting_ref(&rel, m), cell(&m.description)));
        }

        let mut law_counts: BTreeMap<&str, usize> = BTreeMap::new();
        for m in ms {
            for id in &m.laws {
                *law_counts.entry(id.as_str()).or_default() += 1;
            }
        }
        let mut laws: Vec<(&str, usize)> = law_counts
            .into_iter()
            .filter(|(id, _)| law_titles.contains_key(*id) || roster_law_titles.contains_key(*id))
            .collect();
        laws.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));
        if !laws.is_empty() {
            body.push_str("\n## 扱った法令\n\n");
            for (id, n) in &laws {
                let Some(label) = law_label(&rel, id, law_titles, roster_law_titles) else { continue };
                body.push_str(&format!("- {label}（{n} 回）\n"));
            }
        }

        // 発言者: 人物ページがある人はリンク、委員 (姓のみ) は表記と開催回へのリンク。
        let mut linked: BTreeSet<&str> = BTreeSet::new();
        let mut members: BTreeMap<&str, Vec<&MeetingInfo>> = BTreeMap::new();
        for m in ms {
            for sp in &m.speakers {
                if people.contains_key(&sp.name) {
                    linked.insert(sp.name.as_str());
                } else {
                    members.entry(sp.name.as_str()).or_default().push(m);
                }
            }
        }
        if !linked.is_empty() || !members.is_empty() {
            body.push_str("\n## 法令に言及した発言者\n\n");
            for name in &linked {
                body.push_str(&format!("- [{name}]({})\n", rel_link(&rel, &person_page(name))));
            }
            for (name, mms) in &members {
                let refs: Vec<String> = mms
                    .iter()
                    .map(|m| {
                        let target = if m.page.is_empty() { m.url.clone() } else { rel_link(&rel, &m.page) };
                        if target.is_empty() { m.date.clone() } else { format!("[{}]({target})", m.date) }
                    })
                    .collect();
                body.push_str(&format!("- {name}（{}）\n", refs.join("、")));
            }
        }
        let organization = ms[0].organization.clone();
        let latest = ms.iter().map(|m| m.date.as_str()).max().unwrap_or("");
        let summarized = ms.iter().filter(|m| !m.page.is_empty()).count();
        let stats = format!(
            "{organization}・収録 {} 回（うち要点あり {summarized} 回・最新 {latest}）",
            ms.len()
        );
        let mut page = if path.exists() { Page::read(&path)? } else { Page::default() };
        let body = committee_body(title, &first_llm_block(&page.body), &body);
        // description は LLM が書いた 1 行を優先し、無ければ開催状況にする。
        let existing = page.get_str("description").to_string();
        let description = if existing.is_empty() || existing == page.get_str("stats") { stats.clone() } else { existing };
        let changed = page.body != body;
        let timestamp = if changed || page.get("timestamp").is_none() {
            json!(now_rfc3339())
        } else {
            page.get("timestamp").cloned().unwrap_or(json!(now_rfc3339()))
        };
        page.frontmatter = vec![
            ("type".into(), json!("committee")),
            ("title".into(), json!(title)),
            ("description".into(), json!(description)),
            ("stats".into(), json!(stats)),
            ("organization".into(), json!(organization)),
            ("date".into(), json!(latest)),
            ("timestamp".into(), timestamp),
        ];
        page.body = body;
        page.write(&path)?;
        out.push((title.clone(), rel, ms.len()));
    }
    Ok(out)
}

fn load_meetings(wiki: &Path) -> Result<Vec<MeetingInfo>> {
    let mut out = Vec::new();
    for path in walk_md(wiki, "meetings") {
        let page = Page::read(&path)?;
        let laws = page
            .get("laws")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        let corpus = page.get_str("corpus").to_string();
        let speakers = page
            .get("speakers")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|v| {
                        let name = v["name"].as_str()?.to_string();
                        let default_role = if corpus == KIND_SHINGIKAI { "member" } else { "kokkai" };
                        Some(Speaker {
                            name,
                            group: v["group"].as_str().map(String::from),
                            position: v["position"].as_str().map(String::from),
                            role: v["role"].as_str().unwrap_or(default_role).to_string(),
                            laws: v["laws"]
                                .as_array()
                                .map(|l| l.iter().filter_map(|x| x.as_str().map(String::from)).collect())
                                .unwrap_or_default(),
                            // 抜粋は名簿だけが持つ (会議ページの frontmatter には書かない)。
                            excerpt: None,
                            excerpt_url: None,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        out.push(MeetingInfo {
            page: rel_path(wiki, &path),
            url: page.get_str("resource").to_string(),
            title: page.get_str("title").to_string(),
            date: page.get_str("date").to_string(),
            corpus,
            organization: page.get_str("organization").to_string(),
            committee: page.get_str("committee").to_string(),
            description: if evidence::summary_source_status(&page) == "stale" {
                format!("【原資料更新・要再確認】{}", page.get_str("description"))
            } else { page.get_str("description").to_string() },
            laws,
            speakers,
        });
    }
    out.sort_by(|a, b| b.date.cmp(&a.date).then_with(|| a.page.cmp(&b.page)));
    Ok(out)
}

/// 名簿 (`.lawpub/roster`) のうち、まだ会議ページが無い会議を `MeetingInfo` にする。
///
/// 人物・会議体ページは LLM が要点を書いた会議だけでなくコーパスの全会議を対象にするので、
/// 会議ページが無い回は原文 (国会会議録・議事録) へ直接つなぐ。
/// (会議ページの無い会議, 法令 ID → 題名, 会議ページ → 発言者全員)。
type RosterInput = (Vec<MeetingInfo>, BTreeMap<String, String>, BTreeMap<String, Vec<Speaker>>);

fn load_roster_meetings(wiki: &Path, have_pages: &BTreeSet<String>) -> RosterInput {
    let mut out = Vec::new();
    let mut law_titles = BTreeMap::new();
    let mut by_page: BTreeMap<String, Vec<Speaker>> = BTreeMap::new();
    for entry in super::roster::load(wiki) {
        for l in &entry.laws {
            law_titles.entry(l.law_id.clone()).or_insert_with(|| l.title.clone());
        }
        if entry.speakers.is_empty() {
            continue;
        }
        let speakers: Vec<Speaker> = entry
            .speakers
            .iter()
            .map(|sp| Speaker {
                name: sp.name.clone(),
                group: sp.group.clone(),
                position: sp.position.clone(),
                role: sp.role.clone(),
                laws: sp.laws.clone(),
                excerpt: sp.excerpt.clone(),
                excerpt_url: sp.excerpt_url.clone(),
            })
            .collect();
        let page = meeting_page(&entry.corpus, &entry.id);
        if have_pages.contains(&page) {
            // 会議ページの speakers は LLM 用の抜粋 (上限 12 件) から作るので抜けがある。
            // ページへのリンクはそのまま、発言者だけ名簿の全員で置き換える。
            by_page.insert(page, speakers);
            continue;
        }
        out.push(MeetingInfo {
            page: String::new(),
            url: entry.url.clone(),
            title: if entry.title.is_empty() { entry.id.clone() } else { entry.title.clone() },
            date: entry.date.clone(),
            corpus: entry.corpus.clone(),
            organization: entry.organization.clone(),
            committee: entry.committee.clone(),
            description: String::new(),
            laws: entry.laws.iter().map(|l| l.law_id.clone()).collect(),
            speakers,
        });
    }
    (out, law_titles, by_page)
}

/// 会議への markdown リンク。会議ページがあればそこへ、無ければ原文へ。
fn meeting_ref(from: &str, m: &MeetingInfo) -> String {
    let title = cell(&m.title);
    if m.page.is_empty() {
        if m.url.is_empty() {
            title
        } else {
            format!("[{title}]({})", m.url)
        }
    } else {
        format!("[{title}]({})", rel_link(from, &m.page))
    }
}

fn write_index(
    wiki: &Path,
    meetings: &[MeetingInfo],
    committees: &[(String, String, usize)],
    parties: &[(String, String, usize)],
    bills: &[BillInfo],
    pubcomments: &[PubcommentInfo],
    law_titles: &BTreeMap<String, String>,
    // (氏名, 発言した会議数)。発言の多い順。
    people: &[(String, usize)],
) -> Result<()> {
    let mut body = String::from(
        "\n# lawrenceanum wiki\n\n\
         国会会議録・審議会議事録のうち法令に言及した発言と、議案の審議経過・法令の公布・施行を、法令・会議・議案・人物・論点ごとに整理した wiki です。\n\
         正本は [lawrenceanum](https://github.com/bokuweb/lawrenceanum) の正規化コーパスで、本文の要約は LLM が書き、\
         すべての記述に会議録の発言 ID と原文引用を付けています（`lawpub wiki-check` で照合済み）。\n\n",
    );

    body.push_str("## 最近の会議\n\n| 日付 | 会議 | 概要 |\n|---|---|---|\n");
    for m in meetings.iter().take(30) {
        body.push_str(&format!(
            "| {} | [{}]({}) | {} |\n",
            m.date,
            cell(&m.title),
            m.page,
            cell(&m.description)
        ));
    }

    // 法令は直近の言及日が新しい順。
    let mut latest: BTreeMap<&str, &str> = BTreeMap::new();
    for m in meetings {
        for id in &m.laws {
            let e = latest.entry(id.as_str()).or_insert(m.date.as_str());
            if m.date.as_str() > *e {
                *e = m.date.as_str();
            }
        }
    }
    let mut laws: Vec<(&String, &String)> = law_titles.iter().collect();
    laws.sort_by(|a, b| {
        let da = latest.get(a.0.as_str()).copied().unwrap_or("");
        let db = latest.get(b.0.as_str()).copied().unwrap_or("");
        db.cmp(da).then_with(|| a.1.cmp(b.1))
    });
    body.push_str(&format!(
        "\n## 法令（{}）\n\n| 最終言及 | 法令 |\n|---|---|\n",
        laws.len()
    ));
    for (id, title) in laws {
        body.push_str(&format!(
            "| {} | [{}]({}) |\n",
            latest.get(id.as_str()).copied().unwrap_or(""),
            cell(title),
            law_page(id)
        ));
    }

    if !committees.is_empty() {
        body.push_str(&format!("\n## 会議体（{}）\n\n", committees.len()));
        let links: Vec<String> =
            committees.iter().map(|(title, page, n)| format!("[{title}]({page})（{n}）")).collect();
        body.push_str(&links.join(" · "));
        body.push('\n');
    }

    if !parties.is_empty() {
        body.push_str(&format!("\n## 会派（{}）\n\n", parties.len()));
        let mut rows: Vec<&(String, String, usize)> = parties.iter().collect();
        rows.sort_by(|a, b| b.2.cmp(&a.2).then_with(|| a.0.cmp(&b.0)));
        let links: Vec<String> =
            rows.iter().map(|(title, page, n)| format!("[{title}]({page})（{n}）")).collect();
        body.push_str(&links.join(" · "));
        body.push('\n');
    }

    if !pubcomments.is_empty() {
        let mut recent: Vec<&PubcommentInfo> = pubcomments.iter().collect();
        recent.sort_by(|a, b| b.date.cmp(&a.date));
        body.push_str(&format!("\n## 最近のパブコメ（{}）\n\n| 日付 | 案件 | 状況 |\n|---|---|---|\n", pubcomments.len()));
        for pc in recent.iter().take(20) {
            body.push_str(&format!("| {} | [{}]({}) | {} |\n", pc.date, cell(&pc.title), pc.page, cell(&pc.description)));
        }
    }

    let mut kanpo: Vec<(String, String, String, String)> = walk_md(wiki, "kanpo")
        .iter()
        .filter_map(|path| {
            let p = Page::read(path).ok()?;
            Some((p.get_str("date").to_string(), p.get_str("title").to_string(), rel_path(wiki, path), p.get_str("description").to_string()))
        })
        .collect();
    if !kanpo.is_empty() {
        kanpo.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.2.cmp(&b.2)));
        body.push_str(&format!("\n## 最近の官報（{}）\n\n| 日付 | 項目 | 内容 |\n|---|---|---|\n", kanpo.len()));
        for (date, title, page, description) in kanpo.iter().take(20) {
            body.push_str(&format!("| {date} | [{}]({page}) | {} |\n", cell(title), cell(description)));
        }
    }

    if !bills.is_empty() {
        body.push_str(&format!("\n## 最近の議案（{}）\n\n| 日付 | 議案 | 経過 |\n|---|---|---|\n", bills.len()));
        for b in bills.iter().take(30) {
            body.push_str(&format!("| {} | [{}]({}) | {} |\n", b.date, cell(&b.title), b.page, cell(&b.description)));
        }
    }

    let cases = hanrei::completed_pages(wiki)?;
    if !cases.is_empty() {
        body.push_str(&format!("\n## 最近取得した判例（{}）\n\n| 裁判日 | 判例 | 概要 |\n|---|---|---|\n", cases.len()));
        for (rel, p) in cases.iter().take(30) {
            body.push_str(&format!("| {} | [{}]({rel}) | {} |\n", cell(p.get_str("judged_on")), cell(p.get_str("title")), cell(p.get_str("description"))));
        }
    }

    let topics = walk_md(wiki, "topics");
    body.push_str(&format!("\n## 論点・キーワード（{}）\n\n", topics.len()));
    for path in topics {
        if let Ok(p) = Page::read(&path) {
            let rel = rel_path(wiki, &path);
            let desc = p.get_str("description");
            let sep = if desc.is_empty() { "" } else { " — " };
            body.push_str(&format!("- [{}]({rel}){sep}{desc}\n", p.get_str("title")));
        }
    }

    // 人物はコーパスの全会議から作るので数千件になりうる。index には発言の多い順に出し、
    // 残りは会議体ページ・法令ページから辿る。
    const INDEX_PEOPLE: usize = 200;
    body.push_str(&format!("\n## 人物（{}）\n\n", people.len()));
    let links: Vec<String> = people
        .iter()
        .take(INDEX_PEOPLE)
        .map(|(n, _)| format!("[{n}]({})", person_page(n)))
        .collect();
    body.push_str(&links.join(" · "));
    if people.len() > INDEX_PEOPLE {
        body.push_str(&format!(
            "\n\n発言の多い {INDEX_PEOPLE} 人を挙げています（全 {} 人）。ほかの人物は会議体ページからたどれます。",
            people.len()
        ));
    }
    body.push('\n');

    let page = Page {
        frontmatter: vec![
            ("type".into(), json!("index")),
            ("title".into(), json!("lawrenceanum wiki")),
            ("description".into(), json!("国会・審議会での法令への言及を、法令・会議・人物・論点ごとにたどれる OKF 形式の wiki")),
            ("resource".into(), json!("https://github.com/bokuweb/lawrenceanum")),
            ("timestamp".into(), json!(now_rfc3339())),
        ],
        body,
    };
    page.write(&wiki.join("index.md"))
}

fn append_log(
    wiki: &Path,
    today: &str,
    completed: &[Task],
    meetings: &[MeetingInfo],
) -> Result<()> {
    let path = wiki.join("log.md");
    let mut page = if path.exists() {
        Page::read(&path)?
    } else {
        Page {
            frontmatter: vec![
                ("type".into(), json!("log")),
                ("title".into(), json!("更新ログ")),
                ("description".into(), json!("wiki の日次更新の記録")),
            ],
            body: "\n# 更新ログ\n".into(),
        }
    };
    // 修正パス後に再実行されても同じ日の節を二重に足さない (当日の節は常に末尾)。
    let heading = format!("\n## {today}\n");
    if let Some(i) = page.body.find(&heading) {
        page.body.truncate(i);
    }
    let mut section = format!("{heading}\n");
    for t in completed {
        // 会議以外 (議案・パブコメ・官報) は、LLM が書いたページの description を添える。
        let desc = match meetings.iter().find(|m| m.page == t.page) {
            Some(m) => m.description.clone(),
            None => Page::read(&wiki.join(&t.page)).map(|p| p.get_str("description").to_string()).unwrap_or_default(),
        };
        section.push_str(&format!(
            "- {} [{}]({}) — {}\n",
            t.date, t.title, t.page, desc
        ));
    }
    page.body.push_str(&section);
    page.set("timestamp", json!(now_rfc3339()));
    page.write(&path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meeting(desc: &str, summary: &str) -> String {
        format!(
            "---\ntype: \"meeting\"\ntitle: \"参議院 厚生労働委員会 第1号\"\ndescription: {}\ndate: \"2026-10-01\"\ncorpus: \"kokkai\"\nlaws: [\"L1\"]\nspeakers: [{{\"name\": \"山田太郎\", \"group\": \"無所属\", \"laws\": [\"L1\"]}}]\n---\n# x\n\n{LLM_BEGIN}\n{summary}\n{LLM_END}\n",
            serde_json::to_string(desc).unwrap()
        )
    }

    fn law() -> &'static str {
        "---\ntype: \"law\"\ntitle: \"予防接種法\"\nlaw_id: \"L1\"\n---\n# 予防接種法\n\n<!-- llm:begin -->\n<!-- llm:end -->\n\n<!-- lawpub:begin timeline -->\n<!-- lawpub:end timeline -->\n"
    }

    fn setup(desc: &str, summary: &str) -> (PathBuf, PathBuf, PathBuf) {
        let root = temp_dir("finalize");
        let wiki = root.join("wiki");
        let work = root.join("work");
        std::fs::create_dir_all(wiki.join("meetings/kokkai")).unwrap();
        std::fs::create_dir_all(wiki.join("laws")).unwrap();
        std::fs::create_dir_all(&work).unwrap();
        std::fs::write(wiki.join("meetings/kokkai/M1.md"), meeting(desc, summary)).unwrap();
        std::fs::write(wiki.join("laws/L1.md"), law()).unwrap();
        let plan = Plan {
            schema_version: 1,
            before: BTreeMap::new(),
            date: "2026-10-08".into(),
            base_url: "x".into(),
            tasks: vec![Task {
                key: "kokkai:M1".into(),
                kind: "kokkai".into(),
                id: "M1".into(),
                date: "2026-10-01".into(),
                title: "参議院 厚生労働委員会 第1号".into(),
                page: "meetings/kokkai/M1.md".into(),
                source: "sources/kokkai_M1.json".into(),
                laws: vec![],
                people: vec![],
                committee: None,
                created: vec!["meetings/kokkai/M1.md".into(), "laws/L1.md".into()],
                person: None,
            }],
        };
        std::fs::write(work.join("plan.json"), serde_json::to_vec(&plan).unwrap()).unwrap();
        (root, wiki, work)
    }

    #[test]
    fn completed_task_builds_timeline_people_index_and_log() {
        let summary = "山田議員が救済拡充を求めた[^1]。\n\n[^1]: [kokkai:M1_001](https://kokkai.ndl.go.jp/txt/M1/1) 「副反応の救済を拡充すべき」";
        let (root, wiki, work) = setup("予防接種法の救済拡充を質疑", summary);
        run_finalize(&FinalizeArgs {
            wiki: wiki.clone(),
            work: work.clone(),
        })
        .unwrap();
        // 修正パス後の再実行でも結果は変わらない。
        run_finalize(&FinalizeArgs {
            wiki: wiki.clone(),
            work,
        })
        .unwrap();
        assert_eq!(
            std::fs::read_to_string(wiki.join("log.md"))
                .unwrap()
                .matches("## 2026-10-08")
                .count(),
            1
        );

        let law = std::fs::read_to_string(wiki.join("laws/L1.md")).unwrap();
        assert!(law.contains("| 2026-10-01 | 国会 | [参議院 厚生労働委員会 第1号](../meetings/kokkai/M1.md) — 予防接種法の救済拡充を質疑 |"));
        let person = Page::read(&wiki.join("people/山田太郎.md")).unwrap();
        assert_eq!(person.get_str("affiliation"), "無所属");
        assert!(person.body.contains("[予防接種法](../laws/L1.md)"));
        assert!(std::fs::read_to_string(wiki.join("index.md"))
            .unwrap()
            .contains("(laws/L1.md)"));
        assert!(std::fs::read_to_string(wiki.join("log.md"))
            .unwrap()
            .contains("## 2026-10-08"));
        assert_eq!(
            State::load(&wiki).unwrap().processed["kokkai:M1"].status,
            "linked"
        );
        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn roster_meetings_without_pages_still_produce_people_and_committees() {
        // 会議ページ (LLM の要点) が無い会議でも、名簿があれば人物・会議体ページに載る。
        let summary = "山田議員が救済拡充を求めた[^1]。\n\n[^1]: [kokkai:M1_001](https://kokkai.ndl.go.jp/txt/M1/1) 「副反応の救済を拡充すべき」";
        let (root, wiki, work) = setup("予防接種法の救済拡充を質疑", summary);
        let entry = super::super::roster::RosterMeeting {
            schema_version: super::super::roster::ROSTER_VERSION,
            corpus: KIND_KOKKAI.into(),
            id: "M9".into(),
            date: "2026-06-16".into(),
            title: "衆議院 厚生労働委員会 第13号".into(),
            organization: "衆議院".into(),
            committee: "厚生労働委員会".into(),
            url: "https://kokkai.ndl.go.jp/txt/M9".into(),
            laws: vec![
                super::super::roster::RosterLaw { law_id: "L1".into(), title: "予防接種法".into() },
                // wiki に法令ページが無い法令。題名は名簿のものをそのまま出す。
                super::super::roster::RosterLaw { law_id: "L2".into(), title: "感染症法".into() },
            ],
            speakers: vec![super::super::roster::RosterSpeaker {
                name: "田中花子".into(),
                group: Some("会派B".into()),
                position: None,
                role: "kokkai".into(),
                laws: vec!["L1".into(), "L2".into()],
                excerpt: Some("予防接種法の救済を拡充すべきでは。".into()),
                excerpt_url: Some("https://kokkai.ndl.go.jp/txt/M9/3".into()),
            }],
        };
        let dir = wiki.join(super::super::roster::ROSTER_DIR).join(KIND_KOKKAI);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("M9.json"), serde_json::to_vec(&entry).unwrap()).unwrap();

        // 会議ページのある M1 の名簿。抜粋に入らなかった発言者も人物ページになる。
        let m1 = super::super::roster::RosterMeeting {
            id: "M1".into(),
            date: "2026-10-01".into(),
            title: "参議院 厚生労働委員会 第1号".into(),
            committee: "厚生労働委員会".into(),
            organization: "参議院".into(),
            url: "https://kokkai.ndl.go.jp/txt/M1".into(),
            speakers: vec![
                super::super::roster::RosterSpeaker {
                    name: "山田太郎".into(),
                    group: Some("無所属".into()),
                    position: None,
                    role: "kokkai".into(),
                    laws: vec!["L1".into()],
                    excerpt: None,
                    excerpt_url: None,
                },
                super::super::roster::RosterSpeaker {
                    name: "佐藤大臣".into(),
                    group: None,
                    position: Some("厚生労働大臣".into()),
                    role: "kokkai".into(),
                    laws: vec!["L1".into()],
                    excerpt: None,
                    excerpt_url: None,
                },
            ],
            ..entry.clone()
        };
        std::fs::write(dir.join("M1.json"), serde_json::to_vec(&m1).unwrap()).unwrap();

        run_finalize(&FinalizeArgs { wiki: wiki.clone(), work: work.clone() }).unwrap();

        let person = Page::read(&wiki.join("people/田中花子.md")).unwrap();
        assert!(person.body.contains("## 主な発言（抜粋・1）"), "{}", person.body);

        // 人物タスクで要約済み (llm_until) の会議は、抜粋ではなく経緯の要約から発言へリンクする。
        let mut summarized = person.clone();
        summarized.set("llm_until", json!("2026-06-16"));
        summarized.write(&wiki.join("people/田中花子.md")).unwrap();
        run_finalize(&FinalizeArgs { wiki: wiki.clone(), work }).unwrap();
        let after = Page::read(&wiki.join("people/田中花子.md")).unwrap();
        assert!(!after.body.contains("予防接種法の救済を拡充すべきでは。"), "{}", after.body);
        assert!(after.body.contains("## 発言した会議（一覧）"));
        assert_eq!(person.get_str("affiliation"), "会派B");
        // 会議ページが無いので原文へ、法令ページがある L1 だけがリンクになる。
        assert!(person.body.contains("[衆議院 厚生労働委員会 第13号](https://kokkai.ndl.go.jp/txt/M9)"), "{}", person.body);
        assert!(person.body.contains("[予防接種法](../laws/L1.md)、感染症法"), "{}", person.body);
        // 発言の抜粋は原文のまま引用し、その発言の原文 URL につなぐ。
        assert!(
            person.body.contains("**2026-06-16**　[衆議院 厚生労働委員会 第13号](https://kokkai.ndl.go.jp/txt/M9/3)"),
            "{}",
            person.body
        );
        assert!(person.body.contains("> 予防接種法の救済を拡充すべきでは。"), "{}", person.body);
        assert!(after.get("llm_until").is_some(), "llm_until は描き直しでも残る");

        let committee = std::fs::read_to_string(wiki.join("committees/衆議院厚生労働委員会.md")).unwrap();
        assert!(committee.contains("[田中花子](../people/田中花子.md)"), "{committee}");
        assert!(committee.contains("うち要点あり 0 回"), "{committee}");

        // 会議ページのある会議は今までどおりページへリンクし、発言者は名簿の全員になる。
        let yamada = Page::read(&wiki.join("people/山田太郎.md")).unwrap();
        assert!(yamada.body.contains("(../meetings/kokkai/M1.md)"), "{}", yamada.body);
        let sato = Page::read(&wiki.join("people/佐藤大臣.md")).unwrap();
        assert_eq!(sato.get_str("position"), "厚生労働大臣");
        assert!(sato.body.contains("(../meetings/kokkai/M1.md)"), "{}", sato.body);
        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn party_pages_come_from_speaker_groups_and_bill_votes() {
        let summary = "質疑[^1]。\n\n[^1]: [kokkai:M1_001](https://kokkai.ndl.go.jp/txt/M1/1) 「副反応の救済を拡充すべき」";
        let (root, wiki, work) = setup("予防接種法の救済拡充を質疑", summary);
        std::fs::create_dir_all(wiki.join("bills/221")).unwrap();
        std::fs::write(
            wiki.join("bills/221/B1.md"),
            "---\ntype: \"bill\"\ntitle: \"予防接種法の一部を改正する法律案\"\ndate: \"2026-11-20\"\nlaws: [\"L1\"]\nstages: []\nvotes: [{\"house\": \"衆議院\", \"side\": \"賛成\", \"groups\": [\"無所属\", \"会派B\"]}, {\"house\": \"衆議院\", \"side\": \"反対\", \"groups\": [\"会派C\"]}]\nproposer_groups: [\"会派B\"]\n---\n# 議案\n",
        )
        .unwrap();
        run_finalize(&FinalizeArgs { wiki: wiki.clone(), work }).unwrap();

        // 発言者の会派 (山田太郎 = 無所属) と、議案の賛否に出てくる会派の両方がページになる。
        let muzokusei = std::fs::read_to_string(wiki.join("parties/無所属.md")).unwrap();
        assert!(muzokusei.contains("type: \"party\""), "{muzokusei}");
        assert!(muzokusei.contains("[山田太郎](../people/山田太郎.md)（1 会議）"), "{muzokusei}");
        assert!(
            muzokusei.contains("| 2026-11-20 | [予防接種法の一部を改正する法律案](../bills/221/B1.md) | 衆議院 | 賛成 |"),
            "{muzokusei}"
        );
        assert!(muzokusei.contains("[予防接種法](../laws/L1.md)（1 人）"), "{muzokusei}");

        // 発言が無くても、賛否・提出に出てくる会派はページになる。
        let b = std::fs::read_to_string(wiki.join("parties/会派B.md")).unwrap();
        assert!(b.contains("## 提出した議案（1）"), "{b}");
        let c = std::fs::read_to_string(wiki.join("parties/会派C.md")).unwrap();
        assert!(c.contains("| 衆議院 | 反対 |"), "{c}");

        // 人物ページから会派ページへつながる。
        let person = std::fs::read_to_string(wiki.join("people/山田太郎.md")).unwrap();
        assert!(person.contains("会派: [無所属](../parties/無所属.md)"), "{person}");
        // index にも会派の節が出る。
        assert!(std::fs::read_to_string(wiki.join("index.md")).unwrap().contains("## 会派（3）"));
        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn law_timeline_merges_meetings_bills_and_revisions() {
        let summary = "質疑[^1]。\n\n[^1]: [kokkai:M1_001](https://kokkai.ndl.go.jp/txt/M1/1) 「副反応の救済を拡充すべき」";
        let (root, wiki, work) = setup("予防接種法の救済拡充を質疑", summary);
        std::fs::write(
            wiki.join("laws/L1.md"),
            law().replace(
                "law_id: \"L1\"\n",
                "law_id: \"L1\"\nrevisions: [{\"date\":\"2026-11-20\",\"kind\":\"promulgated\",\"label\":\"公布: 予防接種法の一部を改正する法律（令和八年法律第九十号）\",\"law_num\":\"令和八年法律第九十号\"},{\"date\":\"2019-06-01\",\"kind\":\"enforced\",\"label\":\"施行: 関係法律の整理等に関する法律（令和元年法律第一号）\",\"law_num\":\"令和元年法律第一号\"}]\n",
            ),
        )
        .unwrap();
        std::fs::create_dir_all(wiki.join("bills/221")).unwrap();
        std::fs::write(
            wiki.join("bills/221/B1.md"),
            "---\ntype: \"bill\"\ntitle: \"予防接種法の一部を改正する法律案\"\ndate: \"2026-11-20\"\nlaw_num_text: \"令和八年法律第九十号\"\nlaws: [\"L1\"]\nstages: [{\"date\":\"2026-10-20\",\"kind\":\"received\",\"label\":\"衆議院で受理\"},{\"date\":\"2026-10-25\",\"kind\":\"referred\",\"label\":\"衆議院 厚生労働委員会に付託\"},{\"date\":\"2026-11-10\",\"kind\":\"plenary\",\"label\":\"参議院 本会議で可決\"}]\n---\n# 議案\n",
        )
        .unwrap();
        run_finalize(&FinalizeArgs { wiki: wiki.clone(), work }).unwrap();

        let law = std::fs::read_to_string(wiki.join("laws/L1.md")).unwrap();
        let rows: Vec<&str> = law.lines().filter(|l| l.starts_with("| 2026-")).collect();
        assert_eq!(
            rows,
            vec![
                "| 2026-11-20 | 公布 | 公布: 予防接種法の一部を改正する法律（令和八年法律第九十号） — [議案](../bills/221/B1.md) |",
                "| 2026-11-10 | 議案 | [予防接種法の一部を改正する法律案](../bills/221/B1.md): 参議院 本会議で可決 |",
                "| 2026-10-20 | 議案 | [予防接種法の一部を改正する法律案](../bills/221/B1.md): 衆議院で受理 |",
                "| 2026-10-01 | 国会 | [参議院 厚生労働委員会 第1号](../meetings/kokkai/M1.md) — 予防接種法の救済拡充を質疑 |",
            ],
            "付託は議案ページだけに載せる"
        );
        assert!(std::fs::read_to_string(wiki.join("index.md")).unwrap().contains("## 最近の議案（1）"));
        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn law_timeline_links_kanpo_to_promulgation_or_adds_kanpo_rows() {
        let summary = "質疑[^1]。\n\n[^1]: [kokkai:M1_001](https://kokkai.ndl.go.jp/txt/M1/1) 「副反応の救済を拡充すべき」";
        let (root, wiki, work) = setup("予防接種法の救済拡充を質疑", summary);
        std::fs::write(
            wiki.join("laws/L1.md"),
            law().replace(
                "law_id: \"L1\"\n",
                concat!(
                    "law_id: \"L1\"\n",
                    "revisions: [{\"date\":\"2026-09-01\",\"kind\":\"promulgated\",\"label\":\"公布: 予防接種法施行令の一部を改正する政令（令和八年政令第三百号）\",\"law_num\":\"令和八年政令第三百号\"}]\n",
                    "kanpo: [",
                    "{\"date\":\"2026-10-09\",\"issue\":\"号外第223号\",\"page\":2,\"title\":\"予防接種法の一部を改正する法律の一部の施行期日を定める政令（三二二）\",\"pdf_url\":\"https://k/p2.pdf\",\"relation\":\"amends\"},",
                    "{\"date\":\"2026-10-05\",\"issue\":\"本紙第1600号\",\"page\":4,\"title\":\"予防接種法第五条の規定に基づき、定期接種の対象を定める件の一部を改正する件（厚生労働三〇〇）\",\"pdf_url\":\"https://k/p4.pdf\",\"relation\":\"basis\"},",
                    "{\"date\":\"2026-09-01\",\"issue\":\"号外第190号\",\"page\":1,\"title\":\"予防接種法施行令の一部を改正する政令（三〇〇）\",\"pdf_url\":\"https://k/p1.pdf\",\"relation\":\"amends\"}",
                    "]\n",
                ),
            ),
        )
        .unwrap();
        // 施行期日の政令には項目ページがあり、LLM が 1 行を書いている。
        std::fs::create_dir_all(wiki.join("kanpo/2026-10-09")).unwrap();
        std::fs::write(
            wiki.join("kanpo/2026-10-09/g223-p0002-aaaa0000.md"),
            "---\ntype: \"kanpo\"\ntitle: \"施行期日を定める政令\"\ndescription: \"改正法の残りを 2027-04-01 に施行\"\nstats: \"2026-10-09 号外第223号・2ページ（改め文）\"\ndate: \"2026-10-09\"\n---\n# x\n",
        )
        .unwrap();
        let law = std::fs::read_to_string(wiki.join("laws/L1.md")).unwrap().replace(
            "\"pdf_url\":\"https://k/p2.pdf\",",
            "\"pdf_url\":\"https://k/p2.pdf\",\"item_page\":\"kanpo/2026-10-09/g223-p0002-aaaa0000.md\",",
        );
        std::fs::write(wiki.join("laws/L1.md"), law).unwrap();
        run_finalize(&FinalizeArgs { wiki: wiki.clone(), work }).unwrap();

        let law = std::fs::read_to_string(wiki.join("laws/L1.md")).unwrap();
        let rows: Vec<&str> = law.lines().filter(|l| l.starts_with("| 2026-")).collect();
        assert_eq!(
            rows,
            vec![
                "| 2026-10-09 | 官報 | [予防接種法の一部を改正する法律の一部の施行期日を定める政令（三二二）](../kanpo/2026-10-09/g223-p0002-aaaa0000.md)（号外第223号・2ページ） — 改正法の残りを 2027-04-01 に施行 |",
                "| 2026-10-05 | 官報 | [予防接種法第五条の規定に基づき、定期接種の対象を定める件の一部を改正する件（厚生労働三〇〇）](https://k/p4.pdf)（本紙第1600号・4ページ。この法令に基づく告示等） |",
                "| 2026-10-01 | 国会 | [参議院 厚生労働委員会 第1号](../meetings/kokkai/M1.md) — 予防接種法の救済拡充を質疑 |",
                "| 2026-09-01 | 公布 | 公布: 予防接種法施行令の一部を改正する政令（令和八年政令第三百号） — [官報](https://k/p1.pdf) |",
            ],
            "e-Gov に反映済みの公布は行を重ねずに官報へつなぎ、項目ページがあればそちらを指す"
        );
        assert!(std::fs::read_to_string(wiki.join("index.md")).unwrap().contains("## 最近の官報（1）"));
        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn officials_get_person_pages_and_members_go_to_committee_page() {
        let summary = "公示案の了承を求めた[^1]。\n\n[^1]: [shingikai:S1#2](https://www.mhlw.go.jp/s1) 「結果案の了承を求めたい」";
        let (root, wiki, work) = setup("形式的言及: 経費の説明", "質疑[^1]。\n\n[^1]: [kokkai:M1_001](https://kokkai.ndl.go.jp/txt/M1/1) 「副反応の救済を拡充すべき」");
        std::fs::create_dir_all(wiki.join("meetings/shingikai")).unwrap();
        std::fs::write(
            wiki.join("meetings/shingikai/S1.md"),
            format!(
                "---\ntype: \"meeting\"\ntitle: \"指定難病検討委員会 第68回\"\ndescription: \"公示案を了承\"\ndate: \"2026-09-01\"\ncorpus: \"shingikai\"\norganization: \"厚生労働省\"\ncommittee: \"疾病対策部会指定難病検討委員会\"\nlaws: [\"L1\"]\nspeakers: [{{\"name\": \"森光健康・生活衛生局長\", \"role\": \"official\", \"laws\": [\"L1\"]}}, {{\"name\": \"森委員\", \"role\": \"member\", \"laws\": [\"L1\"]}}]\n---\n# x\n\n{LLM_BEGIN}\n{summary}\n{LLM_END}\n"
            ),
        )
        .unwrap();
        run_finalize(&FinalizeArgs { wiki: wiki.clone(), work }).unwrap();

        // 形式的言及の会議でも、議事進行でない発言者 (plan で除外済み) は人物ページになる。
        assert!(wiki.join("people/山田太郎.md").exists());
        let official = Page::read(&wiki.join("people/森光健康・生活衛生局長.md")).unwrap();
        assert_eq!(official.get_str("affiliation"), "厚生労働省");
        assert!(!wiki.join("people/森委員.md").exists(), "姓だけの委員は人物ページにしない");

        let committee = std::fs::read_to_string(wiki.join("committees/疾病対策部会指定難病検討委員会.md")).unwrap();
        assert!(committee.contains("type: \"committee\""));
        assert!(committee.contains("| 2026-09-01 | [指定難病検討委員会 第68回](../meetings/shingikai/S1.md) | 公示案を了承 |"));
        assert!(committee.contains("- [予防接種法](../laws/L1.md)（1 回）"));
        assert!(committee.contains("- [森光健康・生活衛生局長](../people/森光健康・生活衛生局長.md)"));
        assert!(committee.contains("- 森委員（[2026-09-01](../meetings/shingikai/S1.md)）"));
        assert!(std::fs::read_to_string(wiki.join("index.md")).unwrap().contains("## 会議体（"));
        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn narratives_on_person_and_committee_pages_survive_regeneration() {
        let summary = "質疑[^1]。\n\n[^1]: [kokkai:M1_001](https://kokkai.ndl.go.jp/txt/M1/1) 「副反応の救済を拡充すべき」";
        let (root, wiki, work) = setup("救済拡充を質疑", summary);
        let meeting = std::fs::read_to_string(wiki.join("meetings/kokkai/M1.md"))
            .unwrap()
            .replace("corpus: \"kokkai\"\n", "corpus: \"kokkai\"\norganization: \"参議院\"\ncommittee: \"厚生労働委員会\"\n");
        std::fs::write(wiki.join("meetings/kokkai/M1.md"), meeting).unwrap();
        let narrative = "### 2026-10-01 参議院 厚生労働委員会\n- 副反応の救済拡充を求めた[^1]。\n\n[^1]: [kokkai:M1_001](https://kokkai.ndl.go.jp/txt/M1/1) 「副反応の救済を拡充すべき」";
        std::fs::create_dir_all(wiki.join("people")).unwrap();
        // 全体像の区間が無い旧形式のページ (唯一の LLM 区間が経緯)。
        std::fs::write(
            wiki.join("people/山田太郎.md"),
            format!("---\ntype: \"person\"\ntitle: \"山田太郎\"\ndescription: \"予防接種の救済拡充を追及\"\n---\n\n# 山田太郎\n\n## 発言と経緯\n\n{LLM_BEGIN}\n{narrative}\n{LLM_END}\n"),
        )
        .unwrap();
        run_finalize(&FinalizeArgs { wiki: wiki.clone(), work: work.clone() }).unwrap();
        run_finalize(&FinalizeArgs { wiki: wiki.clone(), work }).unwrap();

        let person = Page::read(&wiki.join("people/山田太郎.md")).unwrap();
        assert_eq!(person_blocks(&person.body), (String::new(), narrative.to_string()), "LLM の経緯は描き直しでも残る");
        assert!(person.body.contains(PERSON_SUMMARY_HEADING), "全体像の区間を足す");
        assert_eq!(person.get_str("description"), "予防接種の救済拡充を追及", "LLM の 1 行を優先");
        assert!(person.body.contains("## 発言した会議（一覧）"));
        assert!(person.body.contains("[参議院 厚生労働委員会 第1号](../meetings/kokkai/M1.md)"));

        let committee = Page::read(&wiki.join("committees/参議院厚生労働委員会.md")).unwrap();
        assert!(committee.body.contains("## 審議の経緯"));
        assert_eq!(committee.get_str("description"), committee.get_str("stats"));
        let meeting = Page::read(&wiki.join("meetings/kokkai/M1.md")).unwrap();
        assert_eq!(meeting.get("llm_version"), Some(&json!(NARRATIVE_VERSION)));
        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn person_task_completes_only_when_the_page_was_written() {
        let quote = "[^1]: [kokkai:M1_001](https://kokkai.ndl.go.jp/txt/M1/1) 「副反応の救済を拡充すべき」";
        let (root, wiki, work) = setup("救済拡充を質疑", &format!("質疑[^1]。\n\n{quote}"));
        let old = format!("### 2026-10-01 参議院 厚生労働委員会\n- 救済拡充を求めた[^1]。\n\n{quote}");
        let write_person = |summary: &str| {
            std::fs::create_dir_all(wiki.join("people")).unwrap();
            std::fs::write(
                wiki.join("people/山田太郎.md"),
                format!("---\ntype: \"person\"\ntitle: \"山田太郎\"\ndescription: \"予防接種の救済拡充を追及\"\n---\n{}", person_body("山田太郎", "", summary, &old, "", "")),
            )
            .unwrap();
        };
        write_person("");
        let before = llm_hash(&std::fs::read_to_string(wiki.join("people/山田太郎.md")).unwrap());
        let mut plan = Plan::load(&work).unwrap().unwrap();
        plan.tasks.push(Task {
            key: "person:山田太郎".into(),
            kind: "person".into(),
            id: "山田太郎".into(),
            date: "2026-10-01".into(),
            title: "山田太郎".into(),
            page: "people/山田太郎.md".into(),
            source: "sources/person_山田太郎.json".into(),
            laws: vec![],
            people: vec![],
            committee: None,
            created: vec![],
            person: Some(PersonTask { until: "2026-10-01".into(), before }),
        });
        std::fs::write(work.join("plan.json"), serde_json::to_vec(&plan).unwrap()).unwrap();

        // 前の経緯に引用があっても、書き足していなければ未完了。
        run_finalize(&FinalizeArgs { wiki: wiki.clone(), work: work.clone() }).unwrap();
        assert_eq!(Page::read(&wiki.join("people/山田太郎.md")).unwrap().get("llm_until"), None);

        // 全体像を書いたら完了し、llm_until が進む (描き直しでも残る)。
        write_person("- **救済制度**: 副反応の救済拡充を求めた[^1]。");
        run_finalize(&FinalizeArgs { wiki: wiki.clone(), work: work.clone() }).unwrap();
        let person = Page::read(&wiki.join("people/山田太郎.md")).unwrap();
        assert_eq!(person.get("llm_until"), Some(&json!("2026-10-01")));
        assert_eq!(person_blocks(&person.body).0, "- **救済制度**: 副反応の救済拡充を求めた[^1]。");
        assert!(std::fs::read_to_string(wiki.join("log.md")).unwrap().contains("山田太郎"));
        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn unchanged_summaries_do_not_advance_meeting_bill_or_pubcomment_state() {
        let summary = "救済拡充を求めた[^1]。\n\n[^1]: [kokkai:M1_001](https://kokkai.ndl.go.jp/txt/M1/1) 「副反応の救済を拡充すべき」";
        for kind in ["kokkai", "bill", "pubcomment"] {
            let (root, wiki, work) = setup("救済拡充を質疑", summary);
            let mut plan = Plan::load(&work).unwrap().unwrap();
            let task = &mut plan.tasks[0];
            task.kind = kind.into();
            task.created.clear(); // 以前の要約がある既存ページ。
            let old_path = wiki.join(&task.page);
            let mut page = Page::read(&old_path).unwrap();
            if kind != "kokkai" {
                task.page = format!("{kind}s/test.md");
                page.set("type", json!(kind));
                std::fs::create_dir_all(wiki.join(format!("{kind}s"))).unwrap();
                std::fs::remove_file(&old_path).unwrap();
            }
            let path = wiki.join(&task.page);
            page.set("latest_date", json!("2026-10-08"));
            page.set("llm_latest_date", json!("2026-10-01"));
            page.set("result_date", json!("2026-10-08"));
            page.set("llm_result_date", json!("2026-10-01"));
            plan.before.insert(task.page.clone(), llm_hash(&page.body));
            plan.schema_version = 2;
            // メタデータだけ変わっても、本文を読んで更新したことにはならない。
            page.set("description", json!("更新された説明文"));
            page.body.push_str("\n<!-- lawpub:begin meta -->\n更新された機械データ\n<!-- lawpub:end meta -->\n");
            page.write(&path).unwrap();
            std::fs::write(work.join("plan.json"), serde_json::to_vec(&plan).unwrap()).unwrap();
            let args = FinalizeArgs { wiki: wiki.clone(), work: work.clone() };
            run_finalize(&args).unwrap();
            let page = Page::read(&path).unwrap();
            assert!(!State::load(&wiki).unwrap().is_processed("kokkai:M1"), "{kind}");
            assert_eq!(page.get_str("llm_latest_date"), "2026-10-01", "{kind}");
            assert_eq!(page.get_str("llm_result_date"), "2026-10-01", "{kind}");

            let mut page = page;
            page.body = page.body.replace("救済拡充を求めた[^1]", "山田議員が救済拡充を求めた[^1]");
            page.write(&path).unwrap();
            run_finalize(&args).unwrap();
            let page = Page::read(&path).unwrap();
            match kind {
                "bill" => assert_eq!(page.get_str("llm_latest_date"), "2026-10-08"),
                "pubcomment" => assert_eq!(page.get_str("llm_result_date"), "2026-10-08"),
                _ => assert!(State::load(&wiki).unwrap().is_processed("kokkai:M1")),
            }
            std::fs::remove_dir_all(root).ok();
        }
    }

    #[test]
    fn incomplete_task_is_rolled_back_for_retry() {
        let (root, wiki, work) = setup("", "");
        run_finalize(&FinalizeArgs {
            wiki: wiki.clone(),
            work,
        })
        .unwrap();
        assert!(!wiki.join("meetings/kokkai/M1.md").exists());
        assert!(
            !wiki.join("laws/L1.md").exists(),
            "孤立した新規法令ページも消す"
        );
        assert!(!State::load(&wiki).unwrap().is_processed("kokkai:M1"));
        std::fs::remove_dir_all(root).ok();
    }
}
