//! 罫線（`.rules.json`）のあるページを一括で解析し、復元した表を集計する（表の復元の検証用）。
//!
//! ```text
//! cargo run --release -p kanpo-parser --example tablescan -- <archive_dir> > tables.tsv
//! ```
//!
//! 1 ページ 1 行: パス・表の数・最大の表の大きさ・空セルの割合・罫線なしと比べた使われた語の差。

use kanpo_parser::trace::WordStatus;
use std::path::{Path, PathBuf};

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            walk(&p, out);
        } else if p.to_string_lossy().ends_with(".rules.json") {
            out.push(p);
        }
    }
}

fn main() {
    let root = std::env::args().nth(1).expect("usage: tablescan <dir>");
    let mut files = Vec::new();
    walk(Path::new(&root), &mut files);
    files.sort();
    println!("path\ttables\tlargest\tempty_ratio\tused_diff");
    for f in files {
        let bbox = f.to_string_lossy().replace(".rules.json", ".bbox.html");
        let Ok(xhtml) = std::fs::read_to_string(&bbox) else { continue };
        let rules: Option<kanpo_parser::lines::PageRules> =
            std::fs::read_to_string(&f).ok().and_then(|s| serde_json::from_str(&s).ok());
        let with = kanpo_parser::analyze_with_rules(&xhtml, &[rules]);
        let without = kanpo_parser::analyze(&xhtml);
        let tables: Vec<_> = with
            .pages
            .iter()
            .flat_map(|p| p.regions.iter().flat_map(|r| r.lines.iter().filter_map(|l| l.table.as_ref())))
            .collect();
        let used = |a: &kanpo_parser::Analysis| {
            a.pages.iter().flat_map(|p| &p.words).filter(|w| w.status == WordStatus::Used).count() as i64
        };
        let cells: usize = tables.iter().map(|t| t.cells.len()).sum();
        let empty: usize = tables.iter().map(|t| t.cells.iter().filter(|c| c.text.is_empty()).count()).sum();
        let largest = tables.iter().map(|t| format!("{}x{}", t.rows, t.cols)).max_by_key(|s| s.len()).unwrap_or_default();
        println!(
            "{}\t{}\t{}\t{:.2}\t{}",
            bbox,
            tables.len(),
            largest,
            if cells > 0 { empty as f32 / cells as f32 } else { 0.0 },
            used(&with) - used(&without)
        );
    }
}
