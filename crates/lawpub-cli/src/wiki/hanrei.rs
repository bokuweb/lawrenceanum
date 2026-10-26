//! 新規判例と件数を制限した過去判例を法令と結んで Wiki に取り込む。

use super::*;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

pub const KIND: &str = "hanrei";
pub const UNKNOWN: &str = "確認できる資料がありません。";
pub const PROMPT_VERSION: &str = "hanrei-rag-v4";
const INTAKE: &str = ".lawpub/hanrei-intake.json";

/// 判例自身の引用を画面上の原文と照合するためのデータ。法令や別事件の引用は混ぜない。
pub fn explanation_citations(page: &Page) -> Vec<Value> {
    let Some(id) = page.get("case_id").and_then(Value::as_u64) else {
        return Vec::new();
    };
    let text = first_llm_block(&page.body);
    let prefix = format!("hanrei:{id}#");
    check::citations_in(&text)
        .into_iter()
        .filter_map(|c| {
            let field = match c.reference.strip_prefix(&prefix)? {
                "holding" => "holding",
                "summary" => "summary",
                "body" => "body_text",
                _ => return None,
            };
            Some(json!({"label":c.label,"reference":c.reference,"field":field,"quote":c.quote}))
        })
        .collect()
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Intake {
    initialized_at: Option<String>,
    seen: BTreeSet<u64>,
    pending: BTreeMap<u64, Pending>,
    #[serde(default)]
    backfill_cursor: Option<u64>,
    #[serde(default)]
    daily: Daily,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Daily {
    date: String,
    new_planned: usize,
    backfill_planned: usize,
    new_probes: usize,
    backfill_probes: usize,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Pending {
    first_seen: String,
    attempts: usize,
    status: String,
    #[serde(default)]
    origin: String,
    #[serde(default)]
    next_attempt: Option<String>,
    #[serde(default)]
    selection_reason: String,
    #[serde(default)]
    refresh: bool,
}

impl Intake {
    fn load(wiki: &Path) -> Result<Self> {
        let path = wiki.join(INTAKE);
        if path.exists() {
            Ok(serde_json::from_slice(&std::fs::read(path)?)?)
        } else {
            Ok(Self::default())
        }
    }

    fn save(&self, wiki: &Path) -> Result<()> {
        let path = wiki.join(INTAKE);
        std::fs::create_dir_all(path.parent().unwrap())?;
        let tmp = path.with_extension("tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(self)?)?;
        std::fs::rename(tmp, path)?;
        Ok(())
    }

    fn observe(&mut self, index: &Value, today: &str) -> Result<()> {
        let cases = index["cases"]
            .as_array()
            .context("判例索引に cases がありません")?;
        anyhow::ensure!(
            index["count"].as_u64() == Some(cases.len() as u64),
            "判例索引の件数が不一致です"
        );
        anyhow::ensure!(!cases.is_empty(), "空の判例索引では基準を更新できません");
        let ids = cases
            .iter()
            .map(|c| {
                c["case_id"]
                    .as_u64()
                    .filter(|id| *id > 0)
                    .context("判例索引の case_id が不正です")
            })
            .collect::<Result<BTreeSet<_>>>()?;
        anyhow::ensure!(ids.len() == cases.len(), "判例索引に ID の重複があります");
        if self.initialized_at.is_none() {
            self.initialized_at = Some(today.into());
            self.seen = ids;
        } else {
            for id in ids {
                if self.seen.insert(id) {
                    self.pending.insert(
                        id,
                        Pending {
                            first_seen: today.into(),
                            attempts: 0,
                            status: "pending".into(),
                            ..Pending::default()
                        },
                    );
                }
            }
        }
        Ok(())
    }
}

pub(crate) fn snapshot_path(wiki: &Path, id: u64) -> PathBuf {
    wiki.join(format!(".lawpub/hanrei-sources/{id}.json"))
}

pub fn units(bundle: &Value) -> Vec<Unit> {
    enforcements::units(bundle)
}

/// 引用の正本は Wiki 内に固定した原資料。配信の更新や削除によらず検証できる。
pub fn unit(wiki: &Path, reference: &str) -> Result<Option<Unit>> {
    let Some(id) = reference
        .strip_prefix("hanrei:")
        .and_then(|s| s.split_once('#'))
        .and_then(|(id, _)| id.parse::<u64>().ok())
    else {
        return Ok(None);
    };
    let path = snapshot_path(wiki, id);
    if !path.exists() {
        return Ok(None);
    }
    let bundle: Value = serde_json::from_slice(&std::fs::read(path)?)?;
    Ok(units(&bundle)
        .into_iter()
        .find(|u| u.reference == reference))
}

fn excerpt(out: &mut Vec<Value>, id: u64, key: &str, url: &str, text: &str, scope: &str) {
    if !text.trim().is_empty() && (url.starts_with("https://") || url.starts_with("http://")) {
        out.push(json!({"ref":format!("hanrei:{id}#{key}"),"url":url,"text":text,"scope":scope}));
    }
}

/// 離れた抜粋を連結せず、それぞれの原文位置を残す。引用の正本は全文の snapshot。
fn prompt_bundle(bundle: &Value) -> Value {
    let mut prompt = bundle.clone();
    let mut budget = 36_000usize;
    let mut selected = Vec::new();
    let mut total = 0;
    let mut included = 0;
    let mut partial = Vec::new();
    let mut omitted = Vec::new();
    for excerpt in bundle["excerpts"].as_array().into_iter().flatten() {
        let reference = excerpt["ref"].as_str().unwrap_or("");
        let chars: Vec<char> = excerpt["text"].as_str().unwrap_or("").chars().collect();
        let count = chars.len();
        total += count;
        let cap = if reference.ends_with("#holding") {
            4_000
        } else if reference.ends_with("#summary") {
            6_000
        } else if reference.ends_with("#body") {
            22_000
        } else {
            4_000
        };
        let take = count.min(cap).min(budget);
        if take == 0 {
            if count > 0 {
                omitted.push(reference);
            }
            continue;
        }
        let ranges = if take == count {
            vec![(0, count)]
        } else {
            partial.push(reference);
            let head = take / 4;
            let middle = take / 4;
            let tail = take - head - middle;
            let middle_start = ((count - middle) / 2).max(head).min(count - tail - middle);
            vec![
                (0, head),
                (middle_start, middle_start + middle),
                (count - tail, count),
            ]
        };
        for (start, end) in ranges.into_iter().filter(|(s, e)| s < e) {
            let mut fragment = excerpt.clone();
            fragment["text"] = json!(chars[start..end].iter().collect::<String>());
            fragment["fragment"] = json!({"start_char":start,"end_char":end,"total_chars":count});
            selected.push(fragment);
        }
        budget -= take;
        included += take;
    }
    prompt["excerpts"] = json!(selected);
    prompt["summary_input"] = json!({
        "total_chars":total,"included_chars":included,"partial_refs":partial,"omitted_refs":omitted,
        "selection":"資料ごとの文字数上限。長い資料は冒頭・中盤・末尾の離れた抜粋（末尾を多めに収録）"
    });
    prompt["source_truncated"] = json!(included < total);
    // 識別情報と根拠を分け、予算外の本文や重複した要旨を入力しない。
    if let Some(case) = prompt["case"].as_object_mut() {
        for field in ["body_text", "holding", "summary", "body_citations"] {
            case.remove(field);
        }
    }
    prompt
}

fn official_summary(bundle: &Value) -> String {
    let mut section = String::from("## 裁判所の公開情報（原文）\n\n裁判所が公開した文章です。AIによる要約・解説とは区別して掲載しています。\n\n");
    for (key, title) in [("holding", "判示事項"), ("summary", "裁判要旨")] {
        section.push_str(&format!("### {title}\n\n"));
        let text = bundle["case"][key].as_str().unwrap_or("").trim();
        if text.is_empty() {
            section.push_str("取得した公開データには記載がありません。\n\n");
        } else {
            for line in text.lines() {
                let escaped: String = line
                    .replace('&', "&amp;")
                    .replace('<', "&lt;")
                    .replace('>', "&gt;")
                    .chars()
                    .flat_map(|c| {
                        if "\\`*_[]#|".contains(c) {
                            vec!['\\', c]
                        } else {
                            vec![c]
                        }
                    })
                    .collect();
                section.push_str(&format!("> {escaped}\n"));
            }
            section.push('\n');
        }
    }
    if let Ok(url) = url::Url::parse(bundle["case"]["detail_url"].as_str().unwrap_or("")) {
        if matches!(url.scheme(), "https" | "http") {
            section.push_str(&format!("[出典：裁判所の裁判例情報](<{url}>)\n"));
        }
    }
    section
}

fn bundle(
    source: &Source,
    doc: &Value,
    resolver: &linking::LawRefResolver,
    titles: &BTreeMap<String, String>,
    today: &str,
) -> Result<Value> {
    let id = doc["case_id"].as_u64().context("判例 ID がありません")?;
    let s = |k: &str| doc[k].as_str().unwrap_or("");
    let mut excerpts = Vec::new();
    excerpt(
        &mut excerpts,
        id,
        "holding",
        s("detail_url"),
        s("holding"),
        "裁判所の判示事項",
    );
    excerpt(
        &mut excerpts,
        id,
        "summary",
        s("detail_url"),
        s("summary"),
        "裁判所の裁判要旨",
    );
    excerpt(
        &mut excerpts,
        id,
        "body",
        s("pdf_url"),
        s("body_text"),
        "判決PDFの機械抽出本文。文字・読み順の誤りがあり得る",
    );
    // 関連条文だけを根拠に判例の要約を作らない。
    let has_case_text = !excerpts.is_empty();
    let resolved = linking::resolve_hanrei_laws(doc, resolver, titles, |law| {
        source.get_json(&format!("laws/{law}/versions.json"))
    })?;
    let mut laws = Vec::new();
    for law in resolved.laws.into_iter().take(30) {
        let mut info = serde_json::to_value(&law)?;
        info["relation"] = json!("条文への言及（適用法条との同定は未確認）");
        if has_case_text {
            for version in &law.versions {
                if !version.body_available {
                    continue;
                }
                let Some(document) = source.get_json(&format!(
                    "laws/{}/revisions/{}.json",
                    law.law_id, version.revision_id
                ))?
                else {
                    continue;
                };
                // A mismatched response must never become evidence for a historical version.
                if document["revision_id"].as_str() != Some(&version.revision_id) {
                    continue;
                }
                let scope = format!("法令版候補 {}（施行日 {}、選定区分 {}）。根拠: {}。適用法条・経過措置は未確認。{}", version.revision_id, version.effective_date, law.basis, law.evidence.iter().map(|e| e.text.as_str()).collect::<Vec<_>>().join(" / "), linking::HANREI_LAW_NOTICE);
                for a in document["articles"].as_array().into_iter().flatten() {
                    let refs = resolver.article_refs(&format!(
                        "{}{}",
                        law.title,
                        a["article_no"].as_str().unwrap_or("")
                    ));
                    let Some((_, number)) = refs
                        .iter()
                        .find(|(l, n)| l == &law.law_id && law.articles.contains(n))
                    else {
                        continue;
                    };
                    let text = a["paragraphs"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(|p| p["text"].as_str())
                        .collect::<Vec<_>>()
                        .join("\n");
                    let suffix = version
                        .revision_id
                        .strip_prefix(&format!("{}_", law.law_id))
                        .unwrap_or(&version.revision_id);
                    let url = format!("https://laws.e-gov.go.jp/law/{}/{}", law.law_id, suffix);
                    excerpt(
                        &mut excerpts,
                        id,
                        &format!("law-{}-{number}-{}", law.law_id, version.revision_id),
                        &url,
                        &text,
                        &scope,
                    );
                }
            }
        }
        laws.push(info);
    }
    let scope = if s("body_text").trim().is_empty() {
        "判示事項・裁判要旨（判決全文未収録）"
    } else {
        "判決PDFの機械抽出本文・判示事項・裁判要旨"
    };
    let mut result = json!({"kind":KIND,"case_id":id,"case":doc,"laws":laws,"excerpts":excerpts,"has_case_text":has_case_text,"scope":scope,"captured_on":today,"summary_format_version":4,"prompt_version":PROMPT_VERSION,"generation_model":std::env::var("WIKI_MODEL").unwrap_or_else(|_| "unknown".into())});
    result["summary_input"] = prompt_bundle(&result)["summary_input"].clone();
    result["source_fingerprint"] = json!(hex::encode(Sha256::digest(serde_json::to_vec(&result)?)));
    Ok(result)
}

fn write_page(wiki: &Path, source: &Source, bundle: &Value, first_seen: &str) -> Result<Task> {
    let id = bundle["case_id"].as_u64().unwrap();
    let doc = &bundle["case"];
    let s = |k: &str| doc[k].as_str().unwrap_or("");
    let title = format!("{} — {} {}", s("case_name"), s("court"), s("case_number"));
    let rel = format!("hanrei/{id}.md");
    let mut created = Vec::new();
    let mut laws = Vec::new();
    let mut links = Vec::new();
    for law in bundle["laws"].as_array().into_iter().flatten() {
        let law_id = law["law_id"].as_str().unwrap();
        let title = law["title"].as_str().unwrap();
        if ensure_law_page(wiki, source.base(), law_id, title)? {
            created.push(law_page(law_id));
        }
        let article_numbers = law["articles"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>();
        let articles = article_numbers
            .iter()
            .map(|article| format!("[{article}](#/hanrei/{id}/laws/{law_id}/{article})"))
            .collect::<Vec<_>>()
            .join("、");
        links.push(format!(
            "[{}]({})（参照条番号: {}）",
            cell(title),
            rel_link(&rel, &law_page(law_id)),
            articles
        ));
        let evidence = law["evidence"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|v| v["text"].as_str())
            .collect::<Vec<_>>()
            .join(" / ");
        let versions = law["versions"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|v| {
                let revision = v["revision_id"].as_str()?;
                revision.strip_prefix(&format!("{law_id}_"))?;
                Some((v, revision))
            })
            .flat_map(|(v, revision)| {
                let query = url::form_urlencoded::Serializer::new(String::new())
                    .append_pair("revision", revision)
                    .finish();
                article_numbers.iter().map(move |article| format!(
                    "[条番号 {article}・版候補 {}](#/hanrei/{id}/laws/{law_id}/{article}?{query}){}",
                    cell(revision),
                    if v["body_available"] == false { "（本文未収録）" } else { "" }
                ))
            })
            .collect::<Vec<_>>()
            .join("、");
        links.push(format!(
            "{}（根拠: {}{}）",
            if versions.is_empty() {
                "過去版を特定できません"
            } else {
                &versions
            },
            cell(&evidence),
            if law["incomplete"] == true {
                " / 収録範囲に不足あり"
            } else {
                ""
            }
        ));
        laws.push(TaskLaw {
            law_id: law_id.into(),
            title: title.into(),
            page: law_page(law_id),
        });
    }
    let path = wiki.join(&rel);
    if !path.exists() {
        created.push(rel.clone());
    }
    let mut page = Page::default();
    for (k, v) in [
        ("type", json!(KIND)),
        ("title", json!(title)),
        ("description", json!("")),
        ("tags", json!([])),
        ("case_id", json!(id)),
        ("date", json!(first_seen)),
        ("judged_on", doc["judged_on"].clone()),
        ("court", doc["court"].clone()),
        ("case_number", doc["case_number"].clone()),
        ("prompt_version", bundle["prompt_version"].clone()),
        ("resource", doc["detail_url"].clone()),
        (
            "laws",
            json!(laws.iter().map(|l| &l.law_id).collect::<Vec<_>>()),
        ),
        ("referenced_articles", bundle["laws"].clone()),
        ("source_scope", bundle["scope"].clone()),
        ("source_fingerprint", bundle["source_fingerprint"].clone()),
        (
            "summary_format_version",
            bundle["summary_format_version"].clone(),
        ),
        ("summary_input", bundle["summary_input"].clone()),
    ] {
        page.set(k, v);
    }
    page.body = format!("# {}\n\n{LLM_BEGIN}\n{LLM_END}\n\n<!-- lawpub:begin meta -->\n## 裁判例の情報\n\n| 項目 | 内容 |\n|---|---|\n| 事件番号 | {} |\n| 裁判所 | {} |\n| 裁判日 | {} |\n| 結果 | {} |\n| 新規取得を確認した日 | {first_seen} |\n| 収録範囲 | {} |\n| 原文 | [裁判所]({}) |\n| 関連法令 | {} |\n\n条文リンクは機械抽出した版の候補です。本文の出来事の日付・改正前の記載、なければ裁判日を参照しています。適用法条・経過措置は未確認です。過去版未収録の場合は現行版で代用しません。\n<!-- lawpub:end meta -->\n",cell(&title),cell(s("case_number")),cell(s("court")),cell(s("judged_on")),cell(s("result")),bundle["scope"].as_str().unwrap_or(""),s("detail_url"),links.join("、"));
    page.body = page.body.replacen(
        LLM_BEGIN,
        &format!(
            "以下はAIによる要約・解説です。引用先と収録範囲を確認してください。\n\n{LLM_BEGIN}"
        ),
        1,
    );
    page.body.push_str(&format!(
        "\n<!-- lawpub:begin official-summary -->\n{}<!-- lawpub:end official-summary -->\n",
        official_summary(bundle)
    ));
    page.write(&path)?;
    Ok(Task {
        key: format!("hanrei:{id}"),
        kind: KIND.into(),
        id: id.to_string(),
        date: first_seen.into(),
        title,
        page: rel,
        source: format!("sources/hanrei-{id}.json"),
        laws,
        people: vec![],
        committee: None,
        created,
        person: None,
    })
}

/// Explicit, bounded historical intake. Never reset the normal new-case baseline.
pub fn queue_cases(source: &Source, wiki: &Path, ids: &[u64], today: &str) -> Result<()> {
    queue_cases_with_refresh(source, wiki, ids, today, false)
}

pub fn queue_cases_with_refresh(
    source: &Source,
    wiki: &Path,
    ids: &[u64],
    today: &str,
    refresh: bool,
) -> Result<()> {
    let selected: BTreeSet<_> = ids.iter().copied().collect();
    anyhow::ensure!(
        !selected.is_empty() && selected.len() <= 10,
        "過去判例の指定は1〜10件です"
    );
    anyhow::ensure!(
        selected.len() == ids.len() && !selected.contains(&0),
        "判例IDが不正または重複しています"
    );
    let index = source
        .get_json("hanrei/index.json")?
        .context("判例索引がありません")?;
    let mut intake = Intake::load(wiki)?;
    intake.observe(&index, today)?;
    let published: BTreeSet<_> = index["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|c| c["case_id"].as_u64())
        .collect();
    anyhow::ensure!(
        selected.is_subset(&published),
        "公開索引にない判例IDが含まれています"
    );
    let mut added = 0;
    for id in selected {
        let path = wiki.join(format!("hanrei/{id}.md"));
        if path.exists() {
            let page = Page::read(&path)?;
            if !refresh && ready(&page) && source_matches(wiki, &page)? {
                continue;
            }
        }
        if refresh {
            intake.pending.remove(&id);
        }
        if let std::collections::btree_map::Entry::Vacant(entry) = intake.pending.entry(id) {
            entry.insert(Pending {
                first_seen: today.into(),
                status: "pending".into(),
                origin: "manual".into(),
                refresh,
                ..Pending::default()
            });
            added += 1;
        }
    }
    intake.save(wiki)?;
    eprintln!("wiki-queue-hanrei: {added} historical case(s) queued");
    Ok(())
}

/// 初回は索引だけを固定する。保留は消さず、取得試行の少ないものから巡回する。
#[cfg(test)]
pub fn plan_tasks(
    source: &Source,
    wiki: &Path,
    work: &Path,
    max: usize,
    max_probes: usize,
    today: &str,
) -> Result<Vec<Task>> {
    plan_tasks_with_backfill(source, wiki, work, max, 0, max_probes, today)
}

const PRIORITY_LAWS: [&str; 3] = ["325AC0000000144", "337AC0000000139", "322AC0000000049"];

/// 日付不明も最後に残す。和暦文字列をそのまま辞書順に並べない。
pub(crate) fn case_date(case: &Value) -> String {
    let raw = case["judged_on"].as_str().unwrap_or("");
    let text: String = raw
        .chars()
        .map(|c| match c {
            '０'..='９' => char::from_u32(c as u32 - '０' as u32 + '0' as u32).unwrap(),
            _ => c,
        })
        .collect();
    if chrono::NaiveDate::parse_from_str(&text, "%Y-%m-%d").is_ok() {
        return text;
    }
    for (era, base) in [
        ("令和", 2018),
        ("平成", 1988),
        ("昭和", 1925),
        ("大正", 1911),
        ("明治", 1867),
    ] {
        if let Some(rest) = text.strip_prefix(era) {
            if let Some((year, rest)) = rest.split_once('年') {
                if let Some((month, day)) = rest.split_once('月') {
                    let y = if year == "元" {
                        Some(1)
                    } else {
                        year.parse::<i32>().ok()
                    };
                    if let (Some(y), Ok(m), Ok(d)) =
                        (y, month.parse(), day.trim_end_matches('日').parse())
                    {
                        if y > 0 {
                            if let Some(date) = chrono::NaiveDate::from_ymd_opt(base + y, m, d) {
                                return date.to_string();
                            }
                        }
                    }
                }
            }
        }
    }
    String::new()
}

fn historical_candidates(
    source: &Source,
    wiki: &Path,
    index: &Value,
    intake: &Intake,
    limit: usize,
) -> Result<Vec<(u64, String)>> {
    if limit == 0 {
        return Ok(vec![]);
    }
    let pages = completed_pages(wiki)?;
    let completed: BTreeSet<_> = pages
        .iter()
        .filter_map(|(_, p)| p.get("case_id").and_then(Value::as_u64))
        .collect();
    let shared_articles: BTreeSet<_> = pages.iter().flat_map(|(_, p)| article_keys(p)).collect();
    let eligible = |id: u64| {
        intake.seen.contains(&id) && !completed.contains(&id) && !intake.pending.contains_key(&id)
    };
    let mut all: Vec<_> = index["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|c| Some((case_date(c), c["case_id"].as_u64()?)))
        .collect();
    all.sort_by(|a, b| b.cmp(a));
    let mut priority = BTreeSet::new();
    let mut shared = BTreeSet::new();
    let laws =
        std::env::var("WIKI_HANREI_PRIORITY_LAWS").unwrap_or_else(|_| PRIORITY_LAWS.join(","));
    for law in laws.split(',').map(str::trim).filter(|s| !s.is_empty()) {
        anyhow::ensure!(
            law.chars().all(|c| c.is_ascii_alphanumeric()),
            "優先法令IDが不正です"
        );
        if let Some(reverse) = source.get_json(&format!("links/law-to-hanrei/{law}.json"))? {
            for case in reverse["linked_cases"].as_array().into_iter().flatten() {
                let Some(id) = case["case_id"].as_u64() else {
                    continue;
                };
                priority.insert(id);
                if case["articles"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .any(|article| shared_articles.contains(&(law.into(), article.into())))
                {
                    shared.insert(id);
                }
            }
        }
    }
    let mut preferred: Vec<_> = all
        .iter()
        .filter(|(_, id)| priority.contains(id) && eligible(*id))
        .map(|(_, id)| *id)
        .collect();
    preferred.sort_by_key(|id| !shared.contains(id));
    let offset = intake
        .backfill_cursor
        .and_then(|id| all.iter().position(|(_, n)| *n == id))
        .map_or(0, |n| n + 1);
    let general: Vec<_> = all
        .iter()
        .cycle()
        .skip(offset)
        .take(all.len())
        .filter(|(_, id)| eligible(*id))
        .map(|(_, id)| *id)
        .collect();
    let mut selected = BTreeSet::new();
    let mut out = Vec::new();
    let (mut p, mut g) = (preferred.into_iter(), general.into_iter());
    while out.len() < limit {
        let next = if out.len() % 2 == 0 {
            p.by_ref()
                .find(|id| !selected.contains(id))
                .map(|id| (id, "priority_law"))
                .or_else(|| {
                    g.by_ref()
                        .find(|id| !selected.contains(id))
                        .map(|id| (id, "corpus_cursor"))
                })
        } else {
            g.by_ref()
                .find(|id| !selected.contains(id))
                .map(|id| (id, "corpus_cursor"))
                .or_else(|| {
                    p.by_ref()
                        .find(|id| !selected.contains(id))
                        .map(|id| (id, "priority_law"))
                })
        };
        let Some((id, reason)) = next else { break };
        selected.insert(id);
        out.push((
            id,
            if reason == "priority_law" && shared.contains(&id) {
                "shared_article"
            } else {
                reason
            }
            .into(),
        ));
    }
    Ok(out)
}

/// 状態や Markdown を書き換えず、候補と既存の保留を確認する。
pub fn preview(source: &Source, wiki: &Path, max_backfill: usize, today: &str) -> Result<Value> {
    let mut intake = Intake::load(wiki)?;
    let index = source
        .get_json("hanrei/index.json")?
        .context("判例索引がありません")?;
    intake.observe(&index, today)?;
    let candidates = historical_candidates(source, wiki, &index, &intake, max_backfill)?;
    Ok(
        json!({"date":today,"max_backfill":max_backfill,"candidates":candidates.iter().map(|(id, reason)| json!({"case_id":id,"reason":reason})).collect::<Vec<_>>(),"progress":progress(wiki)?}),
    )
}

pub fn progress(wiki: &Path) -> Result<Value> {
    let intake = Intake::load(wiki)?;
    let completed = completed_pages(wiki)?;
    let mut statuses = BTreeMap::<String, usize>::new();
    for pending in intake.pending.values() {
        *statuses.entry(pending.status.clone()).or_default() += 1;
    }
    Ok(
        json!({"schema_version":1,"initialized_at":intake.initialized_at,"known_cases":intake.seen.len(),"completed":completed.len(),"pending_new":intake.pending.values().filter(|p| p.origin != "backfill").count(),"pending_backfill":intake.pending.values().filter(|p| p.origin == "backfill").count(),"statuses":statuses,"daily":intake.daily}),
    )
}

/// 新規・手動の上限を維持し、過去分には独立した枠を追加する。
/// 自動処理では日ごとの枠と試行数を保存し、同日の再実行でも増量しない。
#[allow(clippy::too_many_arguments)]
pub fn plan_tasks_with_backfill(
    source: &Source,
    wiki: &Path,
    work: &Path,
    max: usize,
    max_backfill: usize,
    max_probes: usize,
    today: &str,
) -> Result<Vec<Task>> {
    let mut intake = Intake::load(wiki)?;
    let published = source.get_json("hanrei/index.json")?;
    if let Some(index) = &published {
        intake.observe(index, today)?;
    }
    if intake.initialized_at.is_none() {
        return Ok(vec![]);
    }
    if intake.daily.date != today {
        intake.daily = Daily {
            date: today.into(),
            ..Daily::default()
        };
    }
    let automatic = max_backfill > 0;
    let new_limit = if automatic {
        max.saturating_sub(intake.daily.new_planned)
    } else {
        max
    };
    let old_limit = max_backfill.saturating_sub(intake.daily.backfill_planned);
    // Missing sources must not trigger unbounded requests. Each kind has its own daily probe budget.
    let probe_limit = max_probes.min(80);
    let new_probes = if automatic {
        probe_limit.saturating_sub(intake.daily.new_probes)
    } else {
        max_probes
    };
    let old_probes = probe_limit.saturating_sub(intake.daily.backfill_probes);
    let due = |p: &Pending| p.next_attempt.as_deref().is_none_or(|date| date <= today);
    let mut queued: Vec<_> = intake
        .pending
        .iter()
        .filter(|(_, p)| due(p))
        .map(|(id, p)| {
            (
                p.attempts,
                p.first_seen.clone(),
                *id,
                p.origin == "backfill",
            )
        })
        .collect();
    queued.sort();
    let mut queue: Vec<_> = queued
        .iter()
        .filter(|q| !q.3)
        .take(new_probes)
        .map(|q| (q.2, String::new()))
        .collect();
    // Reserve half the historical probes for fresh candidates; retries cannot monopolize the batch.
    let retry: Vec<_> = queued
        .iter()
        .filter(|q| q.3)
        .take(old_probes.div_ceil(2))
        .map(|q| (q.2, "retry".into()))
        .collect();
    if old_limit > 0 {
        let fresh = if let Some(index) = &published {
            historical_candidates(
                source,
                wiki,
                index,
                &intake,
                old_probes.saturating_sub(retry.len()),
            )?
        } else {
            vec![]
        };
        let (mut r, mut f) = (retry.into_iter(), fresh.into_iter());
        loop {
            let pair = (r.next(), f.next());
            if pair == (None, None) {
                break;
            }
            for c in [pair.0, pair.1].into_iter().flatten() {
                queue.push(c);
            }
        }
    }
    intake.save(wiki)?;
    if new_limit + old_limit == 0 || queue.is_empty() {
        return Ok(vec![]);
    }
    let index = source
        .get_json("laws/index.json")?
        .context("法令索引がありません")?;
    let names: Vec<(String, String)> = index["laws"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|l| Some((l["law_id"].as_str()?.into(), l["title"].as_str()?.into())))
        .collect();
    let resolver = linking::LawRefResolver::new(&names)?;
    let titles = names.into_iter().collect();
    let mut tasks = Vec::new();
    let (mut new_count, mut old_count) = (0, 0);
    for (id, reason) in queue {
        let backfill = intake
            .pending
            .get(&id)
            .is_some_and(|p| p.origin == "backfill")
            || !reason.is_empty();
        if (backfill && old_count >= old_limit) || (!backfill && new_count >= new_limit) {
            continue;
        }
        let path = wiki.join(format!("hanrei/{id}.md"));
        if path.exists()
            && !intake.pending.get(&id).is_some_and(|p| p.refresh)
            && Page::read(&path)
                .is_ok_and(|p| ready(&p) && source_matches(wiki, &p).unwrap_or(false))
        {
            intake.pending.remove(&id);
            continue;
        }
        let pending = intake.pending.entry(id).or_insert_with(|| Pending {
            first_seen: today.into(),
            origin: "backfill".into(),
            selection_reason: reason.clone(),
            ..Pending::default()
        });
        let first_seen = Page::read(&path)
            .ok()
            .map(|p| p.get_str("date").to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| pending.first_seen.clone());
        if backfill {
            intake.daily.backfill_probes += 1;
            if reason == "corpus_cursor" {
                intake.backfill_cursor = Some(id);
            }
        } else {
            intake.daily.new_probes += 1;
        }
        pending.attempts += 1;
        if automatic || pending.origin == "backfill" {
            let delay = match pending.attempts {
                1 => 1,
                2 => 3,
                3 => 7,
                _ => 30,
            };
            let date = chrono::NaiveDate::parse_from_str(today, "%Y-%m-%d")?;
            pending.next_attempt = Some((date + chrono::Duration::days(delay)).to_string());
        }
        pending.status = "source_unavailable".into();
        let result = (|| -> Result<Option<Value>> {
            let Some(doc) = source.get_json(&format!("hanrei/{id}.json"))? else {
                return Ok(None);
            };
            anyhow::ensure!(
                doc["case_id"].as_u64() == Some(id),
                "判例の ID が不一致です"
            );
            bundle(source, &doc, &resolver, &titles, today).map(Some)
        })();
        let bundle = match result {
            Ok(Some(b)) => b,
            Ok(None) => continue,
            Err(e) => {
                tracing::warn!("wiki-plan: hanrei:{id}: {e:#} — 次回に再試行");
                continue;
            }
        };
        if bundle["has_case_text"] != true {
            pending.status = "no_case_text".into();
            continue;
        }
        // Keep both the previous page and its immutable source until finalize succeeds.
        let backup = work.join(format!("hanrei-backups/{id}"));
        if path.exists() && !backup.exists() {
            std::fs::create_dir_all(&backup)?;
            std::fs::copy(&path, backup.join("page.md"))?;
            let previous_source = snapshot_path(wiki, id);
            if previous_source.exists() {
                std::fs::copy(previous_source, backup.join("source.json"))?;
            }
        }
        let snapshot = snapshot_path(wiki, id);
        std::fs::create_dir_all(snapshot.parent().unwrap())?;
        std::fs::write(snapshot, serde_json::to_vec_pretty(&bundle)?)?;
        let task = write_page(wiki, source, &bundle, &first_seen)?;
        let dir = work.join("docs/hanrei");
        std::fs::create_dir_all(&dir)?;
        std::fs::write(dir.join(format!("{id}.json")), serde_json::to_vec(&bundle)?)?;
        let prompt = prompt_bundle(&bundle);
        std::fs::create_dir_all(work.join("sources"))?;
        std::fs::write(work.join(&task.source), serde_json::to_vec_pretty(&prompt)?)?;
        pending.status = "summary_pending".into();
        if backfill {
            old_count += 1;
            intake.daily.backfill_planned += 1;
        } else {
            new_count += 1;
            intake.daily.new_planned += 1;
        }
        tasks.push(task);
    }
    intake.save(wiki)?;
    Ok(tasks)
}

pub fn summary_errors(page: &Page) -> Vec<String> {
    let body = first_llm_block(&page.body);
    let mut errors = Vec::new();
    let prefix = format!(
        "hanrei:{}#",
        page.get("case_id").and_then(Value::as_u64).unwrap_or(0)
    );
    let citations = check::citations_in(&body);
    let case_citation = |c: &check::Citation| {
        ["holding", "summary", "body"]
            .iter()
            .any(|s| c.reference == format!("{prefix}{s}"))
    };
    if !citations.iter().any(case_citation) {
        errors.push("この判例の判示事項・裁判要旨・本文の引用が必要です".into());
    }
    if citations.iter().any(|c| !c.reference.starts_with(&prefix)) {
        errors.push("別の判例の引用をこの判例の根拠にできません".into());
    }
    let version = page
        .get("summary_format_version")
        .and_then(Value::as_u64)
        .unwrap_or(1);
    let enhanced = version >= 2;
    let headings = if version >= 3 {
        vec![
            "要約",
            "事案",
            "争点",
            "結論",
            "判断と理由",
            "射程・留意点",
            "法令との関係",
        ]
    } else if enhanced {
        vec!["要約", "事案", "争点", "判断と理由", "法令との関係"]
    } else {
        vec!["事案", "争点", "判断と理由", "法令との関係"]
    };
    for heading in headings {
        let section = body
            .split_once(&format!("## {heading}\n"))
            .map(|(_, s)| s.split("\n## ").next().unwrap_or(s))
            .unwrap_or("");
        let prose = section
            .lines()
            .filter(|l| !l.trim_start().starts_with("[^"))
            .collect::<Vec<_>>()
            .join("\n");
        if (prose.trim() != UNKNOWN || (enhanced && heading == "要約"))
            && !citations.iter().any(|c| {
                (!enhanced || case_citation(c)) && prose.contains(&format!("[^{}]", c.label))
            })
        {
            errors.push(format!(
                "「{heading}」にはこの判例の出典付きの説明{}が必要です",
                if heading == "要約" {
                    String::new()
                } else {
                    format!("、または「{UNKNOWN}」")
                }
            ));
        }
    }
    if version >= 4 {
        errors.extend(super::hanrei_rag::paragraph_errors(page));
    }
    errors
}

pub fn ready(page: &Page) -> bool {
    !page.get_str("description").trim().is_empty()
        && !page.get_str("source_fingerprint").is_empty()
        && page.get_str("source_fingerprint") == page.get_str("llm_source_fingerprint")
        && summary_errors(page).is_empty()
}

pub fn source_matches(wiki: &Path, page: &Page) -> Result<bool> {
    let Some(id) = page.get("case_id").and_then(Value::as_u64) else {
        return Ok(false);
    };
    let path = snapshot_path(wiki, id);
    if !path.exists() {
        return Ok(false);
    }
    let doc: Value = serde_json::from_slice(&std::fs::read(path)?)?;
    Ok(
        page.get("source_fingerprint") == doc.get("source_fingerprint")
            && page.get("source_scope") == doc.get("scope")
            && page.get("referenced_articles") == doc.get("laws")
            && page.get("summary_format_version") == doc.get("summary_format_version")
            && page.get("summary_input") == doc.get("summary_input")
            && page
                .get("judged_on")
                .is_none_or(|v| Some(v) == doc["case"].get("judged_on"))
            && page
                .get("court")
                .is_none_or(|v| Some(v) == doc["case"].get("court"))
            && (doc.get("summary_format_version").is_none()
                || replace_block(&page.body, "official-summary", &official_summary(&doc))
                    .is_some_and(|body| body == page.body))
            && page.get("laws")
                == Some(&json!(doc["laws"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|l| l["law_id"].clone())
                    .collect::<Vec<_>>())),
    )
}

pub fn completed_pages(wiki: &Path) -> Result<Vec<(String, Page)>> {
    let mut pages = Vec::new();
    for path in walk_md(wiki, "hanrei") {
        let page = Page::read(&path)?;
        if ready(&page) && source_matches(wiki, &page)? {
            pages.push((rel_path(wiki, &path), page));
        }
    }
    pages.sort_by(|a, b| {
        b.1.get_str("date")
            .cmp(a.1.get_str("date"))
            .then_with(|| a.0.cmp(&b.0))
    });
    Ok(pages)
}

/// 完了した ID は未完了一覧から外す。既知 ID の集合は維持し再投入を防ぐ。
pub fn finish(wiki: &Path, completed: &[Task]) -> Result<()> {
    if !wiki.join(INTAKE).exists() {
        return Ok(());
    }
    let mut intake = Intake::load(wiki)?;
    let before = serde_json::to_vec(&intake.pending)?;
    for task in completed.iter().filter(|t| t.kind == KIND) {
        if let Ok(id) = task.id.parse::<u64>() {
            intake.pending.remove(&id);
        }
    }
    for pending in intake.pending.values_mut() {
        if pending.status == "summary_pending" {
            pending.status = "summary_incomplete".into();
        }
    }
    if serde_json::to_vec(&intake.pending)? != before {
        intake.save(wiki)?;
    }
    Ok(())
}

pub fn references_law(page: &Page, law: &str) -> bool {
    page.get("laws")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .any(|v| v.as_str() == Some(law))
}

fn article_keys(page: &Page) -> BTreeSet<(String, String)> {
    page.get("referenced_articles")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|law| Some((law["law_id"].as_str()?, law["articles"].as_array()?)))
        .flat_map(|(law, articles)| {
            articles
                .iter()
                .filter_map(move |a| Some((law.to_string(), a.as_str()?.to_string())))
        })
        .collect()
}

#[derive(Debug, Serialize)]
pub struct CommonArticle {
    law_id: String,
    title: String,
    article: String,
}

#[derive(Debug, Serialize)]
pub struct RelatedCase {
    path: String,
    case_id: u64,
    title: String,
    judged_on: String,
    court: String,
    description: String,
    common_articles: Vec<CommonArticle>,
}

/// 同じ法令・条番号への言及という観測事実だけを結ぶ。先例関係は推定しない。
pub fn case_relations(cases: &[(String, Page)]) -> BTreeMap<String, Vec<RelatedCase>> {
    let mut result = BTreeMap::new();
    let keys: Vec<_> = cases.iter().map(|(_, page)| article_keys(page)).collect();
    let mut index: BTreeMap<_, Vec<usize>> = BTreeMap::new();
    for (i, refs) in keys.iter().enumerate() {
        for key in refs {
            index.entry(key.clone()).or_default().push(i);
        }
    }
    for (i, (rel, original)) in cases.iter().enumerate() {
        let mut related: BTreeMap<usize, Vec<_>> = BTreeMap::new();
        for key in &keys[i] {
            for &other in &index[key] {
                if other != i {
                    related.entry(other).or_default().push(key);
                }
            }
        }
        let mut related: Vec<_> = related.into_iter().collect();
        related.sort_by(|(a, ka), (b, kb)| {
            kb.len()
                .cmp(&ka.len())
                .then_with(|| cases[*a].0.cmp(&cases[*b].0))
        });
        let entries = related
            .into_iter()
            .take(10)
            .filter_map(|(other, common)| {
                let (target, page) = &cases[other];
                let common_articles = common
                    .iter()
                    .map(|(law_id, article)| {
                        let title = original
                            .get("referenced_articles")
                            .and_then(Value::as_array)
                            .into_iter()
                            .flatten()
                            .find(|l| l["law_id"].as_str() == Some(law_id))
                            .and_then(|l| l["title"].as_str())
                            .unwrap_or(law_id);
                        CommonArticle {
                            law_id: law_id.clone(),
                            title: title.to_string(),
                            article: article.clone(),
                        }
                    })
                    .collect();
                Some(RelatedCase {
                    path: target.trim_end_matches(".md").to_string(),
                    case_id: page.get("case_id").and_then(Value::as_u64)?,
                    title: page.get_str("title").to_string(),
                    judged_on: page.get_str("judged_on").to_string(),
                    court: page.get_str("court").to_string(),
                    description: page.get_str("description").to_string(),
                    common_articles,
                })
            })
            .collect();
        result.insert(rel.clone(), entries);
    }
    result
}

/// 保存済みの古い関連区間も、現在の公開対象だけを使って置き換える。
pub fn body_with_case_relations(rel: &str, original: &Page, related: &[RelatedCase]) -> String {
    let mut section = String::new();
    if !related.is_empty() {
        section.push_str("## 共通する参照条文を持つ判例\n\n同じ法令・条番号への言及を根拠に、共通条文の多い順に最大10件を表示します。先例関係・同じ争点や結論・適用法令の版の一致を示すものではありません。\n\n| 判例 | 裁判日・裁判所 | 共通する参照条文 | 概要 |\n|---|---|---|---|\n");
        for case in related {
            let articles = case
                .common_articles
                .iter()
                .map(|a| {
                    format!(
                        "[{} 第{}条](#/hanrei/{}/laws/{}/{})",
                        cell(&a.title),
                        a.article.replace('_', "の"),
                        original.get("case_id").and_then(Value::as_u64).unwrap_or(0),
                        a.law_id,
                        a.article
                    )
                })
                .collect::<Vec<_>>()
                .join("、");
            section.push_str(&format!(
                "| [{}]({}) | {} {} | {} | {} |\n",
                cell(&case.title),
                rel_link(rel, &format!("{}.md", case.path)),
                cell(&case.judged_on),
                cell(&case.court),
                articles,
                cell(&case.description)
            ));
        }
    }
    match replace_block(&original.body, "related-cases", &section) {
        Some(body) => body,
        None if !section.is_empty() => format!(
            "{}\n<!-- lawpub:begin related-cases -->\n{section}<!-- lawpub:end related-cases -->\n",
            original.body
        ),
        None => original.body.clone(),
    }
}

fn write_case_relations(wiki: &Path, cases: &[(String, Page)]) -> Result<()> {
    let relations = case_relations(cases);
    for (rel, original) in cases {
        let body = body_with_case_relations(rel, original, &relations[rel]);
        if body != original.body {
            let mut page = original.clone();
            page.body = body;
            page.write(&wiki.join(rel))?;
        }
    }
    Ok(())
}

pub fn write_backlinks(wiki: &Path, cases: &[(String, Page)]) -> Result<()> {
    write_case_relations(wiki, cases)?;
    for path in walk_md(wiki, "laws") {
        let mut page = Page::read(&path)?;
        let rel = rel_path(wiki, &path);
        let related: Vec<_> = cases
            .iter()
            .filter(|(_, c)| references_law(c, page.get_str("law_id")))
            .collect();
        let mut section = String::new();
        if !related.is_empty() {
            section.push_str("## 関連する判例\n\n条文への言及から関連付けた判例です。判決当時の適用条文は未同定です。\n\n| 裁判日 | 判例 | 概要 |\n|---|---|---|\n");
            for (target, c) in related {
                section.push_str(&format!(
                    "| {} | [{}]({}) | {} |\n",
                    cell(c.get_str("judged_on")),
                    cell(c.get_str("title")),
                    rel_link(&rel, target),
                    cell(c.get_str("description"))
                ));
            }
        }
        let body = match replace_block(&page.body, "hanrei", &section) {
            Some(body) => body,
            None if !section.is_empty() => format!(
                "{}\n<!-- lawpub:begin hanrei -->\n{section}<!-- lawpub:end hanrei -->\n",
                page.body
            ),
            None => continue,
        };
        if body != page.body {
            page.body = body;
            page.write(&path)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn daily_backfill_reserves_twenty_separate_slots_and_prevents_same_day_repeat() {
        let (source, wiki, work) = fixture();
        let src = Source::new(source.to_str().unwrap()).unwrap();
        let existing: Vec<_> = (1..=30).collect();
        index(&source, &existing);
        plan_tasks(&src, &wiki, &work, 0, 80, "2026-10-25").unwrap();
        let all: Vec<_> = (1..=34).collect();
        index(&source, &all);
        for id in &all {
            case(&source, *id);
        }
        let tasks = plan_tasks_with_backfill(&src, &wiki, &work, 3, 20, 80, "2026-10-25").unwrap();
        assert_eq!(tasks.len(), 23);
        let intake = Intake::load(&wiki).unwrap();
        assert_eq!(
            (intake.daily.new_planned, intake.daily.backfill_planned),
            (3, 20)
        );
        assert_eq!(
            tasks
                .iter()
                .filter(|t| t.id.parse::<u64>().unwrap() > 30)
                .count(),
            3
        );
        assert!(
            plan_tasks_with_backfill(&src, &wiki, &work, 3, 20, 80, "2026-10-25")
                .unwrap()
                .is_empty()
        );
        assert_eq!(Intake::load(&wiki).unwrap().seen.len(), 34);
        // A disabled historical budget cannot accidentally process the pending historical tasks.
        let manual = plan_tasks(&src, &wiki, &work, 3, 80, "2026-10-26").unwrap();
        assert_eq!(manual.len(), 3);
        assert!(manual.iter().all(|t| t.id.parse::<u64>().unwrap() > 30));
    }

    #[test]
    fn missing_sources_back_off_and_do_not_starve_fresh_candidates() {
        let (source, wiki, work) = fixture();
        let src = Source::new(source.to_str().unwrap()).unwrap();
        assert!(
            plan_tasks_with_backfill(&src, &wiki, &work, 0, 20, 4, "2026-10-25")
                .unwrap()
                .is_empty()
        );
        let intake = Intake::load(&wiki).unwrap();
        assert_eq!(intake.pending.len(), 2);
        assert!(intake
            .pending
            .values()
            .all(|p| p.attempts == 1 && p.next_attempt.as_deref() == Some("2026-10-26")));
        plan_tasks_with_backfill(&src, &wiki, &work, 0, 20, 4, "2026-10-26").unwrap();
        assert!(Intake::load(&wiki)
            .unwrap()
            .pending
            .values()
            .all(|p| p.attempts == 2 && p.next_attempt.as_deref() == Some("2026-10-29")));
        plan_tasks_with_backfill(&src, &wiki, &work, 0, 20, 4, "2026-10-27").unwrap();
        assert_eq!(Intake::load(&wiki).unwrap().daily.backfill_probes, 0);
        // Newly available historical candidates can proceed while retries are deferred.
        index(&source, &[100, 200, 300]);
        // This ID was not new at the baseline, so mark it known to simulate a historical addition.
        let mut intake = Intake::load(&wiki).unwrap();
        intake.seen.insert(300);
        intake.save(&wiki).unwrap();
        case(&source, 300);
        let tasks = plan_tasks_with_backfill(&src, &wiki, &work, 0, 20, 4, "2026-10-27").unwrap();
        assert_eq!(
            tasks.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(),
            vec!["300"]
        );
    }

    #[test]
    fn preview_is_read_only_and_interleaves_priority_with_corpus() {
        let (source, wiki, _) = fixture();
        let src = Source::new(source.to_str().unwrap()).unwrap();
        write_json(
            &source.join("links/law-to-hanrei/325AC0000000144.json"),
            &json!({"linked_cases":[{"case_id":100}]}),
        );
        let output = preview(&src, &wiki, 2, "2026-10-25").unwrap();
        assert_eq!(
            output["candidates"][0],
            json!({"case_id":100,"reason":"priority_law"})
        );
        assert_eq!(
            output["candidates"][1],
            json!({"case_id":200,"reason":"corpus_cursor"})
        );
        assert!(!wiki.join(INTAKE).exists());
        assert!(completed_pages(&wiki).unwrap().is_empty());
        assert_eq!(
            case_date(&json!({"judged_on":"平成３１年４月３０日"})),
            "2019-04-30"
        );
        assert_eq!(
            case_date(&json!({"judged_on":"令和元年5月1日"})),
            "2019-05-01"
        );
        assert_eq!(case_date(&json!({"judged_on":"不明"})), "");
    }

    #[test]
    fn legacy_intake_loads_and_public_progress_does_not_dump_baseline() {
        let (_, wiki, _) = fixture();
        write_json(
            &wiki.join(INTAKE),
            &json!({"initialized_at":"2026-10-24","seen":[1,2],"pending":{"3":{"first_seen":"2026-10-25","attempts":0,"status":"pending"}}}),
        );
        let intake = Intake::load(&wiki).unwrap();
        assert_ne!(intake.pending[&3].origin, "backfill");
        let report = progress(&wiki).unwrap();
        assert_eq!(report["known_cases"], 2);
        assert_eq!(report["pending_new"], 1);
        assert!(report.get("seen").is_none());
    }

    fn fixture() -> (PathBuf, PathBuf, PathBuf) {
        let root = temp_dir("hanrei");
        let source = root.join("public");
        let wiki = root.join("wiki");
        let work = root.join("work");
        write_json(
            &source.join("laws/index.json"),
            &json!({"laws":[{"law_id":"L1","title":"民法"}]}),
        );
        write_json(
            &source.join("laws/L1/current.json"),
            &json!({"revision_id":"L1_20260401_A1","articles":[{"article_id":"a709","article_no":"第七百九条","paragraphs":[{"text":"故意又は過失によって他人の権利を侵害した者は、これによって生じた損害を賠償する責任を負う。"}]}]}),
        );
        let current: Value =
            serde_json::from_slice(&std::fs::read(source.join("laws/L1/current.json")).unwrap())
                .unwrap();
        write_json(
            &source.join("laws/L1/revisions/L1_20260401_A1.json"),
            &current,
        );
        write_json(
            &source.join("laws/L1/versions.json"),
            &json!({"versions":[{"revision_id":"L1_20260401_A1","effective_date":"2026-04-01","body_available":true,"path":"laws/L1/revisions/L1_20260401_A1.json"}]}),
        );
        index(&source, &[100, 200]);
        (source, wiki, work)
    }

    #[test]
    fn wiki_uses_the_same_version_candidates_and_never_quotes_current_as_historical() {
        let (source, wiki, _) = fixture();
        let src = Source::new(&source.display().to_string()).unwrap();
        let resolver = linking::LawRefResolver::new(&[("L1".into(), "民法".into())]).unwrap();
        let titles = BTreeMap::from([("L1".into(), "民法".into())]);
        let doc = json!({"case_id":5,"judged_on":"令和8年10月1日","holding":"民法第709条による損害賠償請求について判断した。","detail_url":"https://www.courts.go.jp/hanrei/5/detail2/index.html","pdf_url":"https://www.courts.go.jp/a.pdf","body_text":"平成３０年４月１日付けの処分が争われた。"});
        let history = json!({"versions":[{"revision_id":"L1_20100401_A","effective_date":"2010-04-01","body_available":true,"path":"old.json"},{"revision_id":"L1_20260401_A1","effective_date":"2026-04-01","body_available":true,"path":"new.json"}]});
        write_json(&source.join("laws/L1/versions.json"), &history);
        write_json(
            &source.join("laws/L1/revisions/L1_20100401_A.json"),
            &json!({"revision_id":"L1_20100401_A","articles":[{"article_no":"第709条","paragraphs":[{"text":"候補の過去版の条文"}]}]}),
        );
        let shared =
            linking::resolve_hanrei_laws(&doc, &resolver, &titles, |_| Ok(Some(history.clone())))
                .unwrap();
        let b = bundle(&src, &doc, &resolver, &titles, "2026-10-24").unwrap();
        assert_eq!(
            b["laws"][0]["versions"],
            serde_json::to_value(&shared.laws[0].versions).unwrap()
        );
        assert_eq!(
            b["laws"][0]["evidence"],
            serde_json::to_value(&shared.laws[0].evidence).unwrap()
        );
        let excerpts = b["excerpts"].as_array().unwrap();
        assert!(excerpts.iter().any(|e| e["text"] == "候補の過去版の条文"));
        assert!(!excerpts
            .iter()
            .any(|e| e["url"].as_str().unwrap_or("").contains("20260401")));
        write_page(&wiki, &src, &b, "2026-10-24").unwrap();
        let page = Page::read(&wiki.join("hanrei/5.md")).unwrap();
        assert!(page.body.contains("[709](#/hanrei/5/laws/L1/709)"));
        assert!(page
            .body
            .contains("](#/hanrei/5/laws/L1/709?revision=L1_20100401_A)"));
        assert!(!page.body.contains("laws.e-gov.go.jp"));
        let mut missing = history.clone();
        missing["versions"][0]["body_available"] = json!(false);
        write_json(&source.join("laws/L1/versions.json"), &missing);
        let b = bundle(&src, &doc, &resolver, &titles, "2026-10-24").unwrap();
        assert!(!b["excerpts"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["ref"].as_str().unwrap_or("").contains("#law-")));
        // Reference candidates are navigation metadata, never applicable-law excerpts.
        let mut earlier = doc.clone();
        earlier["body_text"] = json!("平成１３年４月１日付けの処分が争われた。");
        write_json(&source.join("laws/L1/versions.json"), &history);
        let b = bundle(&src, &earlier, &resolver, &titles, "2026-10-24").unwrap();
        assert_eq!(b["laws"][0]["basis"], "unresolved");
        assert!(b["laws"][0]["versions"].as_array().unwrap().is_empty());
        assert_eq!(
            b["laws"][0]["reference_versions"][0]["revision_id"],
            "L1_20100401_A"
        );
        assert!(!b["excerpts"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["ref"].as_str().unwrap_or("").contains("#law-")));
        std::fs::remove_dir_all(source.parent().unwrap()).unwrap();
    }

    #[test]
    fn daily_plan_wires_new_cases_into_the_shared_llm_chunks() {
        let (source, wiki, work) = fixture();
        let args = plan::PlanArgs {
            base_url: source.display().to_string(),
            wiki: wiki.clone(),
            work: work.clone(),
            max_items: 0,
            max_probes: 10,
            lookback_days: 45,
            today: Some("2026-10-23".into()),
            max_bills: 0,
            max_pubcomments: 0,
            max_kanpo: 0,
            max_people: 0,
            max_roster: 0,
            reiki_base_url: String::new(),
            chunk_size: 4,
            max_enforcements: 0,
            max_hanrei: 3,
            max_hanrei_backfill: 0,
        };
        plan::run_plan(&args).unwrap();
        index(&source, &[100, 200, 5]);
        case(&source, 5);
        plan::run_plan(&args).unwrap();
        let planned = Plan::load(&work).unwrap().unwrap();
        assert_eq!(planned.tasks.len(), 1);
        assert_eq!(planned.tasks[0].kind, "hanrei");
        assert!(std::fs::read_to_string(work.join("chunks/001.md"))
            .unwrap()
            .contains("判例ページ"));
        std::fs::remove_dir_all(source.parent().unwrap()).unwrap();
    }

    fn write_json(path: &Path, value: &Value) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, serde_json::to_vec(value).unwrap()).unwrap();
    }

    #[test]
    fn long_sources_keep_separate_contiguous_fragments_within_budget() {
        let body = format!("冒頭{}中盤{}末尾", "甲".repeat(30_000), "乙".repeat(30_000));
        let bundle = json!({"case":{"body_text":body,"holding":"duplicate","summary":"duplicate"},"excerpts":[
            {"ref":"hanrei:5#holding","text":"判示事項".repeat(2000)},
            {"ref":"hanrei:5#summary","text":"裁判要旨".repeat(2500)},
            {"ref":"hanrei:5#body","text":body},
            {"ref":"hanrei:5#law-L1-709-v1","text":"条文".repeat(4000)},
            {"ref":"hanrei:5#law-L1-710-v1","text":"予算超過"}
        ]});
        let prompt = prompt_bundle(&bundle);
        assert_eq!(prompt["source_truncated"], true);
        assert_eq!(prompt["summary_input"]["included_chars"], 36_000);
        assert_eq!(
            prompt["summary_input"]["omitted_refs"],
            json!(["hanrei:5#law-L1-710-v1"])
        );
        for field in ["body_text", "holding", "summary"] {
            assert!(prompt["case"].get(field).is_none());
        }
        let excerpts = prompt["excerpts"].as_array().unwrap();
        assert_eq!(
            excerpts
                .iter()
                .map(|e| e["text"].as_str().unwrap().chars().count())
                .sum::<usize>(),
            36_000
        );
        let fragments: Vec<_> = excerpts
            .iter()
            .filter(|e| e["ref"] == "hanrei:5#body")
            .collect();
        assert_eq!(fragments.len(), 3);
        assert!(fragments[0]["text"].as_str().unwrap().starts_with("冒頭"));
        assert!(fragments[1]["text"].as_str().unwrap().contains("中盤"));
        assert!(fragments[2]["text"].as_str().unwrap().ends_with("末尾"));
        let chars: Vec<_> = body.chars().collect();
        let mut previous_end = 0;
        for e in fragments {
            let start = e["fragment"]["start_char"].as_u64().unwrap() as usize;
            let end = e["fragment"]["end_char"].as_u64().unwrap() as usize;
            assert!(start >= previous_end);
            assert_eq!(e["text"], chars[start..end].iter().collect::<String>());
            previous_end = end;
        }
        let short = prompt_bundle(
            &json!({"case":{},"excerpts":[{"ref":"hanrei:5#body","text":"短い判決本文"}]}),
        );
        assert_eq!(short["source_truncated"], false);
        assert_eq!(short["excerpts"][0]["text"], "短い判決本文");
        // 残り予算が少ない場合や上限の直上でも、抜粋範囲を重複させない。
        for len in [1, 2, 3, 22_001, 22_002] {
            let p = prompt_bundle(
                &json!({"case":{},"excerpts":[{"ref":"hanrei:5#body","text":"あ".repeat(len)}]}),
            );
            let ranges = p["excerpts"].as_array().unwrap();
            for pair in ranges.windows(2) {
                assert!(
                    pair[0]["fragment"]["end_char"].as_u64()
                        <= pair[1]["fragment"]["start_char"].as_u64()
                );
            }
        }
    }

    #[test]
    fn each_explanation_requires_case_evidence_and_official_text_is_preserved() {
        let (source, wiki, work) = fixture();
        let src = Source::new(&source.display().to_string()).unwrap();
        plan_tasks(&src, &wiki, &work, 3, 10, "2026-10-23").unwrap();
        index(&source, &[100, 200, 5]);
        case(&source, 5);
        let case_path = source.join("hanrei/5.json");
        let mut doc: Value = serde_json::from_slice(&std::fs::read(&case_path).unwrap()).unwrap();
        doc["summary"] = json!("公開要旨の原文。\n[リンクではない](https://example.com)\n<!-- lawpub:begin llm --> &lt;");
        write_json(&case_path, &doc);
        let tasks = plan_tasks(&src, &wiki, &work, 3, 10, "2026-10-24").unwrap();
        summarize(&wiki, &tasks[0]);
        let original = Page::read(&wiki.join(&tasks[0].page)).unwrap();
        assert!(summary_errors(&original).is_empty());
        assert!(source_matches(&wiki, &original).unwrap());
        assert!(original
            .body
            .contains("> 公開要旨の原文。\n> \\[リンクではない\\]"));
        assert!(original
            .body
            .contains("&lt;!-- lawpub:begin llm --&gt; &amp;lt;"));
        let mut changed = original.clone();
        changed.body = changed.body.replace("公開要旨の原文", "AIが書き換えた要旨");
        assert!(!source_matches(&wiki, &changed).unwrap());
        changed = original.clone();
        changed.body = changed.body.replace(
            "## 判断と理由\n損害賠償請求について判断した[^1]。",
            "## 判断と理由\n条文だけから結論を説明した[^law]。",
        );
        changed.body = changed.body.replace(LLM_END, &format!("[^law]: [hanrei:5#law-L1-709-v1](https://example.com) 「条文だけの引用である。」\n{LLM_END}"));
        assert!(summary_errors(&changed)
            .iter()
            .any(|e| e.contains("判断と理由")));
        changed = original.clone();
        changed.body = changed.body.replace(
            "## 要約\n民法709条による損害賠償請求について判断した[^1]。",
            "",
        );
        assert!(summary_errors(&changed).iter().any(|e| e.contains("要約")));
        // 既存の4節形式を再要約せず読み込める。
        changed
            .frontmatter
            .retain(|(k, _)| k != "summary_format_version");
        assert!(summary_errors(&changed).is_empty());
        std::fs::remove_dir_all(source.parent().unwrap()).unwrap();
    }

    #[test]
    fn outcome_and_limits_require_case_evidence_and_keep_older_formats_readable() {
        let mut page = Page::default();
        page.set("case_id", json!(42));
        page.set("summary_format_version", json!(3));
        let sections = [
            "要約",
            "事案",
            "争点",
            "結論",
            "判断と理由",
            "射程・留意点",
            "法令との関係",
        ];
        let prose = sections
            .iter()
            .map(|h| format!("## {h}\nこの事件の説明[^1]。\n\n"))
            .collect::<String>();
        page.body = format!("{LLM_BEGIN}\n{prose}[^1]: [hanrei:42#summary](https://example.com/case) 「この事件の原文」\n[^law]: [hanrei:42#law-L1-1-v1](https://example.com/law) 「条文の原文」\n{LLM_END}");
        assert!(summary_errors(&page).is_empty());
        for heading in ["結論", "射程・留意点"] {
            for text in ["", "根拠のない説明。", "条文だけの説明[^law]。"] {
                let mut changed = page.clone();
                changed.body = changed.body.replace(
                    &format!("## {heading}\nこの事件の説明[^1]。"),
                    &format!("## {heading}\n{text}"),
                );
                assert!(
                    summary_errors(&changed).iter().any(|e| e.contains(heading)),
                    "{heading}: {text}"
                );
            }
            let mut unknown = page.clone();
            unknown.body = unknown.body.replace(
                &format!("## {heading}\nこの事件の説明[^1]。"),
                &format!("## {heading}\n{UNKNOWN}"),
            );
            assert!(summary_errors(&unknown).is_empty());
            let mut missing = page.clone();
            missing.body = missing
                .body
                .replace(&format!("## {heading}\nこの事件の説明[^1]。\n\n"), "");
            assert!(summary_errors(&missing).iter().any(|e| e.contains(heading)));
        }
        let mut legacy = page.clone();
        for heading in ["結論", "射程・留意点"] {
            legacy.body = legacy
                .body
                .replace(&format!("## {heading}\nこの事件の説明[^1]。\n\n"), "");
        }
        assert!(summary_errors(&legacy).iter().any(|e| e.contains("結論")));
        legacy.set("summary_format_version", json!(2));
        assert!(summary_errors(&legacy).is_empty());
        legacy
            .frontmatter
            .retain(|(k, _)| k != "summary_format_version");
        legacy.body = legacy.body.replace("## 要約\nこの事件の説明[^1]。\n\n", "");
        assert!(summary_errors(&legacy).is_empty());
    }

    #[test]
    fn case_relations_require_the_same_law_and_article_and_remove_old_links() {
        let root = temp_dir("case-relations");
        let mut cases = Vec::new();
        for (id, law, articles) in [
            (1, "L1", vec!["709", "710"]),
            (2, "L1", vec!["709"]),
            (3, "L1", vec!["710"]),
            (4, "L2", vec!["709"]),
        ] {
            let mut page = Page::default();
            page.set("case_id", json!(id));
            page.set("title", json!(format!("判例{id}")));
            page.set("description", json!(format!("要約{id}")));
            page.set("judged_on", json!("令和7年1月15日"));
            page.set("court", json!("東京地方裁判所"));
            page.set("date", json!("2026-10-24"));
            page.set(
                "referenced_articles",
                json!([{"law_id":law,"title":law,"articles":articles}]),
            );
            page.body = format!("# 判例{id}\n");
            let rel = format!("hanrei/{id}.md");
            page.write(&root.join(&rel)).unwrap();
            cases.push((rel, page));
        }
        let relations = case_relations(&cases);
        let entries = &relations["hanrei/1.md"];
        assert_eq!(
            entries.iter().map(|c| c.case_id).collect::<Vec<_>>(),
            vec![2, 3]
        );
        assert_eq!(entries[0].path, "hanrei/2");
        assert_eq!(entries[0].judged_on, "令和7年1月15日");
        assert_eq!(entries[0].court, "東京地方裁判所");
        assert_eq!(entries[0].description, "要約2");
        assert_eq!(entries[0].common_articles[0].law_id, "L1");
        assert_eq!(entries[0].common_articles[0].article, "709");
        write_backlinks(&root, &cases).unwrap();
        let one = Page::read(&root.join("hanrei/1.md")).unwrap();
        assert!(one.body.contains("[判例2](../hanrei/2.md)"));
        assert!(one.body.contains("[判例3](../hanrei/3.md)"));
        assert!(!one.body.contains("[判例1]"));
        assert!(!one.body.contains("[判例4]"));
        assert!(one.body.contains("#/hanrei/1/laws/L1/709"));
        assert!(one.body.contains("先例関係・同じ争点や結論"));
        assert!(!Page::read(&root.join("hanrei/2.md"))
            .unwrap()
            .body
            .contains("[判例3]"));
        assert!(!Page::read(&root.join("hanrei/4.md"))
            .unwrap()
            .body
            .contains("related-cases"));
        let updated: Vec<_> = cases
            .iter()
            .map(|(rel, _)| (rel.clone(), Page::read(&root.join(rel)).unwrap()))
            .collect();
        write_backlinks(&root, &updated).unwrap();
        assert_eq!(
            one.body,
            Page::read(&root.join("hanrei/1.md")).unwrap().body
        );
        let remaining: Vec<_> = updated
            .into_iter()
            .filter(|(rel, _)| rel == "hanrei/1.md" || rel == "hanrei/4.md")
            .collect();
        write_backlinks(&root, &remaining).unwrap();
        let one = Page::read(&root.join("hanrei/1.md")).unwrap();
        assert!(!one.body.contains("[判例2]"));
        assert!(!one.body.contains("共通する参照条文"));
        // 多い場合は10件まで。入力順によらず同じ順で出力する。
        let mut many = vec![cases[0].clone()];
        for id in 10..24 {
            let mut page = cases[1].1.clone();
            page.set("case_id", json!(id));
            page.set("title", json!(format!("判例{id}")));
            many.push((format!("hanrei/{id}.md"), page));
        }
        // 複数条文が共通する判例を、パス順より優先する。
        many[14].1.set(
            "referenced_articles",
            cases[0].1.get("referenced_articles").unwrap().clone(),
        );
        assert_eq!(case_relations(&many)["hanrei/1.md"][0].case_id, 23);
        write_backlinks(&root, &many).unwrap();
        let one = Page::read(&root.join("hanrei/1.md")).unwrap();
        assert_eq!(one.body.matches("| [判例").count(), 10);
        many.reverse();
        write_backlinks(&root, &many).unwrap();
        assert_eq!(
            one.body,
            Page::read(&root.join("hanrei/1.md")).unwrap().body
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    fn index(source: &Path, ids: &[u64]) {
        write_json(
            &source.join("hanrei/index.json"),
            &json!({"count":ids.len(),"cases":ids.iter().map(|id| json!({"case_id":id})).collect::<Vec<_>>()}),
        );
    }

    fn case(source: &Path, id: u64) {
        write_json(
            &source.join(format!("hanrei/{id}.json")),
            &json!({
                "case_id":id,"case_name":"損害賠償請求事件","case_number":"令和8(受)1",
                "judged_on":"令和8年10月1日","court":"最高裁判所第一小法廷","result":"棄却",
                "holding":"民法第709条による損害賠償請求について判断した。","summary":"",
                "referenced_laws":"民法第709条","detail_url":format!("https://www.courts.go.jp/hanrei/{id}/detail2/index.html"),
                "pdf_url":format!("https://www.courts.go.jp/assets/hanrei/{id}.pdf")
            }),
        );
    }

    fn save_plan(wiki: &Path, work: &Path, source: &Path, tasks: Vec<Task>) {
        let before = tasks
            .iter()
            .map(|t| {
                (
                    t.page.clone(),
                    llm_hash(&Page::read(&wiki.join(&t.page)).unwrap().body),
                )
            })
            .collect();
        write_json(
            &work.join("plan.json"),
            &serde_json::to_value(Plan {
                schema_version: 2,
                date: "2026-10-24".into(),
                base_url: source.display().to_string(),
                tasks,
                before,
            })
            .unwrap(),
        );
    }

    fn summarize(wiki: &Path, task: &Task) {
        let path = wiki.join(&task.page);
        let mut page = Page::read(&path).unwrap();
        page.set("description", json!("民法709条に関する損害賠償請求の判例"));
        let narrative = format!("## 要約\n民法709条による損害賠償請求について判断した[^1]。\n\n## 事案\n{UNKNOWN}\n\n## 争点\n{UNKNOWN}\n\n## 結論\n{UNKNOWN}\n\n## 判断と理由\n損害賠償請求について判断した[^1]。\n\n## 射程・留意点\n{UNKNOWN}\n\n## 法令との関係\n民法709条に言及した[^1]。\n\n[^1]: [hanrei:{}#holding](https://www.courts.go.jp/hanrei/{}/detail2/index.html) 「民法第709条による損害賠償請求について判断した。」",task.id,task.id);
        page.body = page.body.replace(
            &format!("{LLM_BEGIN}\n{LLM_END}"),
            &format!("{LLM_BEGIN}\n{narrative}\n{LLM_END}"),
        );
        page.write(&path).unwrap();
    }

    #[test]
    fn plans_only_new_cases_keeps_budget_overflow_and_rolls_back_unwritten_tasks() {
        let (source, wiki, work) = fixture();
        let src = Source::new(&source.display().to_string()).unwrap();
        assert!(plan_tasks(&src, &wiki, &work, 3, 10, "2026-10-23")
            .unwrap()
            .is_empty());
        assert!(!wiki.join("hanrei").exists(), "過去分の雛形も作らない");
        index(&source, &[100, 200, 5, 6]);
        case(&source, 5);
        case(&source, 6);
        let tasks = plan_tasks(&src, &wiki, &work, 1, 10, "2026-10-24").unwrap();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].laws[0].law_id, "L1");
        let page = Page::read(&wiki.join(&tasks[0].page)).unwrap();
        assert_eq!(
            page.get_str("source_scope"),
            "判示事項・裁判要旨（判決全文未収録）"
        );
        assert!(page.body.contains("../laws/L1.md"));
        let bundle: Value =
            serde_json::from_slice(&std::fs::read(work.join(&tasks[0].source)).unwrap()).unwrap();
        assert_eq!(bundle["summary_format_version"], 4);
        assert_eq!(page.get("summary_format_version"), Some(&json!(4)));
        assert_eq!(bundle["laws"][0]["articles"], json!(["709"]));
        assert!(bundle["excerpts"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["scope"]
                .as_str()
                .unwrap()
                .contains("適用法条・経過措置は未確認")));
        save_plan(&wiki, &work, &source, tasks.clone());
        finalize::run_finalize(&finalize::FinalizeArgs {
            wiki: wiki.clone(),
            work: work.clone(),
        })
        .unwrap();
        assert!(!wiki.join(&tasks[0].page).exists());
        let retried = plan_tasks(&src, &wiki, &work, 3, 10, "2026-11-30").unwrap();
        assert_eq!(retried.len(), 2, "日付窓外でも未完了と上限超過を再投入する");
        std::fs::remove_dir_all(source.parent().unwrap()).unwrap();
    }

    #[test]
    fn completed_case_has_verified_citations_law_backlinks_and_exports_without_reprocessing() {
        let (source, wiki, work) = fixture();
        let src = Source::new(&source.display().to_string()).unwrap();
        plan_tasks(&src, &wiki, &work, 3, 10, "2026-10-23").unwrap();
        index(&source, &[100, 200, 5, 6]);
        case(&source, 5);
        case(&source, 6);
        let tasks = plan_tasks(&src, &wiki, &work, 3, 10, "2026-10-24").unwrap();
        save_plan(&wiki, &work, &source, tasks.clone());
        for task in &tasks {
            summarize(&wiki, task);
        }
        finalize::run_finalize(&finalize::FinalizeArgs {
            wiki: wiki.clone(),
            work: work.clone(),
        })
        .unwrap();
        assert!(
            !Intake::load(&wiki).unwrap().pending.contains_key(&5),
            "完了した判例を未完了一覧に残さない"
        );
        let check_args = check::CheckArgs {
            wiki: wiki.clone(),
            work: work.clone(),
            base_url: source.display().to_string(),
            changed_only: false,
        };
        check::run_check(&check_args).unwrap();
        let law = Page::read(&wiki.join("laws/L1.md")).unwrap();
        assert!(law.body.contains("../hanrei/5.md"), "法令から判例に戻れる");
        assert!(Page::read(&wiki.join("index.md"))
            .unwrap()
            .body
            .contains("hanrei/5.md"));
        let out = source.parent().unwrap().join("export");
        export::run_export(&export::ExportArgs {
            wiki: wiki.clone(),
            out: out.clone(),
        })
        .unwrap();
        let exported: Value =
            serde_json::from_slice(&std::fs::read(out.join("index.json")).unwrap()).unwrap();
        assert!(exported["pages"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["type"] == "hanrei"));
        assert_eq!(exported["hanrei_progress"]["completed"], 2);
        let previewed = preview(&src, &wiki, 20, "2026-10-25").unwrap();
        assert!(previewed["candidates"]
            .as_array()
            .unwrap()
            .iter()
            .all(|c| c["case_id"] != 5 && c["case_id"] != 6));
        assert!(out.join("hanrei-progress.json").exists());
        let graph: Value =
            serde_json::from_slice(&std::fs::read(out.join("graph.json")).unwrap()).unwrap();
        assert!(graph["links"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["source"] == "hanrei/5" && e["target"] == "hanrei/6"));
        let exported_case: Value =
            serde_json::from_slice(&std::fs::read(out.join("page/hanrei/5.json")).unwrap())
                .unwrap();
        assert_eq!(
            exported_case["hanrei_related_cases"],
            json!([{
                "path":"hanrei/6", "case_id":6, "title":"損害賠償請求事件 — 最高裁判所第一小法廷 令和8(受)1",
                "judged_on":"令和8年10月1日", "court":"最高裁判所第一小法廷",
                "description":Page::read(&wiki.join("hanrei/6.md")).unwrap().get_str("description"),
                "common_articles":[{"law_id":"L1","title":"民法","article":"709"}]
            }])
        );
        let explanation = exported_case["hanrei_explanation"].as_str().unwrap();
        assert_eq!(
            exported_case["hanrei_citations"],
            json!([{"label": exported_case["frontmatter"]["sources"][0]["id"],"reference":"hanrei:5#holding","field":"holding","quote":"民法第709条による損害賠償請求について判断した。"}])
        );
        let mut citations_page = Page::read(&wiki.join("hanrei/5.md")).unwrap();
        citations_page.body = format!("{LLM_BEGIN}\n[^1]: [hanrei:5#summary](https://example.com) 「裁判要旨の引用」\n[^2]: [hanrei:5#body](https://example.com) 「判決本文の引用」\n[^3]: [hanrei:6#holding](https://example.com) 「別事件の引用」\n[^4]: [hanrei:5#law-L1-709](https://example.com) 「条文の引用」\n{LLM_END}");
        assert_eq!(
            explanation_citations(&citations_page),
            vec![
                json!({"label":"1","reference":"hanrei:5#summary","field":"summary","quote":"裁判要旨の引用"}),
                json!({"label":"2","reference":"hanrei:5#body","field":"body_text","quote":"判決本文の引用"}),
            ]
        );
        assert!(explanation.contains("## 要約\n"));
        assert!(explanation.contains(&format!("[^{}]: [hanrei:5#holding]", exported_case["frontmatter"]["sources"][0]["id"].as_str().unwrap())));
        assert!(!explanation.contains("裁判所の公開情報（原文）"));
        assert!(!explanation.contains("共通する参照条文"));
        // ready なページでも固定資料との対応が壊れたら、一覧・本文・グラフから外す。
        let path = wiki.join("hanrei/5.md");
        let original = Page::read(&path).unwrap();
        assert!(source_matches(&wiki, &original).unwrap());
        for field in ["court", "judged_on"] {
            let mut changed = original.clone();
            changed.set(field, json!("別のメタデータ"));
            assert!(!source_matches(&wiki, &changed).unwrap());
        }
        let mut legacy = original.clone();
        legacy.frontmatter.retain(|(key, _)| key != "court");
        assert!(
            source_matches(&wiki, &legacy).unwrap(),
            "旧ページの裁判所欄の欠落は許容"
        );
        let mut wrong = original.clone();
        wrong.set("source_scope", json!("対応しない収録範囲"));
        assert!(ready(&wrong));
        wrong.write(&path).unwrap();
        export::run_export(&export::ExportArgs {
            wiki: wiki.clone(),
            out: out.clone(),
        })
        .unwrap();
        let exported: Value =
            serde_json::from_slice(&std::fs::read(out.join("index.json")).unwrap()).unwrap();
        assert!(!exported["pages"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["path"] == "hanrei/5"));
        assert!(!out.join("page/hanrei/5.json").exists());
        let graph: Value =
            serde_json::from_slice(&std::fs::read(out.join("graph.json")).unwrap()).unwrap();
        assert!(!graph["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|n| n["id"] == "hanrei/5"));
        assert!(!graph["links"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["source"] == "hanrei/5" || e["target"] == "hanrei/5"));
        let remaining: Value =
            serde_json::from_slice(&std::fs::read(out.join("page/hanrei/6.json")).unwrap())
                .unwrap();
        assert_eq!(remaining["hanrei_related_cases"], json!([]));
        assert!(!remaining["body"]
            .as_str()
            .unwrap()
            .contains("../hanrei/5.md"));
        assert!(!remaining["body"]
            .as_str()
            .unwrap()
            .contains("共通する参照条文"));
        assert!(
            Page::read(&wiki.join("hanrei/6.md"))
                .unwrap()
                .body
                .contains("../hanrei/5.md"),
            "export の関連区間再計算は元の Wiki ファイルを書き換えない"
        );
        original.write(&path).unwrap();
        assert!(plan_tasks(&src, &wiki, &work, 3, 10, "2026-10-25")
            .unwrap()
            .is_empty());
        // 作業バンドルや配信元がなくても、固定した原文で古い引用を検証できる。
        std::fs::remove_dir_all(work.join("docs")).unwrap();
        std::fs::remove_dir_all(source.join("hanrei")).unwrap();
        check::run_check(&check_args).unwrap();
        let path = wiki.join(&tasks[0].page);
        let original = std::fs::read_to_string(&path).unwrap();
        for wrong in [
            original.replace(
                "「民法第709条による損害賠償請求について判断した。」",
                "「原文に存在しない引用文字列である。」",
            ),
            original.replace(
                "#holding](https://www.courts.go.jp/hanrei/5/",
                "#holding](https://www.courts.go.jp/hanrei/6/",
            ),
        ] {
            std::fs::write(&path, wrong).unwrap();
            assert!(check::run_check(&check_args).is_err());
        }
        std::fs::remove_dir_all(source.parent().unwrap()).unwrap();
    }

    #[test]
    fn explicit_refresh_restores_old_page_and_snapshot_until_regeneration_succeeds() {
        let (source, wiki, work) = fixture();
        let src = Source::new(&source.display().to_string()).unwrap();
        case(&source, 200);
        queue_cases(&src, &wiki, &[200], "2026-10-24").unwrap();
        let tasks = plan_tasks(&src, &wiki, &work, 1, 10, "2026-10-24").unwrap();
        save_plan(&wiki, &work, &source, tasks.clone());
        summarize(&wiki, &tasks[0]);
        finalize::run_finalize(&finalize::FinalizeArgs {
            wiki: wiki.clone(),
            work: work.clone(),
        })
        .unwrap();
        let path = wiki.join("hanrei/200.md");
        let original = Page::read(&path).unwrap();
        let snapshot = std::fs::read(snapshot_path(&wiki, 200)).unwrap();
        assert!(ready(&original));
        assert!(original
            .get("sources")
            .unwrap()
            .as_array()
            .unwrap()
            .iter()
            .all(|s| s["id"].as_str().unwrap().starts_with("src-")));
        assert!(original.get("verified").is_none());
        queue_cases(&src, &wiki, &[200], "2026-10-25").unwrap();
        assert!(Intake::load(&wiki).unwrap().pending.is_empty());
        queue_cases_with_refresh(&src, &wiki, &[200], "2026-10-25", true).unwrap();
        let refresh_work = work.join("refresh");
        let tasks = plan_tasks(&src, &wiki, &refresh_work, 1, 10, "2026-10-25").unwrap();
        assert_eq!(tasks.len(), 1);
        save_plan(&wiki, &refresh_work, &source, tasks);
        let args = finalize::FinalizeArgs {
            wiki: wiki.clone(),
            work: refresh_work,
        };
        for _ in 0..2 {
            finalize::run_finalize(&args).unwrap();
            let restored = Page::read(&path).unwrap();
            assert!(ready(&restored));
            assert_eq!(
                first_llm_block(&original.body),
                first_llm_block(&restored.body)
            );
            assert_eq!(snapshot, std::fs::read(snapshot_path(&wiki, 200)).unwrap());
            assert!(Intake::load(&wiki).unwrap().pending[&200].refresh);
        }
        let retry_work = work.join("retry");
        let tasks = plan_tasks(&src, &wiki, &retry_work, 1, 10, "2026-10-26").unwrap();
        save_plan(&wiki, &retry_work, &source, tasks.clone());
        summarize(&wiki, &tasks[0]);
        finalize::run_finalize(&finalize::FinalizeArgs {
            wiki: wiki.clone(),
            work: retry_work,
        })
        .unwrap();
        let updated = Page::read(&path).unwrap();
        assert!(ready(&updated));
        assert!(Intake::load(&wiki).unwrap().pending.is_empty());
        assert_eq!(updated.get_str("date"), original.get_str("date"));
        assert_ne!(snapshot, std::fs::read(snapshot_path(&wiki, 200)).unwrap());
        std::fs::remove_dir_all(source.parent().unwrap()).unwrap();
    }

    #[test]
    fn unavailable_sources_stay_pending_and_do_not_starve_other_cases() {
        let (source, wiki, work) = fixture();
        let src = Source::new(&source.display().to_string()).unwrap();
        plan_tasks(&src, &wiki, &work, 0, 1, "2026-10-23").unwrap();
        index(&source, &[100, 200, 5, 6]);
        assert!(plan_tasks(&src, &wiki, &work, 3, 1, "2026-10-24")
            .unwrap()
            .is_empty());
        case(&source, 6);
        assert_eq!(
            plan_tasks(&src, &wiki, &work, 3, 1, "2026-10-25").unwrap()[0].id,
            "6"
        );
        case(&source, 5);
        let path = source.join("hanrei/5.json");
        let mut doc: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        doc["holding"] = json!("");
        doc["referenced_laws"] = json!("");
        write_json(&path, &doc);
        let tasks = plan_tasks(&src, &wiki, &work, 3, 10, "2026-10-26").unwrap();
        assert!(
            !tasks.iter().any(|t| t.id == "5"),
            "参照法条だけ・根拠本文なしでは要約しない"
        );
        doc["body_text"] = json!("法令リンクのない判決本文も引用して要約することができる。");
        write_json(&path, &doc);
        let tasks = plan_tasks(&src, &wiki, &work, 3, 10, "2026-10-27").unwrap();
        let task = tasks.iter().find(|t| t.id == "5").unwrap();
        assert!(task.laws.is_empty());
        assert_eq!(
            Page::read(&wiki.join(&task.page))
                .unwrap()
                .get_str("source_scope"),
            "判決PDFの機械抽出本文・判示事項・裁判要旨"
        );
        std::fs::remove_dir_all(source.parent().unwrap()).unwrap();
    }

    #[test]
    fn explicit_history_queue_is_bounded_idempotent_and_preserves_new_case_intake() {
        let (source, wiki, work) = fixture();
        let src = Source::new(&source.display().to_string()).unwrap();
        index(&source, &[100, 200]);
        case(&source, 100);
        case(&source, 200);
        queue_cases(&src, &wiki, &[200], "2026-10-25").unwrap();
        let state = Intake::load(&wiki).unwrap();
        assert_eq!(state.seen, BTreeSet::from([100, 200]));
        assert_eq!(state.pending.keys().copied().collect::<Vec<_>>(), vec![200]);
        queue_cases(&src, &wiki, &[200], "2026-10-26").unwrap();
        assert_eq!(
            Intake::load(&wiki).unwrap().pending[&200].first_seen,
            "2026-10-25"
        );
        let before = std::fs::read(wiki.join(INTAKE)).unwrap();
        for ids in [
            vec![],
            vec![0],
            vec![200, 200],
            vec![100, 999],
            (1..=11).collect(),
        ] {
            assert!(queue_cases(&src, &wiki, &ids, "2026-10-26").is_err());
            assert_eq!(std::fs::read(wiki.join(INTAKE)).unwrap(), before);
        }
        index(&source, &[100, 200, 5]);
        case(&source, 5);
        let tasks = plan_tasks(&src, &wiki, &work, 10, 10, "2026-10-26").unwrap();
        assert_eq!(tasks.len(), 2, "選んだ過去判例と新規判例だけがタスクになる");
        assert!(!wiki.join("hanrei/100.md").exists());
        assert!(wiki.join("hanrei/200.md").exists());
        assert!(wiki.join("hanrei/5.md").exists());
        std::fs::remove_dir_all(source.parent().unwrap()).unwrap();
    }

    #[test]
    fn baseline_excludes_history_but_accepts_new_ids_regardless_of_size_or_judgment_date() {
        let mut intake = Intake::default();
        intake
            .observe(
                &json!({"count":2,"cases":[{"case_id":100},{"case_id":200}]}),
                "2026-10-23",
            )
            .unwrap();
        assert!(intake.pending.is_empty(), "初回の既存判例は要約しない");
        intake.observe(&json!({"count":3,"cases":[{"case_id":100},{"case_id":200},{"case_id":5,"judged_on":"昭和30年1月1日"}]}), "2026-10-24").unwrap();
        assert_eq!(intake.pending.keys().copied().collect::<Vec<_>>(), vec![5]);
        intake
            .observe(&json!({"count":1,"cases":[{"case_id":200}]}), "2026-10-25")
            .unwrap();
        assert!(
            intake.pending.contains_key(&5),
            "一時的に索引から消えても再試行する"
        );
        assert_eq!(intake.pending[&5].first_seen, "2026-10-24");
    }

    #[test]
    fn malformed_index_cannot_initialize_or_advance_baseline() {
        for index in [
            json!({}),
            json!({"count":2,"cases":[]}),
            json!({"count":1,"cases":[{}]}),
            json!({"count":2,"cases":[{"case_id":1},{"case_id":1}]}),
        ] {
            let mut intake = Intake::default();
            assert!(intake.observe(&index, "2026-10-23").is_err());
            assert!(intake.initialized_at.is_none());
        }
    }

    #[test]
    fn missing_index_does_not_seed_history_and_zero_budget_preserves_new_cases() {
        let (source, wiki, work) = fixture();
        let src = Source::new(&source.display().to_string()).unwrap();
        std::fs::remove_file(source.join("hanrei/index.json")).unwrap();
        assert!(plan_tasks(&src, &wiki, &work, 3, 10, "2026-10-23")
            .unwrap()
            .is_empty());
        assert!(!wiki.join(INTAKE).exists());
        index(&source, &[100, 200]);
        assert!(plan_tasks(&src, &wiki, &work, 3, 10, "2026-10-24")
            .unwrap()
            .is_empty());
        index(&source, &[100, 200, 5]);
        case(&source, 5);
        assert!(plan_tasks(&src, &wiki, &work, 0, 10, "2026-10-25")
            .unwrap()
            .is_empty());
        assert_eq!(Intake::load(&wiki).unwrap().pending[&5].status, "pending");
        assert_eq!(
            plan_tasks(&src, &wiki, &work, 3, 10, "2027-01-01")
                .unwrap()
                .len(),
            1
        );
        std::fs::remove_dir_all(source.parent().unwrap()).unwrap();
    }
}
