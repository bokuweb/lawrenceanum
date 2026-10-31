//! 官報の項目ページ `kanpo/{date}/{slug}.md` (type: kanpo) を作り、改め文の要約を LLM に渡す。
//!
//! - `item_from`: `kanpo/{date}/index.json` の 1 項目から、ページに要る情報を取り出す。対象は、改め文の本文
//!   (`amend_text`) が取れていて、改正対象 (`linked_laws`) か根拠法令 (`basis_laws`) につながる項目だけ。
//!   帰化・破産などの公告は法令につながらないのでページにならない。
//! - `write_page`: 一覧表と、官報の本文 (自動抽出) をページに書く。本文はページ内に保存し、wiki-check は
//!   `kanpo:` の引用をこの本文と照合する。デジタル官報の公開は約 90 日で配信側からも消えうるうえ、
//!   パーサの改善で抽出結果が変わるため、配信側を正本にすると後から検証が落ちる。要約済みのページは本文を固定する。
//! - `plan_tasks`: 未要約のページを新しい順に LLM に渡す。参照は `kanpo:{date}/{slug}`。

use super::*;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::collections::HashMap;

pub const KIND_KANPO: &str = "kanpo";
/// kanpo ページの描画形式。上げると全ページを描き直す (要約済みのページの本文は変えない)。
const RENDER_VERSION: u64 = 1;
/// LLM に渡す本文の上限 (1 項目あたり)。
const MAX_TEXT_CHARS: usize = 12_000;
const SOURCE_FENCE: &str = "````text";
const SOURCE_FENCE_END: &str = "````";

/// ページにする官報の 1 項目。
#[derive(Debug, Clone, PartialEq)]
pub struct KanpoItem {
    pub date: String,
    /// `号外第223号` など。
    pub issue: String,
    pub page_no: Option<u64>,
    pub title: String,
    pub pdf_url: String,
    /// `prose` (改め文) / `shinkyu` (新旧対照表) / `unknown`。
    pub format: String,
    pub text: String,
    pub amends: Vec<String>,
    pub basis: Vec<String>,
}

impl KanpoItem {
    /// `kanpo/2026-10-09/g223-p0002-1a2b3c4d`。同じページに複数の項目が載るので、標題と PDF の hash を足す。
    pub fn slug(&self) -> String {
        let digits: String = self.issue.chars().filter(char::is_ascii_digit).collect();
        let prefix = if self.issue.starts_with("特別号外") {
            "t"
        } else if self.issue.starts_with("号外") {
            "g"
        } else {
            "h"
        };
        let hash = Sha256::digest(format!("{}\u{1f}{}", self.pdf_url, self.title).as_bytes());
        let hash: String = hash.iter().take(4).map(|b| format!("{b:02x}")).collect();
        format!("{prefix}{digits}-p{:04}-{hash}", self.page_no.unwrap_or(0))
    }

    pub fn page(&self) -> String {
        kanpo_page(&self.date, &self.slug())
    }
}

pub fn kanpo_page(date: &str, slug: &str) -> String {
    format!("kanpo/{date}/{slug}.md")
}

/// 号の種別と番号 (`号外第223号`)。
pub fn issue_label(issue: &Value) -> String {
    let kind = match issue["issue_type"].as_str() {
        Some("extra") => "号外",
        Some("special_extra") => "特別号外",
        Some("procurement") => "政府調達",
        _ => "本紙",
    };
    format!("{kind}{}", issue["issue_no"].as_str().unwrap_or(""))
}

/// 1 日分の官報の 1 項目からページの情報を取り出す。本文が無いか、どの法令にもつながらなければ None。
pub fn item_from(date: &str, issue: &Value, item: &Value) -> Option<KanpoItem> {
    let text = item["amend_text"]
        .as_str()
        .filter(|t| !t.trim().is_empty())?;
    let ids = |key: &str| -> Vec<String> {
        item[key]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|l| l["law_id"].as_str().map(String::from))
            .collect()
    };
    let (amends, basis) = (ids("linked_laws"), ids("basis_laws"));
    if amends.is_empty() && basis.is_empty() {
        return None;
    }
    Some(KanpoItem {
        date: date.to_string(),
        issue: issue_label(issue),
        page_no: item["page"].as_u64(),
        title: item["title"].as_str()?.to_string(),
        pdf_url: item["pdf_url"].as_str()?.to_string(),
        format: item["amend_format"]
            .as_str()
            .unwrap_or("unknown")
            .to_string(),
        text: text.to_string(),
        amends,
        basis,
    })
}

fn format_label(format: &str) -> &'static str {
    match format {
        "prose" => "改め文",
        "shinkyu" => "新旧対照表",
        _ => "その他",
    }
}

