//! パブリックコメント (意見公募) を wiki に取り込む。
//!
//! - `sync`: 法令に結びつく案件 (`related_law_name` が法令名と一致) のページ `pubcomments/{case_id}.md`
//!   を index から LLM なしで作る (募集期間・結果公示・意見数・対象法令)。
//! - `plan_tasks`: 結果が公示されたのに未要約の案件を新しい順に LLM に渡す。結果文書 (「意見の要旨」と
//!   「考え方」の対照表) などの添付を引用単位にする。参照は `pubcomment:{case_id}#att{n}`。

use super::*;
use serde_json::json;
use std::collections::HashMap;

pub const KIND_PUBCOMMENT: &str = "pubcomment";
/// pubcomment ページの描画形式。上げると全案件を描き直す。
const RENDER_VERSION: u64 = 1;
/// LLM に渡す添付の上限 (結果文書らしいもの / その他 / 1 案件あたり)。
const RESULT_ATTACHMENT_CHARS: usize = 7_000;
const OTHER_ATTACHMENT_CHARS: usize = 1_500;
const MAX_CHARS_PER_CASE: usize = 12_000;

pub fn pubcomment_page(case_id: &str) -> String {
    format!("pubcomments/{}.md", file_safe(case_id))
}

/// `2026年9月4日` / `2026年5月26日18時0分` → 2026-09-04。
pub(crate) fn parse_jp_date(s: &str) -> Option<String> {
    let s: String = s.chars().filter(|c| !c.is_whitespace()).collect();
    let (y, rest) = s.split_once('年')?;
    let (m, rest) = rest.split_once('月')?;
    let d = rest.split_once('日')?.0;
    let date = chrono::NaiveDate::from_ymd_opt(y.parse().ok()?, m.parse().ok()?, d.parse().ok()?)?;
    Some(date.format("%Y-%m-%d").to_string())
}

/// 「○」や空白を除いた関連法令名を法令 ID に対応付ける (「A及びB」も分割)。
fn related_laws(name: &str, unique: &HashMap<String, String>) -> Vec<String> {
    let name: String = name.trim().trim_start_matches(['○', '◯']).trim().to_string();
    if name.is_empty() {
        return Vec::new();
    }
    if let Some(id) = unique.get(&name) {
        return vec![id.clone()];
    }
    for (i, _) in name.match_indices("及び") {
        let (l, r) = (&name[..i], &name[i + "及び".len()..]);
        if let (Some(a), Some(b)) = (unique.get(l), unique.get(r)) {
            return vec![a.clone(), b.clone()];
        }
    }
    Vec::new()
}

#[derive(Debug, Default)]
pub struct SyncStats {
    pub cases: usize,
    pub written: usize,
}

