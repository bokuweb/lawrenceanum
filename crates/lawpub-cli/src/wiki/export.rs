//! `lawpub wiki-export`: `wiki` ブランチの Markdown を SPA が読む静的 JSON に変換する。
//!
//! ```text
//! {out}/index.json                 全ページの一覧 (type / title / description / date / tags)
//! {out}/graph.json                 ナレッジグラフ (ノード = ページ、エッジ = ページ間リンク)
//! {out}/report-manifest.json       資料用保存版 (ページ・主張・根拠の版と参照関係)
//! {out}/page/{path 拡張子なし}.json frontmatter と本文 (Markdown)
//! ```
//!
//! JSON にしておけば配信時の gzip 事前圧縮 (`lawpub compress`) と SPA の `getJson` に
//! そのまま乗る。`index.md` と `log.md` は全ページへのハブなのでグラフから外す。

use super::check::relative_links;
use super::*;
use serde_json::json;
use std::collections::BTreeSet;

pub struct ExportArgs {
    pub wiki: PathBuf,
    pub out: PathBuf,
}

/// LLM の文章 (要点・経緯・概要) がまだ無く、機械が作った一覧しかないページ。
/// アプリでは一覧・グラフ・導線から外し、開いたときは要約前であることを示す。
///
/// 人物・会議体・会派は対象にしない。これらは「要約待ちの雛形」ではなく、発言者名簿・議案から
/// 丸ごと生成する一覧そのものがページの中身で、LLM の経緯は付けば嬉しいおまけだからである
/// (コーパスの全会議を対象にしたことで、経緯の付かないページが大多数になった)。
pub(crate) fn is_stub(page: &Page) -> bool {
    matches!(page.get_str("type"), "law" | "bill" | "pubcomment" | "kanpo" | "meeting" | "hanrei")
        && llm_blocks(&page.body).iter().all(|b| b.trim().is_empty())
}

/// アプリに出す本文。LLM 区間が空のままの節 (`## 全体像` など、見出しの直後が空の区間) は
/// 見出しごと落とす。人物ページの大多数は経緯がまだ無く、空の見出しだけが並ぶため。
pub(crate) fn display_body(body: &str) -> String {
    let lines: Vec<&str> = body.lines().collect();
    let mut out: Vec<&str> = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        if lines[i].starts_with("## ") {
            let mut j = i + 1;
            while j < lines.len() && lines[j].trim().is_empty() {
                j += 1;
            }
            if lines.get(j).map(|l| l.trim()) == Some(LLM_BEGIN) && lines.get(j + 1).map(|l| l.trim()) == Some(LLM_END) {
                i = j + 2;
                while i < lines.len() && lines[i].trim().is_empty() {
                    i += 1;
                }
                continue;
            }
        }
        out.push(lines[i]);
        i += 1;
    }
    let mut s = out.join("\n");
    if body.ends_with('\n') {
        s.push('\n');
    }
    s
}

/// グラフに載せないハブページ。
pub(crate) const HUB_PAGES: [&str; 3] = ["index.md", "log.md", "README.md"];

/// `from` ページ (wiki ルート相対) からの相対リンクを wiki ルート相対のパスに解決する。
pub(crate) fn resolve_link(from: &str, target: &str) -> Option<String> {
    let mut parts: Vec<&str> = from.split('/').collect();
    parts.pop();
    for seg in target.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            s => parts.push(s),
        }
    }
    Some(parts.join("/"))
}

/// 本文の直接引用を会議単位にまとめる。要約の版との一致や意味的な支持は推定しない。
fn citation_sources(rel: &str, page: &Page, known: &BTreeMap<&str, &Page>) -> Vec<Value> {
    let mut grouped: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for citation in check::citations_in(&llm_blocks(&page.body).join("\n")) {
        if let Some(path) = evidence::citation_meeting_path(&citation.reference)
            .filter(|path| path != rel.trim_end_matches(".md"))
        {
            grouped.entry(path).or_default().insert(citation.reference);
        }
    }
    grouped.into_iter().map(|(path, refs)| {
        let target = known.get(format!("{path}.md").as_str())
            .filter(|p| p.get_str("type") == "meeting");
        json!({
            "path": path, "refs": refs,
            "title": target.map(|p| p.get_str("title")),
            "date": target.and_then(|p| p.get("date")),
            "availability": match target { None => "missing", Some(p) if is_stub(p) => "stub", _ => "available" },
            "summary_source_status": target.map(|p| evidence::summary_source_status(p)),
        })
    }).collect()
}

