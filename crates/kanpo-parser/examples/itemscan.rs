//! 蓄積した官報（`lawpub kanpo-archive` の出力）を目次の項目単位で解析し、構造化結果の
//! 怪しい項目に印を付ける（ページまたぎ・表の中の表のエッジケース探し用）。
//!
//! ```text
//! cargo run --release -p kanpo-parser --example itemscan -- [--pdf] <archive_dir> > items.tsv
//! ```
//!
//! `--pdf` なら、座標付きテキストと罫線を原本の PDF から今の kanpo-pdf で作り直す（`lawpub kanpo-reextract` と同じ）。
//!
//! 改正・廃止・制定の項目ごとに、開始ページから次の項目の開始ページまで（最大 25 ページ）を
//! まとめて解析し、標題に合う記事の構造（P=段落, S{n}=新旧対照表 n 行, T=表）と印を出す。

use kanpo_parser::{Block, Document, PageInput, Run};
use serde_json::Value;
use std::path::Path;

fn runs_text(runs: &[Run]) -> String {
    runs.iter().map(|r| r.text.as_str()).collect()
}

fn tables(runs: &[Run]) -> Vec<&kanpo_parser::model::NestedTable> {
    runs.iter().filter_map(|r| r.table.as_ref()).collect()
}

fn flags(doc: &Document) -> (String, Vec<String>) {
    let mut shape = Vec::new();
    let mut flags = Vec::new();
    let mut prev_s = false;
    let mut short_p_between = false;
    let mut all_tables = Vec::new();
    for b in &doc.blocks {
        match b {
            Block::Paragraph { runs } => {
                let t = runs_text(runs);
                shape.push("P".to_string());
                all_tables.extend(tables(runs));
                short_p_between = prev_s && t.chars().count() < 40;
                if t.contains("改正後") && t.lines().any(|l| l.trim() == "改正後" || l.trim() == "改正前") {
                    flags.push("label-in-paragraph".into());
                }
            }
            Block::Shinkyu { rows, .. } => {
                if short_p_between {
                    flags.push("S-shortP-S".into());
                }
                shape.push(format!("S{}", rows.len()));
                for r in rows {
                    all_tables.extend(tables(&r.after));
                    all_tables.extend(tables(&r.before));
                    let (a, bt) = (runs_text(&r.after), runs_text(&r.before));
                    let empty = |runs: &[Run]| runs.is_empty();
                    if empty(&r.after) != empty(&r.before) {
                        flags.push(format!("one-side-empty:{}", if empty(&r.after) { "after" } else { "before" }));
                    }
                    for t in [&a, &bt] {
                        if t.lines().any(|l| l.trim() == "改正後" || l.trim() == "改正前") {
                            flags.push("label-in-cell".into());
                        }
                    }
                }
                prev_s = true;
                continue;
            }
        }
        prev_s = false;
    }
    for t in &all_tables {
        let rows = t.rows.len();
        let cols = t.rows.first().map(|r| r.len()).unwrap_or(0);
        shape.push(format!("T{rows}x{cols}"));
        if rows <= 1 || cols <= 1 {
            flags.push(format!("thin-table:{rows}x{cols}"));
        }
        let longest = t.rows.iter().flatten().map(|c| runs_text(c).chars().count()).max().unwrap_or(0);
        if longest > 400 {
            flags.push(format!("long-cell:{longest}"));
        }
    }
    flags.sort();
    flags.dedup();
    (shape.join(" "), flags)
}

fn main() {
    let mut args = std::env::args().skip(1).peekable();
    let from_pdf = args.next_if(|a| a == "--pdf").is_some();
    let root = args.next().expect("usage: itemscan [--pdf] <archive_dir>");
    let mut days: Vec<_> = std::fs::read_dir(&root).unwrap().flatten().map(|e| e.path()).filter(|p| p.join("manifest.json").exists()).collect();
    days.sort();
    println!("date\tissue\tpage\tpages\tpicked\tshape\tflags\ttitle");
    for day in days {
        let m: Value = serde_json::from_str(&std::fs::read_to_string(day.join("manifest.json")).unwrap()).unwrap();
        for issue in m["issues"].as_array().into_iter().flatten() {
            let dir = issue["dir"].as_str().unwrap_or("");
            let page_count = issue["page_count"].as_u64().unwrap_or(0) as u32;
            let mut starts: Vec<u32> = issue["items"].as_array().into_iter().flatten().filter_map(|i| i["page"].as_u64()).map(|p| p as u32).collect();
            starts.sort_unstable();
            starts.dedup();
            for item in issue["items"].as_array().into_iter().flatten() {
                let title = item["title"].as_str().unwrap_or("");
                if !(title.contains("改正") || title.contains("廃止") || title.contains("制定")) {
                    continue;
                }
                let start = item["page"].as_u64().unwrap_or(0) as u32;
                let end = starts.iter().copied().find(|&p| p > start).unwrap_or(page_count).max(start).min(start + 24);
                let pages: Vec<PageInput> = (start..=end)
                    .filter_map(|n| {
                        let stem = day.join(dir).join(format!("{dir}{n:04}"));
                        if from_pdf {
                            let pdf = std::fs::read(stem.with_extension("pdf")).ok()?;
                            return Some(PageInput { xhtml: kanpo_parser::bbox_xhtml(&pdf).ok()?, rules: kanpo_parser::page_rules(&pdf).ok() });
                        }
                        let xhtml = std::fs::read_to_string(stem.with_extension("bbox.html")).ok()?;
                        let rules = std::fs::read_to_string(stem.with_extension("rules.json")).ok().and_then(|s| serde_json::from_str(&s).ok());
                        Some(PageInput { xhtml, rules })
                    })
                    .collect();
                if pages.is_empty() {
                    continue;
                }
                let a = kanpo_parser::analyze_item(&pages);
                let picked = kanpo_parser::pick_article(title, &a.articles);
                let (shape, mut fl) = match picked {
                    Some(i) => flags(&a.articles[i].document),
                    None => ("-".into(), vec!["no-article".into()]),
                };
                if a.articles.iter().any(|x| x.text.contains(['\u{E000}', '\u{E001}', '\u{E002}'])) {
                    fl.push("marker-leak".into());
                }
                println!(
                    "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
                    Path::new(&day).file_name().unwrap().to_string_lossy(),
                    dir,
                    start,
                    pages.len(),
                    picked.map(|i| i.to_string()).unwrap_or("-".into()),
                    shape,
                    fl.join(","),
                    title
                );
            }
        }
    }
}
