//! 蓄積した官報の 1 項目（日付・号・開始ページ）をまとめて解析し、標題に合う記事の構造化結果を
//! 読みやすく表示する（itemscan で印の付いた項目の確認用）。
//!
//! ```text
//! cargo run --release -p kanpo-parser --example itemshow -- <archive_dir> 2026-07-31 20260731g00172 200 [width] [標題の一部]
//! ```

use kanpo_parser::{Block, PageInput, Run};
use serde_json::Value;

fn show_runs(label: &str, runs: &[Run], w: usize) {
    for r in runs {
        match &r.table {
            Some(t) => {
                println!("    {label} [表 {}x{}]", t.rows.len(), t.rows.first().map(|r| r.len()).unwrap_or(0));
                for row in &t.rows {
                    let cells: Vec<String> = row.iter().map(|c| c.iter().map(|r| r.text.as_str()).collect::<String>().chars().take(w / 3).collect()).collect();
                    println!("      | {}", cells.join(" | "));
                }
            }
            None => {
                for l in r.text.lines() {
                    println!("    {label} {}", l.chars().take(w).collect::<String>());
                }
            }
        }
    }
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let (root, date, dir, start) = (&a[1], &a[2], &a[3], a[4].parse::<u32>().unwrap());
    let w: usize = a.get(5).and_then(|s| s.parse().ok()).unwrap_or(60);
    // 同じページから始まる項目が複数あるときは、6 番目の引数（標題の一部）で選ぶ。
    let title_filter = a.get(6).cloned().unwrap_or_default();
    let day = std::path::Path::new(root).join(date);
    let m: Value = serde_json::from_str(&std::fs::read_to_string(day.join("manifest.json")).unwrap()).unwrap();
    let issue = m["issues"].as_array().unwrap().iter().find(|i| i["dir"] == dir.as_str()).unwrap();
    let mut starts: Vec<u32> = issue["items"].as_array().unwrap().iter().filter_map(|i| i["page"].as_u64()).map(|p| p as u32).collect();
    starts.sort_unstable();
    starts.dedup();
    let page_count = issue["page_count"].as_u64().unwrap_or(0) as u32;
    let end = starts.iter().copied().find(|&p| p > start).unwrap_or(page_count).max(start).min(start + 24);
    let title = issue["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["page"].as_u64() == Some(start as u64) && i["title"].as_str().is_some_and(|t| t.contains(title_filter.as_str())))
        .and_then(|i| i["title"].as_str())
        .unwrap_or("");
    let pages: Vec<PageInput> = (start..=end)
        .filter_map(|n| {
            let stem = day.join(dir).join(format!("{dir}{n:04}"));
            Some(PageInput {
                xhtml: std::fs::read_to_string(stem.with_extension("bbox.html")).ok()?,
                rules: std::fs::read_to_string(stem.with_extension("rules.json")).ok().and_then(|s| serde_json::from_str(&s).ok()),
            })
        })
        .collect();
    let an = kanpo_parser::analyze_item(&pages);
    let picked = kanpo_parser::pick_article(title, &an.articles);
    println!("# {title}  pages {start}..={end}  articles={} picked={picked:?}", an.articles.len());
    for (i, art) in an.articles.iter().enumerate() {
        if picked.is_some() && Some(i) != picked {
            println!("== article {i} (skip): {}", art.text.lines().next().unwrap_or("").chars().take(w).collect::<String>());
            continue;
        }
        println!("== article {i}");
        for b in &art.document.blocks {
            match b {
                Block::Paragraph { runs } => show_runs("P", runs, w),
                Block::Shinkyu { rows } => {
                    println!("  [新旧対照表 {} 行]", rows.len());
                    for (ri, r) in rows.iter().enumerate() {
                        println!("   row {ri}");
                        show_runs("後", &r.after, w);
                        show_runs("前", &r.before, w);
                    }
                }
            }
        }
    }
}