pub fn run_export(args: &ExportArgs) -> Result<()> {
    let wiki = &args.wiki;
    if args.out.exists() {
        std::fs::remove_dir_all(&args.out)?;
    }
    std::fs::create_dir_all(&args.out)?;

    let mut pages = Vec::new();
    for path in walk_md(wiki, "") {
        let rel = rel_path(wiki, &path);
        if rel.starts_with('.') || rel.contains("/.") {
            continue;
        }
        let page = match Page::read(&path) {
            Ok(p) => p,
            Err(e) => {
                tracing::warn!("wiki-export: skip {rel}: {e:#}");
                continue;
            }
        };
        pages.push((rel, page));
    }
    let published = enforcements::export(&pages, &args.out)?;
    // 未完了の施行予定解説は通常の wiki 一覧にも公開しない。
    pages.retain(|(rel, p)| p.get_str("type") != enforcements::KIND || published.contains(rel));
    pages.retain(|(_,p)| p.get_str("type") != hanrei::KIND || hanrei::ready(p));
    report::write_manifest(&pages, &args.out.join("report-manifest.json"))?;
    let known: BTreeSet<&str> = pages.iter().map(|(rel, _)| rel.as_str()).collect();
    let page_lookup: BTreeMap<&str, &Page> = pages.iter().map(|(rel, page)| (rel.as_str(), page)).collect();

    let mut index = Vec::new();
    let mut nodes = Vec::new();
    let mut links = BTreeSet::new();
    for (rel, page) in &pages {
        let fm: serde_json::Map<String, Value> = page.frontmatter.iter().cloned().collect();
        let id = rel.trim_end_matches(".md");
        let entry = json!({
            "path": id,
            "type": page.get_str("type"),
            "title": page.get_str("title"),
            "description": page.get_str("description"),
            "date": page.get("date").cloned().unwrap_or(Value::Null),
            "tags": page.get("tags").cloned().unwrap_or(json!([])),
            "stub": is_stub(page),
        });
        index.push(entry.clone());

        let dest = args.out.join("page").join(format!("{id}.json"));
        std::fs::create_dir_all(dest.parent().expect("has parent"))?;
        std::fs::write(&dest, serde_json::to_vec(&json!({ "path": id, "frontmatter": fm, "body": display_body(&page.body), "stub": is_stub(page), "claim_assessments": evidence::assessments(page), "citation_sources": citation_sources(rel, page, &page_lookup) }))?)?;

        if HUB_PAGES.contains(&rel.as_str()) {
            continue;
        }
        // 要約前のページも載せ、アプリ側で既定は隠す (切り替えで表示できる)。
        nodes.push(json!({
            "id": id,
            "type": page.get_str("type"),
            "title": page.get_str("title"),
            "description": page.get_str("description"),
            "stub": is_stub(page),
        }));
        for target in relative_links(&page.body) {
            let Some(to) = resolve_link(rel, &target) else { continue };
            if to == *rel || HUB_PAGES.contains(&to.as_str()) || !known.contains(to.as_str()) {
                continue;
            }
            // 無向グラフとして重複を除く。
            let (a, b) = if rel.as_str() < to.as_str() { (rel.clone(), to) } else { (to, rel.clone()) };
            links.insert((a, b));
        }
    }
    let links: Vec<Value> = links
        .into_iter()
        .map(|(a, b)| json!({ "source": a.trim_end_matches(".md"), "target": b.trim_end_matches(".md") }))
        .collect();

    let intake_path = wiki.join(".lawpub/intake.json");
    let intake: Value = if intake_path.exists() { serde_json::from_slice(&std::fs::read(intake_path)?)? } else { Value::Null };
    std::fs::write(
        args.out.join("index.json"),
        serde_json::to_vec(&json!({ "schema_version": 1, "generated_at": now_rfc3339(), "pages": index, "intake": intake }))?,
    )?;
    std::fs::write(
        args.out.join("graph.json"),
        serde_json::to_vec(&json!({ "schema_version": 1, "nodes": nodes, "links": links }))?,
    )?;
    println!("wiki-export: {} pages, {} nodes, {} links", pages.len(), nodes.len(), links.len());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_link_handles_parent_segments() {
        assert_eq!(resolve_link("laws/L1.md", "../meetings/kokkai/M1.md").as_deref(), Some("meetings/kokkai/M1.md"));
        assert_eq!(resolve_link("meetings/kokkai/M1.md", "../../laws/L1.md").as_deref(), Some("laws/L1.md"));
        assert_eq!(resolve_link("index.md", "laws/L1.md").as_deref(), Some("laws/L1.md"));
        assert_eq!(resolve_link("index.md", "../x.md"), None);
    }

    #[test]
    fn empty_llm_sections_are_hidden_from_the_app() {
        let body = person_body("高市早苗", "会派: X", "", "### 2026-07-27 予算委員会\n- 答弁した[^1]。", "", "| 一覧 |\n");
        let shown = display_body(&body);
        assert!(!shown.contains("## 全体像"), "{shown}");
        assert!(shown.contains("## 発言と経緯\n\n<!-- llm:begin -->\n### 2026-07-27"), "{shown}");
        assert!(shown.contains("会派: X\n\n## 発言と経緯"), "{shown}");
        assert!(shown.contains("## 発言した会議（一覧）"));
        let empty = display_body(&person_body("山田太郎", "", "", "", "", ""));
        assert!(!empty.contains("## 全体像") && !empty.contains("## 発言と経緯"), "{empty}");
    }

    #[test]
    fn export_writes_index_pages_and_graph_without_hubs() {
        let root = temp_dir("export");
        let wiki = root.join("wiki");
        let out = root.join("out");
        let write = |rel: &str, s: &str| {
            let p = wiki.join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, s).unwrap();
        };
        write("index.md", "---\ntype: \"index\"\ntitle: \"wiki\"\n---\n[L1](laws/L1.md)\n");
        write("laws/L1.md", "---\ntype: \"law\"\ntitle: \"予防接種法\"\n---\n<!-- llm:begin -->\n経緯\n<!-- llm:end -->\n[会議](../meetings/kokkai/M1.md) [欠落](../laws/none.md) [議案](../bills/221/B1.md)\n");
        // LLM の文章が無い (一覧だけの) 議案ページは要約前としてグラフに載せない。
        write("bills/221/B1.md", "---\ntype: \"bill\"\ntitle: \"議案\"\n---\n<!-- llm:begin -->\n<!-- llm:end -->\n[予防接種法](../../laws/L1.md)\n");
        write(
            "meetings/kokkai/M1.md",
            "---\ntype: \"meeting\"\ntitle: \"厚生労働委員会\"\ndate: \"2026-10-01\"\n---\n<!-- llm:begin -->\n要点\n<!-- llm:end -->\n[予防接種法](../../laws/L1.md)\n",
        );
        // 人物ページは LLM の経緯が無くても一覧そのものが中身なので、グラフに載せる。
        write(
            "people/山田太郎.md",
            "---\ntype: \"person\"\ntitle: \"山田太郎\"\n---\n<!-- llm:begin -->\n<!-- llm:end -->\n[予防接種法](../laws/L1.md)\n",
        );
        write(".lawpub/state.md", "ignored");

        run_export(&ExportArgs { wiki, out: out.clone() }).unwrap();

        let index: Value = serde_json::from_slice(&std::fs::read(out.join("index.json")).unwrap()).unwrap();
        assert_eq!(index["pages"].as_array().unwrap().len(), 5);
        let stub_of = |path: &str| index["pages"].as_array().unwrap().iter().find(|p| p["path"] == path).unwrap()["stub"].clone();
        assert_eq!(stub_of("bills/221/B1"), json!(true));
        assert_eq!(stub_of("laws/L1"), json!(false));
        assert_eq!(stub_of("people/山田太郎"), json!(false), "人物ページは経緯が無くても一覧が中身");
        let page: Value = serde_json::from_slice(&std::fs::read(out.join("page/laws/L1.json")).unwrap()).unwrap();
        assert_eq!(page["frontmatter"]["title"], "予防接種法");
        assert!(page["body"].as_str().unwrap().contains("[会議]"));
        let graph: Value = serde_json::from_slice(&std::fs::read(out.join("graph.json")).unwrap()).unwrap();
        let nodes = graph["nodes"].as_array().unwrap();
        assert_eq!(nodes.len(), 4, "index (ハブ) は除き、要約前の議案は stub 付きで載せる");
        assert_eq!(nodes.iter().find(|n| n["id"] == "bills/221/B1").unwrap()["stub"], json!(true));
        assert_eq!(
            graph["links"],
            json!([
                { "source": "bills/221/B1", "target": "laws/L1" },
                { "source": "laws/L1", "target": "meetings/kokkai/M1" },
                { "source": "laws/L1", "target": "people/山田太郎" }
            ])
        );
        let report: Value = serde_json::from_slice(&std::fs::read(out.join("report-manifest.json")).unwrap()).unwrap();
        assert_eq!(report["pages"].as_object().unwrap().len(), 4);
        assert!(report["pages"].get("index").is_none());
        assert_eq!(report["pages"]["bills/221/B1"]["content"]["stub"], true);
        assert_eq!(report["pages"]["laws/L1"]["content"]["references"], json!(["bills/221/B1", "laws/none", "meetings/kokkai/M1"]));
        assert_eq!(report["pages"]["laws/L1"]["content"]["body"], page["body"]);
        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn exported_citation_sources_group_refs_and_preserve_source_availability() {
        let root = temp_dir("export-citation-sources");
        let wiki = root.join("wiki");
        let out = root.join("out");
        let mut law = Page {
            body: format!("{LLM_BEGIN}\n経緯[^1][^2][^3][^4][^5]。\n\n[^1]: [kokkai:M1_001](https://example.com/1) 「引用」\n[^2]: [kokkai:M1_002](https://example.com/2) 「引用」\n[^3]: [shingikai:S1#0](https://example.com/3) 「引用」\n[^4]: [shingikai:missing#0](https://example.com/4) 「引用」\n[^5]: [gian:B1#0](https://example.com/5) 「引用」\n{LLM_END}\n[^6]: [kokkai:outside_001](https://example.com/6) 「機械区間」"),
            ..Page::default()
        };
        law.set("type", json!("law"));
        law.write(&wiki.join("laws/L.md")).unwrap();
        let mut source = Page { body: format!("{LLM_BEGIN}\n要点\n{LLM_END}"), ..Page::default() };
        source.set("type", json!("meeting"));
        source.set("title", json!("国会の会議"));
        source.set("date", json!("2026-10-01"));
        source.set("source_fingerprint", json!("new"));
        source.set("llm_source_fingerprint", json!("old"));
        source.write(&wiki.join("meetings/kokkai/M1.md")).unwrap();
        let mut stub = Page::default();
        stub.set("type", json!("meeting"));
        stub.set("source_fingerprint", json!("new"));
        stub.write(&wiki.join("meetings/shingikai/S1.md")).unwrap();
        run_export(&ExportArgs { wiki: wiki.clone(), out: out.clone() }).unwrap();
        let read = || -> Value { serde_json::from_slice(&std::fs::read(out.join("page/laws/L.json")).unwrap()).unwrap() };
        let exported = read();
        let sources = exported["citation_sources"].as_array().unwrap();
        assert_eq!(sources.len(), 3, "議案や機械区間の脚注から会議を推測しない");
        assert_eq!(sources[0]["refs"], json!(["kokkai:M1_001", "kokkai:M1_002"]));
        assert_eq!(sources[0]["title"], "国会の会議");
        assert_eq!(sources[0]["date"], "2026-10-01");
        assert_eq!(sources[0]["availability"], "available");
        assert_eq!(sources[0]["summary_source_status"], "stale");
        assert_eq!(sources[1]["availability"], "stub");
        assert_eq!(sources[1]["summary_source_status"], "pending");
        assert_eq!(sources[2]["availability"], "missing");
        assert!(sources[2]["summary_source_status"].is_null());
        // 自分の発言の引用で、自分への導線は作らない。
        assert!(citation_sources("meetings/kokkai/M1.md", &law, &BTreeMap::new())
            .iter().all(|s| s["path"] != "meetings/kokkai/M1"));
        // 会議の更新状態だけが変わっても法令の本文には手を入れない。
        source.set("llm_source_fingerprint", json!("new"));
        source.write(&wiki.join("meetings/kokkai/M1.md")).unwrap();
        run_export(&ExportArgs { wiki, out: out.clone() }).unwrap();
        let updated = read();
        assert_eq!(updated["citation_sources"][0]["summary_source_status"], "current");
        assert_eq!(updated["body"], exported["body"]);
        std::fs::remove_dir_all(root).unwrap();
    }
}
