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
        out.push(BillInfo {
            page: rel_path(wiki, &path),
            title: page.get_str("title").to_string(),
            date: page.get_str("date").to_string(),
            description: page.get_str("description").to_string(),
            law_num_text: page.get("law_num_text").and_then(Value::as_str).map(String::from),
            laws: strs("laws"),
            stages,
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
}

#[derive(Debug, Clone)]
struct MeetingInfo {
    page: String,
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
                        && llm_blocks(&p.body)
                            .iter()
                            .any(|b| !citations_in(b).is_empty())
                })
                .unwrap_or(false);
        if done && task.kind == "pubcomment" {
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
            page.write(&path)?;
            completed.push(task.clone());
        } else {
            tracing::warn!("wiki-finalize: {} は未完了 — 次回に再試行", task.key);
            rolled_back += 1;
            if task.created.contains(&task.page) {
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

    // 2. 会議ページを集める。
    let meetings = load_meetings(wiki)?;

    // 3. 法令ページの時系列 (会議・議案・公布/施行を 1 本の表に)。どの会議からも議案からも
    //    参照されず LLM 区間も空のまま、この plan で新規作成したページは消す。
    let bills = load_bills(wiki)?;
    let pubcomments = load_pubcomments(wiki)?;
    let mut law_titles: BTreeMap<String, String> = BTreeMap::new();
    for path in walk_md(wiki, "laws") {
        let rel = rel_path(wiki, &path);
        let mut page = Page::read(&path)?;
        let law_id = page.get_str("law_id").to_string();
        let law_meetings: Vec<&MeetingInfo> = meetings.iter().filter(|m| m.laws.contains(&law_id)).collect();
        let law_bills: Vec<&BillInfo> = bills.iter().filter(|b| b.laws.contains(&law_id)).collect();
        let law_pubcomments: Vec<&PubcommentInfo> = pubcomments.iter().filter(|p| p.laws.contains(&law_id)).collect();
        let llm_empty = llm_blocks(&page.body).iter().all(|b| b.trim().is_empty());
        if law_meetings.is_empty() && law_bills.is_empty() && law_pubcomments.is_empty() && llm_empty && created_laws.contains(&rel) {
            std::fs::remove_file(&path)?;
            continue;
        }
        law_titles.insert(law_id.clone(), page.get_str("title").to_string());

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
        for r in page.get("revisions").and_then(Value::as_array).into_iter().flatten() {
            let (Some(date), Some(label)) = (r["date"].as_str(), r["label"].as_str()) else { continue };
            let kind = match r["kind"].as_str() {
                Some("promulgated") => "公布",
                Some("scheduled") => "施行予定",
                _ => "施行",
            };
            // 改正法の法律番号で議案ページにつなぐ (国会審議 → 公布 → 施行)。
            let bill = r["law_num"]
                .as_str()
                .and_then(|n| bills.iter().find(|b| b.law_num_text.as_deref() == Some(n)))
                .map(|b| format!(" — [議案]({})", rel_link(&rel, &b.page)))
                .unwrap_or_default();
            rows.push((date.to_string(), 0, kind, format!("{label}{bill}")));
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

    // 4. 人物ページ。国会は議事進行を除く発言者全員 (議員・政府側・職員)、審議会は官職者だけ
    //    (「森委員」のような姓だけの委員は同定できないので会議体ページに載せる)。
    let mut people: BTreeMap<String, Vec<Remark>> = BTreeMap::new();
    for m in &meetings {
        for sp in m.speakers.iter().filter(|sp| sp.role != "member") {
            people.entry(sp.name.clone()).or_default().push((m, sp));
        }
    }
    // 対象から外れた人物のページは、LLM の経緯が無ければ消す。
    for path in walk_md(wiki, "people") {
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        let narrative_empty = Page::read(&path).map(|p| first_llm_block(&p.body).is_empty()).unwrap_or(true);
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
                .filter_map(|id| law_titles.get(id).map(|t| format!("[{}]({})", t, rel_link(&rel, &law_page(id)))))
                .collect();
            table.push_str(&format!("| {} | [{}]({}) | {} |\n", m.date, cell(&m.title), rel_link(&rel, &m.page), laws.join("、")));
        }
        let intro = if official {
            "審議会の議事録の表記どおりのページです（同じ人が別の表記で載ることがあります）。"
        } else {
            "国会会議録に基づくページです。"
        };
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
        let body = person_body(name, intro, &first_llm_block(&page.body), &table);
        if body != page.body {
            fm[6].1 = json!(now_rfc3339());
        }
        page.frontmatter = fm;
        page.body = body;
        page.write(&path)?;
    }

    // 4b. 会議体ページ (委員会・審議会の部会など)。開催回・扱った法令・発言者をまとめる。
    let committees = write_committees(wiki, &meetings, &law_titles, &people)?;

    // 5. index.md / log.md。
    write_index(
        wiki,
        &meetings,
        &committees,
        &bills,
        &pubcomments,
        &law_titles,
        &people.keys().cloned().collect::<Vec<_>>(),
    )?;
    if !completed.is_empty() {
        append_log(wiki, &today, &completed, &meetings)?;
    }
    state.save(wiki)?;
    println!(
        "wiki-finalize: {} completed, {rolled_back} rolled back, {} meetings / {} laws / {} people",
        completed.len(),
        meetings.len(),
        law_titles.len(),
        people.len()
    );
    Ok(())
}

/// 会議体ページを丸ごと生成する。戻り値は (タイトル, ページ, 開催回数)。
fn write_committees(
    wiki: &Path,
    meetings: &[MeetingInfo],
    law_titles: &BTreeMap<String, String>,
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
            body.push_str(&format!("| {} | [{}]({}) | {} |\n", m.date, cell(&m.title), rel_link(&rel, &m.page), cell(&m.description)));
        }

        let mut law_counts: BTreeMap<&str, usize> = BTreeMap::new();
        for m in ms {
            for id in &m.laws {
                *law_counts.entry(id.as_str()).or_default() += 1;
            }
        }
        let mut laws: Vec<(&str, usize)> = law_counts.into_iter().filter(|(id, _)| law_titles.contains_key(*id)).collect();
        laws.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));
        if !laws.is_empty() {
            body.push_str("\n## 扱った法令\n\n");
            for (id, n) in &laws {
                body.push_str(&format!("- [{}]({})（{n} 回）\n", law_titles[*id], rel_link(&rel, &law_page(id))));
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
                let refs: Vec<String> = mms.iter().map(|m| format!("[{}]({})", m.date, rel_link(&rel, &m.page))).collect();
                body.push_str(&format!("- {name}（{}）\n", refs.join("、")));
            }
        }
        let organization = ms[0].organization.clone();
        let latest = ms.iter().map(|m| m.date.as_str()).max().unwrap_or("");
        let stats = format!("{organization}・wiki 収録 {} 回（最新 {latest}）", ms.len());
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
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        out.push(MeetingInfo {
            page: rel_path(wiki, &path),
            title: page.get_str("title").to_string(),
            date: page.get_str("date").to_string(),
            corpus,
            organization: page.get_str("organization").to_string(),
            committee: page.get_str("committee").to_string(),
            description: page.get_str("description").to_string(),
            laws,
            speakers,
        });
    }
    out.sort_by(|a, b| b.date.cmp(&a.date).then_with(|| a.page.cmp(&b.page)));
    Ok(out)
}