pub fn sync(
    source: &Source,
    wiki: &Path,
    unique: &HashMap<String, String>,
    titles: &HashMap<String, String>,
) -> Result<SyncStats> {
    let mut stats = SyncStats::default();
    let Some(index) = source.get_json("pubcomment/index.json")? else {
        return Ok(stats);
    };
    for c in index["cases"].as_array().into_iter().flatten() {
        let Some(case_id) = c["case_id"].as_str() else { continue };
        let laws = related_laws(c["related_law_name"].as_str().unwrap_or(""), unique);
        if laws.is_empty() {
            continue;
        }
        stats.cases += 1;
        let rel = pubcomment_page(case_id);
        let path = wiki.join(&rel);
        let start = c["reception_start"].as_str().and_then(parse_jp_date).or_else(|| c["reception_start"].as_str().map(|s| s.chars().take(10).collect()));
        let end = c["reception_end"].as_str().and_then(parse_jp_date).or_else(|| c["reception_end"].as_str().map(|s| s.chars().take(10).collect()));
        let result = c["result_published"].as_str().and_then(parse_jp_date);
        let opinions = c["opinion_count"].as_u64();
        let status = c["status"].as_str().unwrap_or("");

        let previous = if path.exists() { Page::read(&path).ok() } else { None };
        if let Some(prev) = &previous {
            let same = prev.get("render_version").and_then(Value::as_u64) == Some(RENDER_VERSION)
                && prev.get_str("status") == status
                && prev.get("result_date").and_then(Value::as_str) == result.as_deref()
                && prev.get("opinion_count").and_then(Value::as_u64) == opinions;
            if same {
                continue;
            }
        }
        for id in &laws {
            ensure_law_page(wiki, source.base(), id, titles.get(id).map(String::as_str).unwrap_or(id))?;
        }

        let title = c["title"].as_str().unwrap_or(case_id);
        let ministry = c["ministry"].as_str().unwrap_or("");
        let period = format!("{} 〜 {}", start.as_deref().unwrap_or("?"), end.as_deref().unwrap_or("?"));
        let stats_line = match (&result, opinions) {
            (Some(r), Some(n)) => format!("{ministry}・意見募集 {period}・結果公示 {r}（意見 {n} 件）"),
            (Some(r), None) => format!("{ministry}・意見募集 {period}・結果公示 {r}"),
            _ => format!("{ministry}・意見募集 {period}（結果未公示）"),
        };
        let law_links: Vec<String> = laws
            .iter()
            .map(|id| format!("[{}]({})", titles.get(id).map(String::as_str).unwrap_or(id), rel_link(&rel, &law_page(id))))
            .collect();
        let mut meta = String::from("## 意見募集の情報（一覧）\n\n| 項目 | 内容 |\n|---|---|\n");
        meta.push_str(&format!("| 所管 | {} |\n", cell(format!("{ministry} {}", c["responsible_office"].as_str().unwrap_or("")).trim())));
        meta.push_str(&format!("| 募集期間 | {period} |\n"));
        if let Some(r) = &result {
            meta.push_str(&format!("| 結果公示 | {r} |\n"));
        }
        if let Some(n) = opinions {
            meta.push_str(&format!("| 意見数 | {n} 件 |\n"));
        }
        meta.push_str(&format!("| 対象法令 | {} |\n", law_links.join("、")));
        meta.push_str(&format!("| 原文 | [lawrenceanum のパブコメ]({}/#/pubcomment/{case_id}) |\n", source.base()));

        let narrative = previous.as_ref().map(|p| first_llm_block(&p.body)).unwrap_or_default();
        let narrative_block = if narrative.is_empty() {
            format!("{LLM_BEGIN}\n{LLM_END}")
        } else {
            format!("{LLM_BEGIN}\n{narrative}\n{LLM_END}")
        };
        let body = format!("\n# {title}\n\n## 概要と結果\n\n{narrative_block}\n\n<!-- lawpub:begin meta -->\n{meta}<!-- lawpub:end meta -->\n");
        // description は LLM が書いた 1 行を優先し、無ければ募集状況にする。
        let description = match previous.as_ref().map(|p| p.get_str("description").to_string()) {
            Some(d) if !d.is_empty() && previous.as_ref().map(|p| p.get_str("stats")) != Some(d.as_str()) => d,
            _ => stats_line.clone(),
        };
        let mut fm = vec![
            ("type".into(), json!("pubcomment")),
            ("title".into(), json!(title)),
            ("description".into(), json!(description)),
            ("stats".into(), json!(stats_line)),
            ("timestamp".into(), json!(now_rfc3339())),
            ("date".into(), json!(result.clone().or(start.clone()))),
            ("case_id".into(), json!(case_id)),
            ("ministry".into(), json!(ministry)),
            ("status".into(), json!(status)),
            ("reception_start".into(), json!(start)),
            ("reception_end".into(), json!(end)),
            ("result_date".into(), json!(result)),
            ("opinion_count".into(), json!(opinions)),
            ("laws".into(), json!(laws)),
            ("tags".into(), previous.as_ref().and_then(|p| p.get("tags").cloned()).unwrap_or(json!([]))),
            ("render_version".into(), json!(RENDER_VERSION)),
        ];
        if let Some(v) = previous.as_ref().and_then(|p| p.get("llm_result_date").cloned()) {
            fm.push(("llm_result_date".into(), v));
        }
        Page { frontmatter: fm, body }.write(&path)?;
        stats.written += 1;
    }
    Ok(stats)
}

/// パブコメの引用単位 (抽出テキストのある添付ごと)。
pub fn pubcomment_units(doc: &Value) -> Vec<Unit> {
    let case_id = doc["case_id"].as_str().unwrap_or("");
    doc["attachments"]
        .as_array()
        .into_iter()
        .flatten()
        .enumerate()
        .filter_map(|(i, a)| {
            let text = a["extracted_text"].as_str().filter(|t| !t.trim().is_empty())?;
            Some(Unit {
                reference: format!("{KIND_PUBCOMMENT}:{case_id}#att{i}"),
                url: a["url"].as_str().unwrap_or("").to_string(),
                speaker: a["name"].as_str().or(a["label"].as_str()).map(String::from),
                group: None,
                position: None,
                text: text.to_string(),
            })
        })
        .collect()
}

