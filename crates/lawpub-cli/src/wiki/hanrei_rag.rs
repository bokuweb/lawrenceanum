//! OKF v0.2 provenance and deterministic RAG records derived from cited case prose.
//! Quote matching confirms text location only, never the legal meaning of a claim.
use super::*;
use regex::Regex;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

const SECTIONS: [(&str, &str); 7] = [
    ("summary", "要約"),
    ("facts", "事案"),
    ("issues", "争点"),
    ("disposition", "結論"),
    ("reasoning", "判断と理由"),
    ("limitations", "射程・留意点"),
    ("law_relationship", "法令との関係"),
];

fn hash(value: &Value) -> String {
    format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(value).expect("JSON value"))
    )
}

fn labels(text: &str) -> Vec<String> {
    Regex::new(r"\[\^([^\]\n]+)\]")
        .unwrap()
        .captures_iter(text)
        .map(|c| c[1].to_string())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn section(body: &str, heading: &str) -> String {
    body.split_once(&format!("## {heading}\n"))
        .map(|(_, s)| s.split("\n## ").next().unwrap_or(s))
        .unwrap_or("")
        .lines()
        .filter(|l| !l.trim_start().starts_with("[^"))
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

fn plain(text: &str) -> String {
    let s = Regex::new(r"\[\^[^\]\n]+\]").unwrap().replace_all(text, "");
    let s = Regex::new(r"\[([^\]]+)\]\([^\)]+\)")
        .unwrap()
        .replace_all(&s, "$1")
        .into_owned();
    s.replace("**", "").replace('`', "").trim().to_string()
}

pub fn paragraph_errors(page: &Page) -> Vec<String> {
    let body = first_llm_block(&page.body);
    let citations = check::citations_in(&body);
    let prefix = format!(
        "hanrei:{}#",
        page.get("case_id").and_then(Value::as_u64).unwrap_or(0)
    );
    let own = |r: &str| {
        ["holding", "summary", "body"]
            .iter()
            .any(|k| r == format!("{prefix}{k}"))
    };
    let mut errors = Vec::new();
    let mut seen = BTreeSet::new();
    for c in &citations {
        if !seen.insert(&c.label) {
            errors.push(format!("引用ラベル {} が重複しています", c.label));
        }
    }
    let mut last = 0;
    for (_, heading) in SECTIONS {
        let marker = format!("## {heading}\n");
        match body.find(&marker) {
            Some(pos) if pos >= last => last = pos + marker.len(),
            _ => errors.push(format!("「{heading}」の見出しまたは順序が不正です")),
        }
        for paragraph in section(&body, heading)
            .split("\n\n")
            .filter(|s| !s.trim().is_empty() && s.trim() != hanrei::UNKNOWN)
        {
            let refs = labels(paragraph);
            if !citations
                .iter()
                .any(|c| own(&c.reference) && refs.contains(&c.label))
            {
                errors.push(format!(
                    "「{heading}」の各説明段落に、この判例の原文引用が必要です"
                ));
            }
            for label in refs {
                if !citations.iter().any(|c| c.label == label) {
                    errors.push(format!("引用 {label} の定義がありません"));
                }
            }
        }
    }
    errors
}

/// Character offsets refer to the full, unnormalized snapshot: zero-based, end-exclusive.
fn ranges(text: &str, quote: &str) -> (usize, Vec<Value>) {
    let chars: Vec<_> = text
        .chars()
        .enumerate()
        .filter(|(_, c)| !c.is_whitespace())
        .collect();
    let needle: Vec<_> = quote.chars().filter(|c| !c.is_whitespace()).collect();
    if needle.is_empty() || needle.len() > chars.len() {
        return (0, vec![]);
    }
    let mut count = 0;
    let mut ranges = Vec::new();
    for (start, window) in chars.windows(needle.len()).enumerate() {
        if window.iter().map(|(_, c)| c).eq(needle.iter()) {
            count += 1;
            if ranges.len() < 16 {
                ranges.push(
                    json!({"start_char":chars[start].0,"end_char":window.last().unwrap().0+1}),
                );
            }
        }
    }
    (count, ranges)
}

fn evidence(page: &Page, bundle: &Value) -> Result<Vec<Value>> {
    let mut result = Vec::new();
    let mut seen = BTreeSet::new();
    for citation in check::citations_in(&first_llm_block(&page.body)) {
        anyhow::ensure!(
            seen.insert(citation.label.clone()),
            "duplicate citation label: {}",
            citation.label
        );
        let unit = bundle["excerpts"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|e| e["ref"] == citation.reference)
            .context("citation source missing from snapshot")?;
        anyhow::ensure!(
            unit["url"] == citation.url,
            "citation URL differs from snapshot"
        );
        let quote_len = normalize_for_quote(&citation.quote).chars().count();
        anyhow::ensure!(
            (8..=200).contains(&quote_len),
            "citation length outside 8..200 characters"
        );
        let text = unit["text"].as_str().context("missing source text")?;
        let (match_count, offsets) = ranges(text, &citation.quote);
        anyhow::ensure!(
            match_count > 0,
            "citation does not match source: {}",
            citation.reference
        );
        result.push(json!({
            "id":citation.label,"reference":citation.reference,"resource":citation.url,
            "quote":citation.quote,"scope":unit["scope"],"source_text_hash":hash(&json!(text)),
            "match_count":match_count,"ranges":offsets,"ranges_truncated":match_count > 16,
            "check":"verbatim_ignoring_whitespace","semantic_support":"unverified"
        }));
    }
    result.sort_by(|a, b| a["id"].as_str().cmp(&b["id"].as_str()));
    Ok(result)
}

/// A single OKF page is the source of truth; JSON/SQLite consumers use the same records.
pub fn record(page: &Page, bundle: &Value) -> Result<Value> {
    let id = page
        .get("case_id")
        .and_then(Value::as_u64)
        .context("missing case ID")?;
    let document_id = format!("lawrenceanum:hanrei:{id}");
    let body = first_llm_block(&page.body);
    let evidence = evidence(page, bundle)?;
    let context = json!({"title":page.get_str("title"),"court":page.get_str("court"),
        "case_number":bundle["case"]["case_number"],"judged_on":page.get("judged_on"),
        "judged_on_iso":hanrei::case_date(&bundle["case"])});
    let prefix = format!("hanrei:{id}#");
    let mut chunks = Vec::new();
    for (key, heading) in SECTIONS {
        let prose = section(&body, heading);
        let mut claims = Vec::new();
        for paragraph in prose
            .split("\n\n")
            .map(str::trim)
            .filter(|p| !p.is_empty() && *p != hanrei::UNKNOWN)
        {
            let refs = labels(paragraph);
            let cited: Vec<_> = evidence
                .iter()
                .filter(|e| refs.iter().any(|r| e["id"] == *r))
                .cloned()
                .collect();
            if !cited.iter().any(|e| {
                ["holding", "summary", "body"]
                    .iter()
                    .any(|k| e["reference"] == format!("{prefix}{k}"))
            }) {
                continue;
            }
            let text = plain(paragraph);
            let content_hash = hash(&json!({"context":context,"section":key,"text":text}));
            claims.push(
                json!({"id":format!("{document_id}#{key}/{}", &content_hash[..16]),
                "text":text,"content_hash":content_hash,"evidence_ids":refs}),
            );
        }
        if claims.is_empty() {
            continue;
        }
        let text = claims
            .iter()
            .filter_map(|c| c["text"].as_str())
            .collect::<Vec<_>>()
            .join("\n\n");
        let used: BTreeSet<_> = claims
            .iter()
            .flat_map(|c| c["evidence_ids"].as_array().unwrap())
            .filter_map(Value::as_str)
            .collect();
        let cited: Vec<_> = evidence
            .iter()
            .filter(|e| used.contains(e["id"].as_str().unwrap_or("")))
            .collect();
        let retrieval_text = format!(
            "{}\n{} {}\n{}\n{}",
            page.get_str("title"),
            page.get_str("court"),
            page.get_str("judged_on"),
            heading,
            text
        );
        chunks.push(json!({"id":format!("{document_id}#{key}"),"section":key,"heading":heading,
            "text":text,"retrieval_text":retrieval_text,"claims":claims,
            "content_hash":hash(&json!({"context":context,"text":text,"section":key})),"evidence_hash":hash(&json!(cited))}));
    }
    let mut relations = Vec::new();
    for law in bundle["laws"].as_array().into_iter().flatten() {
        for article in law["articles"].as_array().into_iter().flatten() {
            relations.push(json!({"type":"mentions_article","from":document_id,
                "to":format!("lawrenceanum:law:{}#{}",law["law_id"].as_str().unwrap_or(""),article.as_str().unwrap_or("")),
                "law_id":law["law_id"],"article":article,"application_status":"unconfirmed",
                "version_candidates":law["versions"],"version_basis":law["basis"],"version_evidence":law["evidence"],"incomplete":law["incomplete"]}));
        }
    }
    let mut record = json!({"schema_version":1,"document_id":document_id,"concept_id":format!("hanrei/{id}"),
        "case_id":id,"context":context,"chunks":chunks,"evidence":evidence,"relations":relations,
        "source_scope":bundle["scope"],"summary_input":bundle["summary_input"],
        "provenance":{"prompt_version":bundle["prompt_version"],"model":bundle["generation_model"],
            "summary_format_version":bundle["summary_format_version"],"llm_generated":true,"semantic_support":"unverified"}});
    record["revision_hash"] = json!(hash(&record));
    Ok(record)
}

/// Stable, source-keyed footnote labels survive LLM footnote reordering.
fn normalize_labels(page: &mut Page) {
    let body = first_llm_block(&page.body);
    let mapping: BTreeMap<_, _> = check::citations_in(&body)
        .iter()
        .map(|c| {
            (
                c.label.clone(),
                format!("src-{}", &hash(&json!([c.reference, c.url, c.quote]))[..20]),
            )
        })
        .collect();
    let normalized =
        Regex::new(r"\[\^([^\]\n]+)\]")
            .unwrap()
            .replace_all(&body, |c: &regex::Captures<'_>| {
                format!(
                    "[^{}]",
                    mapping.get(&c[1]).map(String::as_str).unwrap_or(&c[1])
                )
            });
    let mut definitions = BTreeSet::new();
    let normalized = normalized
        .lines()
        .filter(|line| {
            if let Some(label) = line
                .trim_start()
                .strip_prefix("[^")
                .and_then(|s| s.split_once("]: ").map(|(id, _)| id))
            {
                definitions.insert(label.to_string())
            } else {
                true
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    page.body = page.body.replacen(&body, &normalized, 1);
}

pub fn page_record(wiki: &Path, page: &Page) -> Result<Value> {
    let id = page
        .get("case_id")
        .and_then(Value::as_u64)
        .context("missing case ID")?;
    let bundle: Value = serde_json::from_slice(&std::fs::read(hanrei::snapshot_path(wiki, id))?)?;
    record(page, &bundle)
}

pub fn planned_source_matches(work: &Path, task: &Task, page: &Page) -> bool {
    std::fs::read(work.join(&task.source))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        .is_some_and(|bundle| page.get("source_fingerprint") == bundle.get("source_fingerprint"))
}

fn okf_sources(record: &Value) -> Value {
    json!(record["evidence"].as_array().unwrap().iter().map(|e| json!({
        "id":e["id"],"resource":e["resource"],"title":e["reference"],
        "reference":e["reference"],"quote":e["quote"],"source_text_hash":e["source_text_hash"],"scope":e["scope"]})).collect::<Vec<_>>())
}

pub fn check_metadata(wiki: &Path, page: &Page) -> Result<()> {
    let record = page_record(wiki, page)?;
    anyhow::ensure!(
        page.get("rag") == Some(&record),
        "RAG data differs from cited prose or snapshot"
    );
    anyhow::ensure!(
        page.get("sources") == Some(&okf_sources(&record)),
        "OKF sources differ from cited prose"
    );
    anyhow::ensure!(
        page.get_str("status") == "draft" && page.get("verified").is_none(),
        "Generated case explanation must remain unverified"
    );
    let generated = page
        .get("generated")
        .context("missing OKF generated provenance")?;
    anyhow::ensure!(
        generated["by"]
            .as_str()
            .is_some_and(|v| v.starts_with("lawrenceanum/"))
            && generated["at"]
                .as_str()
                .is_some_and(|v| chrono::DateTime::parse_from_rfc3339(v).is_ok()),
        "Invalid OKF generated provenance"
    );
    Ok(())
}

pub fn annotate(wiki: &Path, page: &mut Page) -> Result<()> {
    let id = page
        .get("case_id")
        .and_then(Value::as_u64)
        .context("missing case ID")?;
    let bundle: Value = serde_json::from_slice(&std::fs::read(hanrei::snapshot_path(wiki, id))?)?;
    normalize_labels(page);
    let record = record(page, &bundle)?;
    page.set("sources", okf_sources(&record));
    let model = bundle["generation_model"].as_str().unwrap_or("unknown");
    page.set(
        "generated",
        json!({"by":format!("lawrenceanum/{model}"),"at":chrono::Utc::now().to_rfc3339()}),
    );
    page.set("status", json!("draft"));
    // Source text matching does not constitute verification of the generated explanation.
    page.frontmatter.retain(|(k, _)| k != "verified");
    page.set("rag", record);
    Ok(())
}

pub fn write_export(
    wiki: &Path,
    cases: &[(String, Page)],
    related: &BTreeMap<String, Vec<hanrei::RelatedCase>>,
    out: &Path,
) -> Result<()> {
    use std::io::Write;
    let mut writer = std::io::BufWriter::new(std::fs::File::create(out.join("hanrei-rag.jsonl"))?);
    let mut versions = Vec::new();
    let mut chunk_count = 0;
    for (rel, page) in cases {
        let id = page
            .get("case_id")
            .and_then(Value::as_u64)
            .context("missing case ID")?;
        let bundle: Value =
            serde_json::from_slice(&std::fs::read(hanrei::snapshot_path(wiki, id))?)?;
        let mut record = record(page, &bundle)?;
        let related: Vec<_> = serde_json::to_value(related.get(rel))?.as_array().into_iter().flatten().map(|r|
            json!({"type":"shares_article","to":format!("lawrenceanum:hanrei:{}",r["case_id"]),"common_articles":r["common_articles"],"precedent_status":"unconfirmed"})).collect();
        record["related_cases"] = json!(related);
        record.as_object_mut().unwrap().remove("revision_hash");
        record["revision_hash"] = json!(hash(&record));
        versions.push(
            json!({"document_id":record["document_id"],"revision_hash":record["revision_hash"]}),
        );
        chunk_count += record["chunks"].as_array().unwrap().len();
        serde_json::to_writer(&mut writer, &record)?;
        writeln!(&mut writer)?;
    }
    writer.flush()?;
    std::fs::write(
        out.join("hanrei-rag-manifest.json"),
        serde_json::to_vec_pretty(&json!({
        "schema_version":1,"format":"jsonl","file":"hanrei-rag.jsonl","document_count":cases.len(),"chunk_count":chunk_count,
        "content_hash":hash(&json!(versions)),"documents":versions,"source_commit":std::env::var("WIKI_SOURCE_SHA").ok(),
        "offset_unit":"unicode_scalar","offset_end":"exclusive","semantic_support":"unverified"}))?,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (Page, Value) {
        let mut page = Page::default();
        page.set("case_id", json!(97208));
        page.set("title", json!("生活保護基準"));
        page.set("court", json!("最高裁判所"));
        page.set("judged_on", json!("令和7年6月27日"));
        page.body = format!("{LLM_BEGIN}\n## 要約\n生活保護基準の改定を違法と判断した。[^1]\n\n## 事案\n{}\n\n[^1]: [hanrei:97208#summary](https://example.test/case) 「基準の改定は違法である」\n{LLM_END}",hanrei::UNKNOWN);
        let bundle = json!({"case":{"case_number":"令和5年1号","judged_on":"令和7年6月27日"},"scope":"裁判要旨のみ",
            "excerpts":[{"ref":"hanrei:97208#summary","url":"https://example.test/case","text":"あ 基準の\n改定は違法である。","scope":"裁判要旨"}],
            "laws":[{"law_id":"L1","articles":["8"],"versions":[],"incomplete":true}],"summary_format_version":4,"prompt_version":"v4","generation_model":"test-model"});
        (page, bundle)
    }
    #[test]
    fn chunks_have_context_stable_ids_citations_and_no_unknown_boilerplate() {
        let (page, mut bundle) = fixture();
        let record1 = record(&page, &bundle).unwrap();
        assert_eq!(record1["chunks"].as_array().unwrap().len(), 1);
        assert_eq!(
            record1["chunks"][0]["id"],
            "lawrenceanum:hanrei:97208#summary"
        );
        assert!(record1["chunks"][0]["retrieval_text"]
            .as_str()
            .unwrap()
            .contains("最高裁判所"));
        assert!(!record1["chunks"][0]["retrieval_text"]
            .as_str()
            .unwrap()
            .contains("[^"));
        assert_eq!(
            record1["evidence"][0]["ranges"][0],
            json!({"start_char":2,"end_char":14})
        );
        assert_eq!(record1["relations"][0]["application_status"], "unconfirmed");
        bundle["captured_on"] = json!("2030-01-01");
        assert_eq!(record1, record(&page, &bundle).unwrap());
        bundle["excerpts"][0]["text"] = json!("偽の資料本文です");
        assert!(record(&page, &bundle).is_err());
    }
    #[test]
    fn invalid_and_duplicate_citations_are_rejected_and_paragraphs_require_own_sources() {
        let (mut page, bundle) = fixture();
        page.body = page.body.replace("## 事案", "引用のない説明。\n\n## 事案");
        assert!(paragraph_errors(&page)
            .iter()
            .any(|e| e.contains("各説明段落")));
        page.body = page.body.replace("[^1]\n", "[^missing]\n");
        assert!(paragraph_errors(&page)
            .iter()
            .any(|e| e.contains("定義がありません")));
        let (mut page, _) = fixture();
        page.body=page.body.replace(LLM_END,"[^1]: [hanrei:97208#summary](https://example.test/case) 「基準の改定は違法である」\n<!-- llm:end -->");
        assert!(record(&page, &bundle).is_err());
    }
    #[test]
    fn footnotes_normalize_without_changing_prose_or_source_identity() {
        let (mut page, bundle) = fixture();
        page.body = page.body.replace("[^1]\n", "[^1][^2]\n").replace(LLM_END,
            &format!("[^2]: [hanrei:97208#summary](https://example.test/case) 「基準の改定は違法である」\n{LLM_END}"));
        normalize_labels(&mut page);
        let first = record(&page, &bundle).unwrap();
        normalize_labels(&mut page);
        assert_eq!(first, record(&page, &bundle).unwrap());
        assert!(first["evidence"][0]["id"]
            .as_str()
            .unwrap()
            .starts_with("src-"));
        assert_eq!(
            first["chunks"][0]["text"],
            "生活保護基準の改定を違法と判断した。"
        );
    }
}
