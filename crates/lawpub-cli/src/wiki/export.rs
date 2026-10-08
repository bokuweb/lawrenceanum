//! `lawpub wiki-export`: `wiki` ブランチの Markdown を SPA が読む静的 JSON に変換する。
//!
//! ```text
//! {out}/index.json                 全ページの一覧 (type / title / description / date / tags)
//! {out}/graph.json                 ナレッジグラフ (ノード = ページ、エッジ = ページ間リンク)
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

/// グラフに載せないハブページ。
const HUB_PAGES: [&str; 3] = ["index.md", "log.md", "README.md"];

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
    let known: BTreeSet<&str> = pages.iter().map(|(rel, _)| rel.as_str()).collect();

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
        });
        index.push(entry.clone());

        let dest = args.out.join("page").join(format!("{id}.json"));
        std::fs::create_dir_all(dest.parent().expect("has parent"))?;
        std::fs::write(&dest, serde_json::to_vec(&json!({ "path": id, "frontmatter": fm, "body": page.body }))?)?;

        if HUB_PAGES.contains(&rel.as_str()) {
            continue;
        }
        nodes.push(json!({
            "id": id,
            "type": page.get_str("type"),
            "title": page.get_str("title"),
            "description": page.get_str("description"),
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

    std::fs::write(
        args.out.join("index.json"),
        serde_json::to_vec(&json!({ "schema_version": 1, "generated_at": now_rfc3339(), "pages": index }))?,
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
        write("laws/L1.md", "---\ntype: \"law\"\ntitle: \"予防接種法\"\n---\n[会議](../meetings/kokkai/M1.md) [欠落](../laws/none.md)\n");
        write(
            "meetings/kokkai/M1.md",
            "---\ntype: \"meeting\"\ntitle: \"厚生労働委員会\"\ndate: \"2026-10-01\"\n---\n[予防接種法](../../laws/L1.md)\n",
        );
        write(".lawpub/state.md", "ignored");

        run_export(&ExportArgs { wiki, out: out.clone() }).unwrap();

        let index: Value = serde_json::from_slice(&std::fs::read(out.join("index.json")).unwrap()).unwrap();
        assert_eq!(index["pages"].as_array().unwrap().len(), 3);
        let page: Value = serde_json::from_slice(&std::fs::read(out.join("page/laws/L1.json")).unwrap()).unwrap();
        assert_eq!(page["frontmatter"]["title"], "予防接種法");
        assert!(page["body"].as_str().unwrap().contains("[会議]"));
        let graph: Value = serde_json::from_slice(&std::fs::read(out.join("graph.json")).unwrap()).unwrap();
        assert_eq!(graph["nodes"].as_array().unwrap().len(), 2, "index はハブなので除く");
        assert_eq!(graph["links"], json!([{ "source": "laws/L1", "target": "meetings/kokkai/M1" }]));
        std::fs::remove_dir_all(root).ok();
    }
}