fn write_index(
    wiki: &Path,
    meetings: &[MeetingInfo],
    committees: &[(String, String, usize)],
    bills: &[BillInfo],
    pubcomments: &[PubcommentInfo],
    law_titles: &BTreeMap<String, String>,
    people: &[String],
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

    if !pubcomments.is_empty() {
        let mut recent: Vec<&PubcommentInfo> = pubcomments.iter().collect();
        recent.sort_by(|a, b| b.date.cmp(&a.date));
        body.push_str(&format!("\n## 最近のパブコメ（{}）\n\n| 日付 | 案件 | 状況 |\n|---|---|---|\n", pubcomments.len()));
        for pc in recent.iter().take(20) {
            body.push_str(&format!("| {} | [{}]({}) | {} |\n", pc.date, cell(&pc.title), pc.page, cell(&pc.description)));
        }
    }

    if !bills.is_empty() {
        body.push_str(&format!("\n## 最近の議案（{}）\n\n| 日付 | 議案 | 経過 |\n|---|---|---|\n", bills.len()));
        for b in bills.iter().take(30) {
            body.push_str(&format!("| {} | [{}]({}) | {} |\n", b.date, cell(&b.title), b.page, cell(&b.description)));
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

    body.push_str(&format!("\n## 人物（{}）\n\n", people.len()));
    let links: Vec<String> = people
        .iter()
        .map(|n| format!("[{n}]({})", person_page(n)))
        .collect();
    body.push_str(&links.join(" · "));
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
        let desc = meetings
            .iter()
            .find(|m| m.page == t.page)
            .map(|m| m.description.as_str())
            .unwrap_or("");
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
    fn law_timeline_merges_meetings_bills_and_revisions() {
        let summary = "質疑[^1]。\n\n[^1]: [kokkai:M1_001](https://kokkai.ndl.go.jp/txt/M1/1) 「副反応の救済を拡充すべき」";
        let (root, wiki, work) = setup("予防接種法の救済拡充を質疑", summary);
        std::fs::write(
            wiki.join("laws/L1.md"),
            law().replace(
                "law_id: \"L1\"\n",
                "law_id: \"L1\"\nrevisions: [{\"date\":\"2026-11-20\",\"kind\":\"promulgated\",\"label\":\"公布: 予防接種法の一部を改正する法律（令和八年法律第九十号）\",\"law_num\":\"令和八年法律第九十号\"}]\n",
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
        std::fs::write(
            wiki.join("people/山田太郎.md"),
            format!("---\ntype: \"person\"\ntitle: \"山田太郎\"\ndescription: \"予防接種の救済拡充を追及\"\n---\n{}", person_body("山田太郎", "", narrative, "")),
        )
        .unwrap();
        run_finalize(&FinalizeArgs { wiki: wiki.clone(), work: work.clone() }).unwrap();
        run_finalize(&FinalizeArgs { wiki: wiki.clone(), work }).unwrap();

        let person = Page::read(&wiki.join("people/山田太郎.md")).unwrap();
        assert_eq!(first_llm_block(&person.body), narrative, "LLM の経緯は描き直しでも残る");
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
