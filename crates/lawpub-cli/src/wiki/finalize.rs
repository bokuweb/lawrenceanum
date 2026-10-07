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

/// 指示書 (.github/wiki-agent.md) で LLM に付けさせる、形式的な言及だけの会議の印。
const FORMAL_MENTION_PREFIX: &str = "形式的言及";

pub struct FinalizeArgs {
    pub wiki: PathBuf,
    pub work: PathBuf,
}

/// 人物ページの 1 行: (会議, 会派, 言及した法令 ID)。
type Remark<'a> = (&'a MeetingInfo, Option<String>, Vec<String>);

#[derive(Debug, Clone)]
struct MeetingInfo {
    page: String,
    title: String,
    date: String,
    description: String,
    laws: Vec<String>,
    speakers: Vec<(String, Option<String>, Vec<String>)>,
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
        if done {
            state.mark(&task.key, "linked", &today);
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

    // 3. 法令ページの時系列。どの会議からも参照されず LLM 区間も空のまま新規作成したページは消す。
    let mut law_titles: BTreeMap<String, String> = BTreeMap::new();
    for path in walk_md(wiki, "laws") {
        let rel = rel_path(wiki, &path);
        let mut page = Page::read(&path)?;
        let law_id = page.get_str("law_id").to_string();
        let mut rows: Vec<&MeetingInfo> = meetings
            .iter()
            .filter(|m| m.laws.contains(&law_id))
            .collect();
        let llm_empty = llm_blocks(&page.body).iter().all(|b| b.trim().is_empty());
        if rows.is_empty() && llm_empty && created_laws.contains(&rel) {
            std::fs::remove_file(&path)?;
            continue;
        }
        law_titles.insert(law_id.clone(), page.get_str("title").to_string());
        rows.sort_by(|a, b| b.date.cmp(&a.date).then_with(|| a.page.cmp(&b.page)));
        let mut table = String::from("| 日付 | 会議 | 概要 |\n|---|---|---|\n");
        for m in &rows {
            table.push_str(&format!(
                "| {} | [{}]({}) | {} |\n",
                m.date,
                cell(&m.title),
                rel_link(&rel, &m.page),
                cell(&m.description)
            ));
        }
        if let Some(body) = replace_block(&page.body, "timeline", &table) {
            if body != page.body {
                page.body = body;
                page.set("timestamp", json!(now_rfc3339()));
                page.write(&path)?;
            }
        }
    }

    // 4. 人物ページ (国会の発言者のみ。審議会は姓+役職のため同定できない)。
    //    議事進行や請願報告だけの「形式的言及」の会議は人物の発言として数えない。
    let mut people: BTreeMap<String, Vec<Remark>> = BTreeMap::new();
    for m in meetings
        .iter()
        .filter(|m| !m.description.starts_with(FORMAL_MENTION_PREFIX))
    {
        for (name, group, laws) in &m.speakers {
            people
                .entry(name.clone())
                .or_default()
                .push((m, group.clone(), laws.clone()));
        }
    }
    // people/ は丸ごと生成物なので、対象から外れた人物のページは消す。
    for path in walk_md(wiki, "people") {
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        if !people.keys().any(|n| file_safe(n) == stem) {
            std::fs::remove_file(&path)?;
        }
    }
    for (name, remarks) in &people {
        let mut remarks = remarks.clone();
        remarks.sort_by(|a, b| b.0.date.cmp(&a.0.date));
        let rel = person_page(name);
        let path = wiki.join(&rel);
        let mut page = if path.exists() {
            Page::read(&path)?
        } else {
            Page::default()
        };
        let affiliation = remarks.iter().find_map(|r| r.1.clone()).unwrap_or_default();
        let mut table = String::from("| 日付 | 会議 | 言及した法令 |\n|---|---|---|\n");
        for (m, _, laws) in &remarks {
            let laws: Vec<String> = laws
                .iter()
                .filter_map(|id| {
                    law_titles
                        .get(id)
                        .map(|t| format!("[{}]({})", t, rel_link(&rel, &law_page(id))))
                })
                .collect();
            table.push_str(&format!(
                "| {} | [{}]({}) | {} |\n",
                m.date,
                cell(&m.title),
                rel_link(&rel, &m.page),
                laws.join("、")
            ));
        }
        let description = page.get_str("description").to_string();
        let tags = page.get("tags").cloned().unwrap_or(json!([]));
        let mut fm = vec![
            ("type".to_string(), json!("person")),
            ("title".to_string(), json!(name)),
            ("description".to_string(), json!(description)),
            ("affiliation".to_string(), json!(affiliation)),
            (
                "timestamp".to_string(),
                page.get("timestamp")
                    .cloned()
                    .unwrap_or(json!(now_rfc3339())),
            ),
            ("tags".to_string(), tags),
        ];
        let body = match replace_block(&page.body, "remarks", &table) {
            Some(b) => b,
            None => format!(
                "\n# {name}\n\n国会会議録で法令に言及した発言の一覧です（会議録の記載事実のみ）。\n\n## 法令に言及した発言\n\n<!-- lawpub:begin remarks -->\n{table}<!-- lawpub:end remarks -->\n"
            ),
        };
        if body != page.body {
            fm[4].1 = json!(now_rfc3339());
        }
        page.frontmatter = fm;
        page.body = body;
        page.write(&path)?;
    }

    // 5. index.md / log.md。
    write_index(
        wiki,
        &meetings,
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
        let speakers = page
            .get("speakers")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|s| {
                        let name = s["name"].as_str()?.to_string();
                        let group = s["group"].as_str().map(String::from);
                        let laws = s["laws"]
                            .as_array()
                            .map(|l| {
                                l.iter()
                                    .filter_map(|v| v.as_str().map(String::from))
                                    .collect()
                            })
                            .unwrap_or_default();
                        Some((name, group, laws))
                    })
                    .collect()
            })
            .unwrap_or_default();
        out.push(MeetingInfo {
            page: rel_path(wiki, &path),
            title: page.get_str("title").to_string(),
            date: page.get_str("date").to_string(),
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
    law_titles: &BTreeMap<String, String>,
    people: &[String],
) -> Result<()> {
    let mut body = String::from(
        "\n# lawrenceanum 法令経緯 wiki\n\n\
         国会会議録・審議会議事録のうち法令に言及した発言を、法令・会議・人物・論点ごとに整理した wiki です。\n\
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
            ("title".into(), json!("lawrenceanum 法令経緯 wiki")),
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
        assert!(law.contains("| 2026-10-01 | [参議院 厚生労働委員会 第1号](../meetings/kokkai/M1.md) | 予防接種法の救済拡充を質疑 |"));
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
    fn formal_mentions_do_not_make_person_pages() {
        let summary = "請願の報告のみ[^1]。\n\n[^1]: [kokkai:M1_001](https://kokkai.ndl.go.jp/txt/M1/1) 「請願は四種三十二件であります」";
        let (root, wiki, work) = setup("形式的言及: 請願・陳情の件数報告", summary);
        std::fs::create_dir_all(wiki.join("people")).unwrap();
        std::fs::write(
            wiki.join("people/山田太郎.md"),
            "---\ntype: \"person\"\n---\n",
        )
        .unwrap();
        run_finalize(&FinalizeArgs {
            wiki: wiki.clone(),
            work,
        })
        .unwrap();
        assert!(!wiki.join("people/山田太郎.md").exists());
        assert!(std::fs::read_to_string(wiki.join("laws/L1.md"))
            .unwrap()
            .contains("形式的言及: 請願"));
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
