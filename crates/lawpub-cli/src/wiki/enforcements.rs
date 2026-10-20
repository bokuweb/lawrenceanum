//! 施行予定単位の wiki。原文の収集・同定は決定的に行い、説明だけを LLM に任せる。
use super::*;
use serde_json::json;
use sha2::{Digest, Sha256};

pub const KIND: &str = "enforcement";
pub const UNKNOWN: &str = "確認できる資料がありません。";
const VERSION: u32 = 1;

pub fn date(event: &Value) -> &str {
    event["scheduled_enforcement_date"]
        .as_str()
        .filter(|s| !s.is_empty())
        .or_else(|| event["effective_date"].as_str())
        .unwrap_or("")
}

/// 同じ改正でも段階施行・期日変更を混同しない ID。
pub fn event_key(law: &str, event: &Value) -> String {
    hex::encode(Sha256::digest(
        serde_json::to_vec(&json!([
            law,
            event["event_id"],
            event["revision_id"],
            date(event)
        ]))
        .expect("JSON values"),
    ))
}

fn safe_id(id: &str) -> bool {
    !id.is_empty() && id.bytes().all(|c| c.is_ascii_alphanumeric())
}

fn excerpt(out: &mut Vec<Value>, prefix: &str, id: &str, url: &str, text: &str, scope: &str) {
    // 引用元として遷移できる原本だけを使う。文字列は引用照合のため加工しない。
    if (url.starts_with("https://") || url.starts_with("http://")) && !text.trim().is_empty() {
        out.push(json!({"ref": format!("{prefix}#{id}"), "url":url, "text":text, "scope":scope}));
    }
}

/// 法令名だけのリンクは使わない。改正命令名の完全一致 + 公布前180日以内 + 同名改正が一つ。
fn matches_comment(case: &Value, event: &Value, events: &[Value]) -> bool {
    let title = event["amending_law_title"].as_str().unwrap_or("");
    let name = case["command_title"]
        .as_str()
        .unwrap_or("")
        .trim()
        .trim_end_matches("（案）")
        .trim_end_matches("(案)");
    if title.is_empty() || name != title {
        return false;
    }
    let parse = |s: &str| chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").ok();
    let Some(published) = case["result_published"]
        .as_str()
        .or_else(|| case["reception_end"].as_str())
        .and_then(parse)
    else {
        return false;
    };
    let Some(promulgated) = event["promulgation_date"].as_str().and_then(parse) else {
        return false;
    };
    if !(0..=180).contains(&(promulgated - published).num_days()) {
        return false;
    }
    // 同一改正の段階施行は許すが、別の改正と区別できなければ採用しない。
    !events.iter().any(|other| {
        other["amending_law_title"].as_str() == Some(title)
            && other["promulgation_date"] != event["promulgation_date"]
            && other["promulgation_date"]
                .as_str()
                .and_then(parse)
                .is_some_and(|d| (0..=180).contains(&(d - published).num_days()))
    })
}

fn paragraph_text(side: &Value) -> String {
    side["paragraphs"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|p| p["text"].as_str())
        .collect::<Vec<_>>()
        .join("\n")
}