/// ページに保存した官報の本文 (`lawpub:begin source` のコードブロックの中身)。
pub fn stored_text(body: &str) -> Option<String> {
    let begin = body.find("<!-- lawpub:begin source -->")?;
    let block = &body[begin..body.find("<!-- lawpub:end source -->")?];
    let start = block.find(SOURCE_FENCE)? + SOURCE_FENCE.len();
    let rest = block[start..].strip_prefix('\n').unwrap_or(&block[start..]);
    let end = rest.rfind(SOURCE_FENCE_END)?;
    Some(rest[..end].trim_end_matches('\n').to_string())
}

/// ページを作るか描き直す。LLM の記述・description・tags・要約済みの印は残し、要約済みなら本文も変えない。
/// 法令ページ (`laws/{id}.md`) が無い法令はリンクにしない。戻り値は新規作成したか。
pub fn write_page(wiki: &Path, item: &KanpoItem, titles: &HashMap<String, String>) -> Result<bool> {
    let rel = item.page();
    let path = wiki.join(&rel);
    let previous = if path.exists() {
        Page::read(&path).ok()
    } else {
        None
    };
    let summarized = previous
        .as_ref()
        .is_some_and(|p| p.get("llm_done") == Some(&json!(true)));
    let text = match previous.as_ref().and_then(|p| stored_text(&p.body)) {
        Some(t) if summarized => t,
        _ => item.text.clone(),
    };
    let place = match item.page_no {
        Some(p) => format!("{} {}・{p}ページ", item.date, item.issue),
        None => format!("{} {}", item.date, item.issue),
    };
    let stats = format!("{place}（{}）", format_label(&item.format));
    let law_links = |ids: &[String]| -> String {
        ids.iter()
            .map(|id| {
                let title = titles.get(id).map(String::as_str).unwrap_or(id);
                if wiki.join(law_page(id)).exists() {
                    format!("[{title}]({})", rel_link(&rel, &law_page(id)))
                } else {
                    title.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join("、")
    };
    let mut meta = String::from("## 官報の情報（一覧）\n\n| 項目 | 内容 |\n|---|---|\n");
    meta.push_str(&format!("| 掲載 | {place} |\n"));
    meta.push_str(&format!("| 形式 | {} |\n", format_label(&item.format)));
    if !item.amends.is_empty() {
        meta.push_str(&format!("| 改正対象 | {} |\n", law_links(&item.amends)));
    }
    if !item.basis.is_empty() {
        meta.push_str(&format!("| 根拠法令 | {} |\n", law_links(&item.basis)));
    }
    meta.push_str(&format!("| 原本 | [官報（PDF）]({}) |\n", item.pdf_url));

    let narrative = previous
        .as_ref()
        .map(|p| first_llm_block(&p.body))
        .unwrap_or_default();
    let narrative_block = if narrative.is_empty() {
        format!("{LLM_BEGIN}\n{LLM_END}")
    } else {
        format!("{LLM_BEGIN}\n{narrative}\n{LLM_END}")
    };
    let body = format!(
        "\n# {}\n\n## 改正の内容\n\n{narrative_block}\n\n<!-- lawpub:begin meta -->\n{meta}<!-- lawpub:end meta -->\n\n\
         <!-- lawpub:begin source -->\n## 官報の本文（自動抽出）\n\n官報の PDF から機械的に抽出したテキストです。表の組み方や改行は原本と異なることがあります。\n\n\
         {SOURCE_FENCE}\n{text}\n{SOURCE_FENCE_END}\n<!-- lawpub:end source -->\n",
        item.title
    );
    // description は LLM が書いた 1 行を優先し、無ければ掲載情報にする。
    let description = match previous
        .as_ref()
        .map(|p| p.get_str("description").to_string())
    {
        Some(d)
            if !d.is_empty()
                && previous.as_ref().map(|p| p.get_str("stats")) != Some(d.as_str()) =>
        {
            d
        }
        _ => stats.clone(),
    };
    let fm = vec![
        ("type".into(), json!("kanpo")),
        ("title".into(), json!(item.title)),
        ("description".into(), json!(description)),
        ("stats".into(), json!(stats)),
        ("resource".into(), json!(item.pdf_url)),
        (
            "timestamp".into(),
            previous
                .as_ref()
                .and_then(|p| p.get("timestamp").cloned())
                .unwrap_or(json!(now_rfc3339())),
        ),
        ("date".into(), json!(item.date)),
        ("issue".into(), json!(item.issue)),
        ("page_no".into(), json!(item.page_no)),
        ("format".into(), json!(item.format)),
        ("laws".into(), json!(item.amends)),
        ("basis_laws".into(), json!(item.basis)),
        (
            "tags".into(),
            previous
                .as_ref()
                .and_then(|p| p.get("tags").cloned())
                .unwrap_or(json!([])),
        ),
        ("render_version".into(), json!(RENDER_VERSION)),
        ("llm_done".into(), json!(summarized)),
    ];
    let mut page = Page {
        frontmatter: fm,
        body,
    };
    if let Some(prev) = &previous {
        if prev.frontmatter == page.frontmatter && prev.body == page.body {
            return Ok(false);
        }
        if prev.body != page.body {
            page.set("timestamp", json!(now_rfc3339()));
        }
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    page.write(&path)?;
    Ok(previous.is_none())
}

/// kanpo ページの引用単位 (ページに保存した本文 1 つ)。
pub fn kanpo_unit(wiki: &Path, reference: &str) -> Result<Option<Unit>> {
    let Some(id) = reference.strip_prefix(&format!("{KIND_KANPO}:")) else {
        return Ok(None);
    };
    let Some((date, slug)) = id.split_once('/') else {
        return Ok(None);
    };
    let path = wiki.join(kanpo_page(date, slug));
    if !path.exists() {
        return Ok(None);
    }
    let page = Page::read(&path)?;
    Ok(stored_text(&page.body).map(|text| Unit {
        reference: reference.to_string(),
        url: page.get_str("resource").to_string(),
        speaker: None,
        group: None,
        position: None,
        text,
    }))
}

/// 未要約の kanpo ページを新しい順に最大 `max` 件タスクにする。
pub fn plan_tasks(wiki: &Path, work: &Path, max: usize) -> Result<Vec<Task>> {
    if max == 0 {
        return Ok(Vec::new());
    }
    let mut pending: Vec<(String, PathBuf, Page)> = Vec::new();
    for path in walk_md(wiki, "kanpo") {
        let page = Page::read(&path)?;
        if page.get("llm_done") == Some(&json!(true)) {
            continue;
        }
        pending.push((page.get_str("date").to_string(), path, page));
    }
    pending.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));

    let mut tasks = Vec::new();
    for (date, path, page) in pending.into_iter().take(max) {
        let rel = rel_path(wiki, &path);
        let Some(text) = stored_text(&page.body) else {
            continue;
        };
        let slug = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();
        let reference = format!("{KIND_KANPO}:{date}/{slug}");
        let law_ids = |key: &str| -> Vec<String> {
            page.get(key)
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        };
        let (amends, basis) = (law_ids("laws"), law_ids("basis_laws"));
        // 経緯を書き足す法令ページ (wiki にあるものだけ)。
        let laws: Vec<TaskLaw> = amends
            .iter()
            .chain(&basis)
            .filter(|id| wiki.join(law_page(id)).exists())
            .map(|id| {
                let _ = ensure_law_synthesis(wiki, id);
                let lp = law_page(id);
                let title = Page::read(&wiki.join(&lp))
                    .map(|p| p.get_str("title").to_string())
                    .unwrap_or_default();
                TaskLaw {
                    law_id: id.clone(),
                    title,
                    page: lp,
                }
            })
            .collect();
        let chars: Vec<char> = text.chars().collect();
        let mut excerpt: String = chars.iter().take(MAX_TEXT_CHARS).collect();
        if chars.len() > MAX_TEXT_CHARS {
            excerpt.push('…');
        }
        let source_rel = format!("sources/kanpo_{date}_{slug}.json");
        std::fs::create_dir_all(work.join("sources"))?;
        let bundle = json!({
            "kind": KIND_KANPO,
            "id": reference,
            "title": page.get_str("title"),
            "date": date,
            "issue": page.get_str("issue"),
            "page_no": page.get("page_no"),
            "format": page.get_str("format"),
            "amends": amends,
            "basis_laws": basis,
            "laws": laws,
            "excerpts": [{"ref": reference, "url": page.get_str("resource"), "label": "官報の本文", "text": excerpt}],
        });
        std::fs::write(
            work.join(&source_rel),
            serde_json::to_string_pretty(&bundle)?,
        )?;
        tasks.push(Task {
            key: reference.clone(),
            kind: KIND_KANPO.into(),
            id: format!("{date}/{slug}"),
            date: date.clone(),
            title: page.get_str("title").to_string(),
            page: rel,
            source: source_rel,
            laws,
            people: Vec::new(),
            committee: None,
            created: Vec::new(),
            person: None,
        });
    }
    Ok(tasks)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day() -> Value {
        json!({"date": "2026-10-09", "issues": [{
        "issue_type": "extra", "issue_no": "第223号", "items": [
            {"title": "輸出貿易管理令の一部を改正する政令（三二五）", "page": 2, "pdf_url": "https://k/p2.pdf",
             "amend_format": "prose", "amend_text": "輸出貿易管理令の一部を次のように改正する。\n別表第一を改める。\n附則\nこの政令は、公布の日から施行する。",
             "linked_laws": [{"law_id": "L1", "title": "輸出貿易管理令", "match": "title"}]},
            {"title": "目次に載るだけの項目", "page": 3, "pdf_url": "https://k/p3.pdf",
             "linked_laws": [{"law_id": "L1", "title": "輸出貿易管理令", "match": "title"}]},
            {"title": "日本国に帰化を許可する件", "page": 7, "pdf_url": "https://k/p7.pdf", "amend_text": "本文"}
        ]}]})
    }

    fn items() -> Vec<KanpoItem> {
        let d = day();
        let issue = &d["issues"][0];
        issue["items"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|i| item_from("2026-10-09", issue, i))
            .collect()
    }

    #[test]
    fn only_items_with_text_and_laws_become_pages() {
        let items = items();
        assert_eq!(
            items.len(),
            1,
            "本文の無い項目と、法令につながらない公告はページにしない"
        );
        assert_eq!(items[0].issue, "号外第223号");
        assert!(items[0].slug().starts_with("g223-p0002-"));
        assert_eq!(
            items[0].page(),
            format!("kanpo/2026-10-09/{}.md", items[0].slug())
        );
    }

    #[test]
    fn page_keeps_text_for_citations_and_freezes_it_once_summarized() {
        let root = temp_dir("kanpo-page");
        let wiki = root.join("wiki");
        let work = root.join("work");
        ensure_law_page(&wiki, "https://example.test", "L1", "輸出貿易管理令").unwrap();
        let titles: HashMap<String, String> =
            [("L1".to_string(), "輸出貿易管理令".to_string())].into();
        let item = items().remove(0);
        assert!(write_page(&wiki, &item, &titles).unwrap());
        let rel = item.page();
        let page = Page::read(&wiki.join(&rel)).unwrap();
        assert_eq!(page.get_str("type"), "kanpo");
        assert_eq!(
            page.get_str("description"),
            "2026-10-09 号外第223号・2ページ（改め文）"
        );
        assert!(page
            .body
            .contains("| 改正対象 | [輸出貿易管理令](../../laws/L1.md) |"));
        assert_eq!(stored_text(&page.body).as_deref(), Some(item.text.as_str()));

        let reference = format!("kanpo:2026-10-09/{}", item.slug());
        let unit = kanpo_unit(&wiki, &reference).unwrap().unwrap();
        assert_eq!(unit.url, "https://k/p2.pdf");
        assert!(unit.text.contains("公布の日から施行する"));
        assert!(kanpo_unit(&wiki, "kanpo:2026-10-09/nothing")
            .unwrap()
            .is_none());

        // 未要約ならタスクになり、ソースバンドルに本文と経緯を書く法令ページが載る。
        let tasks = plan_tasks(&wiki, &work, 3).unwrap();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].key, reference);
        assert_eq!(tasks[0].laws[0].page, "laws/L1.md");
        let bundle: Value =
            serde_json::from_slice(&std::fs::read(work.join(&tasks[0].source)).unwrap()).unwrap();
        assert_eq!(bundle["excerpts"][0]["ref"], reference);

        // 要約済みのページは、パーサの改善で本文が変わっても保存した本文を変えない。LLM の記述も残す。
        let mut page = Page::read(&wiki.join(&rel)).unwrap();
        page.body = page.body.replace(
            &format!("{LLM_BEGIN}\n{LLM_END}"),
            &format!("{LLM_BEGIN}\n要約[^1]\n{LLM_END}"),
        );
        page.set("description", json!("別表第一を改める政令"));
        page.set("llm_done", json!(true));
        page.write(&wiki.join(&rel)).unwrap();
        let mut changed = item.clone();
        changed.text = "抽出し直した本文".into();
        assert!(!write_page(&wiki, &changed, &titles).unwrap());
        let page = Page::read(&wiki.join(&rel)).unwrap();
        assert_eq!(stored_text(&page.body).as_deref(), Some(item.text.as_str()));
        assert_eq!(first_llm_block(&page.body), "要約[^1]");
        assert_eq!(page.get_str("description"), "別表第一を改める政令");
        assert!(plan_tasks(&wiki, &work, 3).unwrap().is_empty());
        std::fs::remove_dir_all(root).ok();
    }
}