/// 結果文書 (意見の要旨と考え方) らしい添付か。
fn looks_like_result(text: &str) -> bool {
    let head: String = text.chars().take(1_500).collect();
    head.contains("考え方") || (head.contains("意見") && head.contains("結果"))
}

/// 結果が公示されたのに未要約 (または結果公示日が変わった) 案件を、新しい順に最大 `max` 件タスクにする。
pub fn plan_tasks(source: &Source, wiki: &Path, work: &Path, max: usize) -> Result<Vec<Task>> {
    if max == 0 {
        return Ok(Vec::new());
    }
    let mut pending: Vec<(String, PathBuf, Page)> = Vec::new();
    for path in walk_md(wiki, "pubcomments") {
        let page = Page::read(&path)?;
        let Some(result) = page.get("result_date").and_then(Value::as_str).map(String::from) else { continue };
        if page.get("llm_result_date").and_then(Value::as_str) == Some(result.as_str()) {
            continue;
        }
        pending.push((result, path, page));
    }
    pending.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));

    let mut tasks = Vec::new();
    for (result, path, mut page) in pending {
        if tasks.len() >= max {
            break;
        }
        let case_id = page.get_str("case_id").to_string();
        let doc = match source.get_json(&format!("pubcomment/{case_id}.json")) {
            Ok(Some(d)) => d,
            Ok(None) => continue,
            Err(e) => {
                tracing::warn!("wiki-plan: パブコメ {case_id}: {e:#}");
                continue;
            }
        };
        let units = pubcomment_units(&doc);
        if units.is_empty() {
            // 引用できる文書が無い (結果が本文 HTML のみ等)。表の情報だけで足りる。
            page.set("llm_result_date", json!(result));
            page.write(&path)?;
            continue;
        }
        let dir = work.join("docs").join(KIND_PUBCOMMENT);
        std::fs::create_dir_all(&dir)?;
        std::fs::write(dir.join(format!("{}.json", file_safe(&case_id))), serde_json::to_vec(&doc)?)?;

        // 結果文書を優先して詰める。
        let mut ordered: Vec<&Unit> = units.iter().filter(|u| looks_like_result(&u.text)).collect();
        ordered.extend(units.iter().filter(|u| !looks_like_result(&u.text)));
        let mut total = 0usize;
        let mut excerpts = Vec::new();
        for u in ordered {
            let cap = if looks_like_result(&u.text) { RESULT_ATTACHMENT_CHARS } else { OTHER_ATTACHMENT_CHARS };
            let chars: Vec<char> = u.text.chars().collect();
            let remaining = MAX_CHARS_PER_CASE.saturating_sub(total);
            if remaining < 200 {
                break;
            }
            let n = cap.min(chars.len()).min(remaining);
            let mut text: String = chars[..n].iter().collect();
            if n < chars.len() {
                text.push('…');
            }
            total += n;
            excerpts.push(json!({"ref": u.reference, "url": u.url, "label": u.speaker, "text": text}));
        }

        let laws: Vec<TaskLaw> = page
            .get("laws")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|v| v.as_str())
            .map(|id| {
                let _ = ensure_law_synthesis(wiki, id);
                let lp = law_page(id);
                let title = Page::read(&wiki.join(&lp)).map(|p| p.get_str("title").to_string()).unwrap_or_default();
                TaskLaw { law_id: id.to_string(), title, page: lp }
            })
            .collect();
        let source_rel = format!("sources/pubcomment_{}.json", file_safe(&case_id));
        std::fs::create_dir_all(work.join("sources"))?;
        let bundle = json!({
            "kind": "pubcomment",
            "id": case_id,
            "title": page.get_str("title"),
            "ministry": page.get_str("ministry"),
            "legal_basis": doc["legal_basis"],
            "command_title": doc["command_title"],
            "reception_start": page.get("reception_start"),
            "reception_end": page.get("reception_end"),
            "result_date": result,
            "opinion_count": page.get("opinion_count"),
            "laws": laws,
            "excerpts": excerpts,
        });
        std::fs::write(work.join(&source_rel), serde_json::to_string_pretty(&bundle)?)?;
        tasks.push(Task {
            key: format!("{KIND_PUBCOMMENT}:{case_id}"),
            kind: KIND_PUBCOMMENT.into(),
            id: case_id.clone(),
            date: result.clone(),
            title: page.get_str("title").to_string(),
            page: rel_path(wiki, &path),
            source: source_rel,
            laws,
            people: Vec::new(),
            committee: None,
            created: Vec::new(),
        });
    }
    Ok(tasks)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn japanese_dates_and_related_law_names() {
        assert_eq!(parse_jp_date("2026年9月4日").as_deref(), Some("2026-09-04"));
        assert_eq!(parse_jp_date("2026年5月26日18時0分").as_deref(), Some("2026-05-26"));
        assert!(parse_jp_date("").is_none());
        let m: HashMap<String, String> =
            [("雇用保険法", "L1"), ("労働基準法", "L2")].iter().map(|(a, b)| (a.to_string(), b.to_string())).collect();
        assert_eq!(related_laws("○雇用保険法", &m), vec!["L1"]);
        assert_eq!(related_laws("雇用保険法及び労働基準法", &m), vec!["L1", "L2"]);
        assert!(related_laws("-", &m).is_empty());
    }

    #[test]
    fn sync_and_plan_pubcomment_with_result_document() {
        let root = temp_dir("pubcomment");
        let public = root.join("public");
        let wiki = root.join("wiki");
        let work = root.join("work");
        let write = |rel: &str, v: Value| {
            let p = public.join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, serde_json::to_vec(&v).unwrap()).unwrap();
        };
        write("pubcomment/index.json", json!({"cases": [
            {"case_id": "P1", "title": "雇用保険法施行規則の一部を改正する省令案", "ministry": "厚生労働省",
             "related_law_name": "雇用保険法", "status": "closed", "reception_start": "2026年4月27日",
             "reception_end": "2026年5月26日18時0分", "result_published": "2026年9月4日", "opinion_count": 4},
            {"case_id": "P2", "title": "関係ない案", "related_law_name": null, "status": "open"}
        ]}));
        write("pubcomment/P1.json", json!({"case_id": "P1", "legal_basis": "雇用保険法第一条", "attachments": [
            {"name": "結果.pdf", "url": "https://x/r", "extracted_text": "意見募集の結果について\n御意見に対する厚生労働省の考え方\n要件を明確化します。"},
            {"name": "空.pdf", "url": "https://x/e", "extracted_text": ""}
        ]}));
        let source = Source::new(&public.display().to_string()).unwrap();
        let unique: HashMap<String, String> = [("雇用保険法".to_string(), "L1".to_string())].into();
        let titles: HashMap<String, String> = [("L1".to_string(), "雇用保険法".to_string())].into();
        let st = sync(&source, &wiki, &unique, &titles).unwrap();
        assert_eq!((st.cases, st.written), (1, 1));
        let page = Page::read(&wiki.join("pubcomments/P1.md")).unwrap();
        assert_eq!(page.get_str("result_date"), "2026-09-04");
        assert_eq!(page.get_str("description"), "厚生労働省・意見募集 2026-04-27 〜 2026-05-26・結果公示 2026-09-04（意見 4 件）");
        assert!(page.body.contains("[雇用保険法](../laws/L1.md)"));
        assert!(wiki.join("laws/L1.md").exists());

        std::fs::create_dir_all(&work).unwrap();
        let tasks = plan_tasks(&source, &wiki, &work, 3).unwrap();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].kind, "pubcomment");
        let bundle: Value = serde_json::from_slice(&std::fs::read(work.join(&tasks[0].source)).unwrap()).unwrap();
        assert_eq!(bundle["excerpts"][0]["ref"], "pubcomment:P1#att0");
        assert_eq!(bundle["excerpts"].as_array().unwrap().len(), 1, "空の添付は引用単位にしない");

        // LLM の記述は描き直しでも残り、要約済みなら再度タスクにしない。
        let mut page = Page::read(&wiki.join("pubcomments/P1.md")).unwrap();
        page.body = page.body.replace(&format!("{LLM_BEGIN}\n{LLM_END}"), &format!("{LLM_BEGIN}\n要約[^1]\n{LLM_END}"));
        page.set("llm_result_date", json!("2026-09-04"));
        page.set("render_version", json!(0));
        page.write(&wiki.join("pubcomments/P1.md")).unwrap();
        sync(&source, &wiki, &unique, &titles).unwrap();
        let page = Page::read(&wiki.join("pubcomments/P1.md")).unwrap();
        assert_eq!(first_llm_block(&page.body), "要約[^1]");
        assert!(plan_tasks(&source, &wiki, &work, 3).unwrap().is_empty());
        std::fs::remove_dir_all(root).ok();
    }
}