fn diff_excerpts(
    out: &mut Vec<Value>,
    prefix: &str,
    law: &str,
    diff: &Value,
    from: &str,
    to: &str,
) {
    for (i, article) in diff["articles"]
        .as_array()
        .into_iter()
        .flatten()
        .enumerate()
    {
        let mut parts = Vec::new();
        match article["change_type"].as_str().unwrap_or("") {
            "added" => parts.push((String::new(), paragraph_text(&article["to"]))),
            "removed" => parts.push((paragraph_text(&article["from"]), String::new())),
            "modified" => {
                for p in article["paragraphs"].as_array().into_iter().flatten() {
                    let mut before = String::new();
                    let mut after = String::new();
                    match p["change_type"].as_str().unwrap_or("") {
                        "added" => {
                            after.push_str(p["text"].as_str().unwrap_or(""));
                        }
                        "removed" => {
                            before.push_str(p["text"].as_str().unwrap_or(""));
                        }
                        "modified" => {
                            for op in p["text_diff"].as_array().into_iter().flatten() {
                                let text = op["text"].as_str().unwrap_or("");
                                match op["op"].as_str().unwrap_or("") {
                                    "equal" => {
                                        before.push_str(text);
                                        after.push_str(text);
                                    }
                                    "delete" => before.push_str(text),
                                    "insert" => after.push_str(text),
                                    _ => {}
                                }
                            }
                        }
                        _ => continue,
                    }
                    // 変更されていない項を飛ばして連結すると、原文にない連続引用が可能になる。
                    parts.push((before, after));
                }
            }
            _ => continue,
        }
        let article_id = article["article_id"].as_str().unwrap_or("");
        for (p, (before, after)) in parts.into_iter().enumerate() {
            for (side, revision, text) in [("before", from, before), ("after", to, after)] {
                // e-Gov の版 URL は /law/{法令ID}/{施行日}_{改正法令ID}。
                // ローカル本文のハッシュ等から、存在しない原本 URL を作らない。
                let Some(version) = revision.strip_prefix(&format!("{law}_")) else {
                    continue;
                };
                excerpt(
                    out,
                    prefix,
                    &format!("diff-{i}-{p}-{side}"),
                    &format!("https://laws.e-gov.go.jp/law/{law}/{version}"),
                    &text,
                    &format!("対象施行日の改正{side}条文 ({article_id})"),
                );
            }
        }
    }
}

/// 元コーパスから再構築できるバンドル。取得エラーは隠さず次回再試行する。
fn bundle(source: &Source, law: &str, event: &Value, events: &[Value]) -> Result<Value> {
    let key = event_key(law, event);
    let prefix = format!("{KIND}:{law}/{key}");
    let mut excerpts = Vec::new();
    let kanpo = &event["kanpo"];
    if kanpo["linked"].as_bool() == Some(true) {
        excerpt(&mut excerpts, &prefix, "kanpo", kanpo["pdf_url"].as_str().unwrap_or(""),
            kanpo["amend_text"].as_str().unwrap_or(""),
            "改正全体の官報原文。複数法令・段階施行を含み得る。対象施行日の変更か必ず原文で確認する");
    }
    if let Some(revision) = event["revision_id"]
        .as_str()
        .filter(|s| safe_id(s) || s.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_'))
    {
        if let Some(index) = source.get_json(&format!("laws/{law}/diffs.json"))? {
            for d in index["diffs"].as_array().into_iter().flatten() {
                if d["to_revision_id"].as_str() != Some(revision)
                    || d["to_effective_date"].as_str() != Some(date(event))
                {
                    continue;
                }
                let Some(path) = d["path"]
                    .as_str()
                    .filter(|p| p.starts_with(&format!("laws/{law}/diff/")) && !p.contains(".."))
                else {
                    continue;
                };
                if let Some(diff) = source.get_json(path)? {
                    diff_excerpts(
                        &mut excerpts,
                        &prefix,
                        law,
                        &diff,
                        d["from_revision_id"].as_str().unwrap_or(""),
                        revision,
                    );
                }
            }
        }
    }
    if let Some(links) = source.get_json(&format!("links/law-to-pubcomment/{law}.json"))? {
        for link in links["linked_pubcomments"]
            .as_array()
            .into_iter()
            .flatten()
            .take(30)
        {
            let Some(id) = link["case_id"].as_str().filter(|s| safe_id(s)) else {
                continue;
            };
            let Some(case) = source.get_json(&format!("pubcomment/{id}.json"))? else {
                continue;
            };
            if !matches_comment(&case, event, events) {
                continue;
            }
            let url = case["source"]["detail_url"].as_str().unwrap_or("");
            let scope =
                "対象改正のパブコメ。改正全体の説明であり、段階施行の適用範囲は別途確認する";
            if let Some(text) = case["legal_basis"].as_str() {
                excerpt(
                    &mut excerpts,
                    &prefix,
                    &format!("comment-{id}-basis"),
                    url,
                    text,
                    scope,
                );
            }
            for (i, a) in case["attachments"]
                .as_array()
                .into_iter()
                .flatten()
                .enumerate()
            {
                excerpt(
                    &mut excerpts,
                    &prefix,
                    &format!("comment-{id}-attachment-{i}"),
                    a["url"].as_str().unwrap_or(""),
                    a["extracted_text"].as_str().unwrap_or(""),
                    scope,
                );
            }
            for (i, opinion) in case["opinions"]
                .as_array()
                .into_iter()
                .flatten()
                .enumerate()
            {
                excerpt(
                    &mut excerpts,
                    &prefix,
                    &format!("comment-{id}-response-{i}"),
                    url,
                    opinion["ministry_response"].as_str().unwrap_or(""),
                    scope,
                );
            }
        }
    }
    let context = json!({"event_id":event["event_id"], "revision_id":event["revision_id"],
        "date":date(event), "promulgation_date":event["promulgation_date"],
        "amending_law_id":event["amending_law_id"], "amending_law_num":event["amending_law_num"],
        "amending_law_title":event["amending_law_title"], "enforcement_comment":event["enforcement_comment"]});
    let fingerprint = hex::encode(Sha256::digest(serde_json::to_vec(&json!([
        VERSION, context, excerpts
    ]))?));
    Ok(
        json!({"law_id":law, "explanation_id":key, "event":context, "source_fingerprint":fingerprint, "excerpts":excerpts}),
    )
}

pub fn load_bundle(source: &Source, id: &str) -> Result<Option<Value>> {
    let Some((law, key)) = id.split_once('/') else {
        return Ok(None);
    };
    if !safe_id(law) || key.len() != 64 || !key.bytes().all(|c| c.is_ascii_hexdigit()) {
        return Ok(None);
    }
    let Some(timeline) = source.get_json(&format!("laws/{law}/timeline.json"))? else {
        return Ok(None);
    };
    let Some(events) = timeline["events"].as_array() else {
        return Ok(None);
    };
    let Some(event) = events.iter().find(|e| event_key(law, e) == key) else {
        return Ok(None);
    };
    bundle(source, law, event, events).map(Some)
}

pub fn units(doc: &Value) -> Vec<Unit> {
    doc["excerpts"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|v| {
            Some(Unit {
                reference: v["ref"].as_str()?.into(),
                url: v["url"].as_str()?.into(),
                text: v["text"].as_str()?.into(),
                speaker: None,
                group: None,
                position: None,
            })
        })
        .collect()
}

/// 各節に根拠、または確認不能の明示が必要。他の改正の引用は受け付けない。
pub fn explanation_errors(page: &Page) -> Vec<String> {
    let body = first_llm_block(&page.body);
    let mut errors = Vec::new();
    let prefix = format!(
        "enforcement:{}/{}#",
        page.get_str("law_id"),
        page.get_str("explanation_id")
    );
    let citations = check::citations_in(&body);
    if citations.is_empty() || citations.iter().any(|c| !c.reference.starts_with(&prefix)) {
        errors.push("この施行予定に対応する出典が必要です".into());
    }
    for heading in ["変更内容", "経緯", "根拠"] {
        let marker = format!("## {heading}\n");
        let section = body
            .split_once(&marker)
            .map(|(_, s)| s.split("\n## ").next().unwrap_or(s))
            .unwrap_or("");
        let prose = section
            .lines()
            .filter(|l| !l.trim_start().starts_with("[^"))
            .collect::<Vec<_>>()
            .join("\n");
        let has_ref = citations
            .iter()
            .any(|c| prose.contains(&format!("[^{}]", c.label)));
        if prose.trim() != UNKNOWN && !has_ref {
            errors.push(format!(
                "「{heading}」には出典付きの説明、または「{UNKNOWN}」が必要です"
            ));
        }
    }
    errors
}

pub fn ready(page: &Page) -> bool {
    !page.get_str("description").trim().is_empty()
        && !page.get_str("source_fingerprint").is_empty()
        && page.get_str("source_fingerprint") == page.get_str("llm_source_fingerprint")
        && explanation_errors(page).is_empty()
}

pub fn plan(args: &plan::PlanArgs, source: &Source, today: &str) -> Result<Vec<Task>> {
    let mut tasks = Vec::new();
    if args.max_enforcements == 0 {
        return Ok(tasks);
    }
    let Some(upcoming) = source.get_json("enforcement/upcoming.json")? else {
        return Ok(tasks);
    };
    let mut seen = std::collections::HashSet::new();
    let mut timelines = BTreeMap::new();
    for item in upcoming["items"]
        .as_array()
        .into_iter()
        .flatten()
        .take(args.max_probes)
    {
        if tasks.len() >= args.max_enforcements {
            break;
        }
        let Some(law) = item["law_id"].as_str().filter(|s| safe_id(s)) else {
            continue;
        };
        let Some(day) = item["date"].as_str().filter(|d| *d > today) else {
            continue;
        };
        if !timelines.contains_key(law) {
            let timeline = source.get_json(&format!("laws/{law}/timeline.json"))?;
            timelines.insert(
                law.to_string(),
                timeline
                    .map(|t| t["events"].as_array().cloned().unwrap_or_default())
                    .unwrap_or_default(),
            );
        }
        let events = &timelines[law];
        // upcoming の日付順を保つ。同じ法令の遠い施行で直近の他法令を押し出さない。
        for event in events.iter().filter(|e| date(e) == day) {
            if tasks.len() >= args.max_enforcements {
                break;
            }
            let key = event_key(law, event);
            if !seen.insert(key.clone()) {
                continue;
            }
            let doc = bundle(source, law, event, events)?;
            if units(&doc).is_empty() {
                continue;
            }
            let rel = format!("enforcements/{law}/{key}.md");
            let path = args.wiki.join(&rel);
            if path.exists()
                && Page::read(&path).is_ok_and(|p| {
                    ready(&p)
                        && p.get_str("source_fingerprint")
                            == doc["source_fingerprint"].as_str().unwrap_or("")
                })
            {
                continue;
            }
            // 変更された入力に対して古い説明を完了扱い・公開しない。
            let title = format!(
                "{} — {} 施行",
                item["title"].as_str().unwrap_or(law),
                date(event)
            );
            let mut page = Page {
                frontmatter: Vec::new(),
                body: format!("# {title}\n\n{LLM_BEGIN}\n\n{LLM_END}\n"),
            };
            for (k, v) in [
                ("type", json!(KIND)),
                ("title", json!(title)),
                ("description", json!("")),
                ("tags", json!([])),
                ("law_id", json!(law)),
                ("explanation_id", json!(key)),
                ("event_id", event["event_id"].clone()),
                ("revision_id", event["revision_id"].clone()),
                ("date", json!(date(event))),
                ("source_fingerprint", doc["source_fingerprint"].clone()),
            ] {
                page.set(k, v);
            }
            let created = if path.exists() {
                vec![]
            } else {
                vec![rel.clone()]
            };
            page.write(&path)?;
            let id = format!("{law}/{key}");
            let raw = args
                .work
                .join("docs/enforcement")
                .join(format!("{law}_{key}.json"));
            std::fs::create_dir_all(raw.parent().unwrap())?;
            std::fs::write(raw, serde_json::to_vec_pretty(&doc)?)?;
            let source_rel = format!("sources/enforcement-{law}-{key}.json");
            // 長い原文は LLM 入力だけを制限する。検証用には全文を保持。
            let mut prompt_doc = doc;
            if let Some(excerpts) = prompt_doc["excerpts"].as_array_mut() {
                excerpts.truncate(40);
                let mut budget = 36_000;
                excerpts.retain_mut(|e| {
                    if budget == 0 {
                        return false;
                    }
                    let text = e["text"]
                        .as_str()
                        .unwrap_or("")
                        .chars()
                        .take(12_000.min(budget))
                        .collect::<String>();
                    budget -= text.chars().count();
                    e["text"] = json!(text);
                    true
                });
            }
            std::fs::write(
                args.work.join(&source_rel),
                serde_json::to_vec_pretty(&prompt_doc)?,
            )?;
            tasks.push(Task {
                key: format!("enforcement:{id}"),
                kind: KIND.into(),
                id,
                date: date(event).into(),
                title,
                page: rel,
                source: source_rel,
                laws: vec![],
                people: vec![],
                committee: None,
                created,
                person: None,
            });
        }
    }
    Ok(tasks)
}

/// SPA の施行予定カード用。最新コーパスと一致する完了ページだけを公開する。
pub fn export(pages: &[(String, Page)], out: &Path) -> Result<std::collections::BTreeSet<String>> {
    let source = Source::new(out.parent().unwrap_or(out).to_string_lossy().as_ref())?;
    let mut laws: BTreeMap<String, Vec<Value>> = BTreeMap::new();
    let mut published = std::collections::BTreeSet::new();
    for (rel, page) in pages
        .iter()
        .filter(|(_, p)| p.get_str("type") == KIND && ready(p))
    {
        let law = page.get_str("law_id");
        let id = format!("{law}/{}", page.get_str("explanation_id"));
        let current = match load_bundle(&source, &id) {
            Ok(Some(d)) => d,
            Ok(None) => continue,
            Err(e) => {
                tracing::warn!("wiki-export: skip {rel}: {e:#}");
                continue;
            }
        };
        if current["source_fingerprint"].as_str() != Some(page.get_str("source_fingerprint")) {
            continue;
        }
        published.insert(rel.clone());
        laws.entry(law.into()).or_default().push(json!({"explanation_id":page.get_str("explanation_id"),
            "event_id":page.get("event_id"),"revision_id":page.get("revision_id"),"date":page.get_str("date"),
            "path":rel.trim_end_matches(".md"),"description":page.get_str("description"),"body":page.body}));
    }
    std::fs::create_dir_all(out.join("enforcements"))?;
    for (law, mut items) in laws {
        items.sort_by(|a, b| {
            a["date"].as_str().cmp(&b["date"].as_str()).then(
                a["explanation_id"]
                    .as_str()
                    .cmp(&b["explanation_id"].as_str()),
            )
        });
        std::fs::write(
            out.join("enforcements").join(format!("{law}.json")),
            serde_json::to_vec(&json!({"schema_version":1,"law_id":law,"items":items}))?,
        )?;
    }
    Ok(published)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(root: &Path, path: &str, value: Value) {
        let dest = root.join(path);
        std::fs::create_dir_all(dest.parent().unwrap()).unwrap();
        std::fs::write(dest, serde_json::to_vec(&value).unwrap()).unwrap();
    }

    fn event(day: &str) -> Value {
        json!({"event_id":"amendment-1", "revision_id":format!("L1_{}_A1",day.replace('-',"")),
            "effective_date":day,"promulgation_date":"2026-10-01","amending_law_title":"テスト法の一部を改正する法律",
            "kanpo":{"linked":true,"pdf_url":"https://www.kanpo.go.jp/example.pdf",
                "amend_text":"第一条の申請の期限を三十日から六十日に改める。附則 この法律は令和九年一月一日から施行する。"}})
    }

    fn setup() -> (PathBuf, plan::PlanArgs, Source) {
        let root = temp_dir("enforcements");
        let public = root.join("public");
        let args = plan::PlanArgs {
            base_url: public.to_string_lossy().into(),
            wiki: root.join("wiki"),
            work: root.join("work"),
            max_items: 0,
            max_bills: 0,
            max_pubcomments: 0,
            max_kanpo: 0,
            max_people: 0,
            max_roster: 0,
            reiki_base_url: String::new(),
            chunk_size: 4,
            max_enforcements: 5,
            max_probes: 20,
            lookback_days: 30,
            today: Some("2026-10-19".into()),
        };
        std::fs::create_dir_all(args.work.join("sources")).unwrap();
        write(
            &public,
            "enforcement/upcoming.json",
            json!({"items":[{"law_id":"L1","title":"テスト法","date":"2027-01-01"},
                {"law_id":"L1","title":"テスト法","date":"2027-04-01"}]}),
        );
        write(
            &public,
            "laws/L1/timeline.json",
            json!({"events":[event("2027-01-01"),event("2027-04-01")]}),
        );
        let source = Source::new(&args.base_url).unwrap();
        (root, args, source)
    }

    fn fill(args: &plan::PlanArgs, task: &Task) {
        let doc: Value =
            serde_json::from_slice(&std::fs::read(args.work.join(&task.source)).unwrap()).unwrap();
        let u = &doc["excerpts"][0];
        let mut p = Page::read(&args.wiki.join(&task.page)).unwrap();
        p.set("description", json!("申請期限の改正と施行の根拠"));
        p.body = format!("{LLM_BEGIN}\n## 変更内容\n申請期限を六十日に変更する[^1]。\n\n## 経緯\n{UNKNOWN}\n\n## 根拠\n附則に施行期日が定められている[^1]。\n\n[^1]: [{}]({}) 「第一条の申請の期限を三十日から六十日に改める。」\n{LLM_END}\n",u["ref"].as_str().unwrap(),u["url"].as_str().unwrap());
        p.write(&args.wiki.join(&task.page)).unwrap();
    }

    #[test]
    fn plan_keeps_date_order_across_laws_and_respects_limit() {
        let (root, mut args, source) = setup();
        write(
            &root.join("public"),
            "laws/L2/timeline.json",
            json!({"events":[event("2027-02-01")]}),
        );
        write(
            &root.join("public"),
            "enforcement/upcoming.json",
            json!({"items":[
            {"law_id":"L1","title":"テスト法","date":"2027-01-01"},
            {"law_id":"L2","title":"別の法令","date":"2027-02-01"},
            {"law_id":"L1","title":"テスト法","date":"2027-04-01"}]}),
        );
        args.max_enforcements = 2;
        let tasks = plan(&args, &source, "2026-10-19").unwrap();
        assert_eq!(tasks.len(), 2);
        assert_eq!(tasks[1].date, "2027-02-01");
        assert!(tasks[1].id.starts_with("L2/"));
        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn stages_finalize_verify_export_and_refresh_independently() {
        let (root, args, source) = setup();
        let tasks = plan(&args, &source, "2026-10-19").unwrap();
        assert_eq!(tasks.len(), 2);
        assert_ne!(tasks[0].page, tasks[1].page);
        let p = Plan {
            schema_version: 1,
            before: BTreeMap::new(),
            date: "2026-10-19".into(),
            base_url: args.base_url.clone(),
            tasks: tasks.clone(),
        };
        write(&args.work, "plan.json", serde_json::to_value(p).unwrap());
        for t in &tasks {
            fill(&args, t);
        }
        finalize::run_finalize(&finalize::FinalizeArgs {
            wiki: args.wiki.clone(),
            work: args.work.clone(),
        })
        .unwrap();
        let check = check::CheckArgs {
            wiki: args.wiki.clone(),
            work: args.work.clone(),
            base_url: args.base_url.clone(),
            changed_only: false,
        };
        check::run_check(&check).unwrap();
        assert!(plan(&args, &source, "2026-10-19").unwrap().is_empty());
        // work を消しても配信済み原文から引用を検証できる。
        std::fs::remove_dir_all(args.work.join("docs")).unwrap();
        check::run_check(&check).unwrap();
        let out = root.join("public/wiki");
        let export_args = export::ExportArgs {
            wiki: args.wiki.clone(),
            out: out.clone(),
        };
        export::run_export(&export_args).unwrap();
        let index: Value =
            serde_json::from_slice(&std::fs::read(out.join("enforcements/L1.json")).unwrap())
                .unwrap();
        assert_eq!(index["items"].as_array().unwrap().len(), 2);
        // 一つの段階の資料変更で、その段階の公開だけを停止して再生成する。
        let mut first = event("2027-01-01");
        first["kanpo"]["amend_text"] =
            json!("改正された原本の本文です。申請期限の訂正。附則により施行する。");
        write(
            &root.join("public"),
            "laws/L1/timeline.json",
            json!({"events":[first,event("2027-04-01")]}),
        );
        export::run_export(&export_args).unwrap();
        let index: Value =
            serde_json::from_slice(&std::fs::read(out.join("enforcements/L1.json")).unwrap())
                .unwrap();
        assert_eq!(index["items"].as_array().unwrap().len(), 1);
        assert!(!out
            .join("page")
            .join(tasks[0].page.replace(".md", ".json"))
            .exists());
        let report: Value = serde_json::from_slice(&std::fs::read(out.join("report-manifest.json")).unwrap()).unwrap();
        assert!(report["pages"].get(tasks[0].page.trim_end_matches(".md")).is_none());
        assert!(report["pages"].get(tasks[1].page.trim_end_matches(".md")).is_some());
        let retry = plan(&args, &source, "2026-10-19").unwrap();
        assert_eq!(retry.len(), 1);
        assert!(!ready(
            &Page::read(&args.wiki.join(&retry[0].page)).unwrap()
        ));
        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn citations_require_original_quote_url_event_and_all_sections() {
        let (root, args, source) = setup();
        let task = plan(&args, &source, "2026-10-19").unwrap().remove(0);
        fill(&args, &task);
        let path = args.wiki.join(&task.page);
        let original = Page::read(&path).unwrap();
        let check = check::CheckArgs {
            wiki: args.wiki.clone(),
            work: args.work.clone(),
            base_url: args.base_url.clone(),
            changed_only: false,
        };
        // 第二の未記入雛形は空なので検証対象の記述を持たない。
        check::run_check(&check).unwrap();
        for (before, after) in [
            (
                "第一条の申請の期限を三十日から六十日に改める。",
                "原文に存在しない推測の説明をここに書きます。",
            ),
            (
                "https://www.kanpo.go.jp/example.pdf",
                "https://example.com/wrong.pdf",
            ),
            ("enforcement:L1/", "enforcement:L2/"),
            (
                "## 経緯\n確認できる資料がありません。",
                "## 経緯\n利便性向上のため改正された。",
            ),
            ("## 根拠", "## その他"),
        ] {
            let mut changed = Page::read(&path).unwrap();
            changed.body = original.body.replace(before, after);
            changed.write(&path).unwrap();
            assert!(check::run_check(&check).is_err(), "accepted {after}");
        }
        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn comments_need_amendment_identity_and_date_not_just_target_law() {
        let ev = event("2027-01-01");
        let mut case = json!({"command_title":"テスト法の一部を改正する法律（案）","result_published":"2026-09-20"});
        assert!(matches_comment(&case, &ev, &[ev.clone()]));
        case["command_title"] = json!("テスト法");
        assert!(!matches_comment(&case, &ev, &[ev.clone()]));
        case["command_title"] = ev["amending_law_title"].clone();
        case["result_published"] = json!("2025-09-20");
        assert!(!matches_comment(&case, &ev, &[ev.clone()]));
        case["result_published"] = json!("2026-09-20");
        let mut ambiguous = ev.clone();
        ambiguous["promulgation_date"] = json!("2026-10-02");
        assert!(!matches_comment(&case, &ev, &[ev.clone(), ambiguous]));
    }

    #[test]
    fn diffs_are_bound_to_the_revision_and_stage_date() {
        let (root, args, source) = setup();
        let ev = event("2027-01-01");
        write(
            &root.join("public"),
            "laws/L1/diffs.json",
            json!({"diffs":[{
            "to_revision_id":ev["revision_id"],"from_revision_id":"L1_20260101_A0",
            "to_effective_date":"2027-01-01","path":"laws/L1/diff/test.json"}]}),
        );
        write(
            &root.join("public"),
            "laws/L1/diff/test.json",
            json!({"articles":[{
            "article_id":"a1","change_type":"modified","paragraphs":[{"change_type":"modified","text_diff":[
                {"op":"equal","text":"申請期限は"},{"op":"delete","text":"三十日"},{"op":"insert","text":"六十日"},{"op":"equal","text":"とする。"}]},
                {"change_type":"unchanged","paragraph_no":"2"},
                {"change_type":"added","text":"申請には電子署名を付す。"}]}]}),
        );
        let doc = bundle(&source, "L1", &ev, &[ev.clone()]).unwrap();
        let units = units(&doc);
        assert!(units
            .iter()
            .any(|u| u.text.trim() == "申請期限は三十日とする。"));
        assert!(units
            .iter()
            .any(|u| u.text.trim() == "申請期限は六十日とする。"));
        assert!(units
            .iter()
            .any(|u| u.url == "https://laws.e-gov.go.jp/law/L1/20270101_A1"));
        assert!(units.iter().any(|u| u.text == "申請には電子署名を付す。"));
        assert!(!units
            .iter()
            .filter(|u| u.reference.contains("#diff-"))
            .any(|u| u.text.contains("六十日") && u.text.contains("電子署名")));
        assert_eq!(
            bundle(&source, "L1", &event("2027-04-01"), &[]).unwrap()["excerpts"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        // 取得可能な原文が無ければ推測でタスクを作らない。
        write(
            &root.join("public"),
            "laws/L1/timeline.json",
            json!({"events":[{"event_id":"empty","effective_date":"2027-05-01"}]}),
        );
        assert!(plan(&args, &source, "2026-10-19").unwrap().is_empty());
        std::fs::remove_dir_all(root).ok();
    }
}
