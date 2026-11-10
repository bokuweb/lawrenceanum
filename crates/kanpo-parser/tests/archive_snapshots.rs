//! 蓄積した実際の官報（`tests/archive/`）を入力にした、パーサ全体のスナップショットテスト。
//!
//! `tests/archive/` は `examples/snapshot_sample` が R2 に蓄積した官報から層別に選んだもの
//! （号の種類 × 組み方 × 新旧対照表 × 罫線の表 × 割注 × 私用 CID、と 2〜6 ページの項目）。
//!
//! - `pages/<号>/<ページ>.pdf`: 原本の PDF。本番と同じく kanpo-pdf で座標付きテキストと罫線を
//!   取り出して 1 ページずつ解析し、組み方・本文・記事ごとの構造（段落・新旧対照表の行・表のセル）を
//!   スナップショットにする
//! - `items/<項目>.json`: 項目のページをまとめて解析し（ページをまたぐ表のつなぎ方）、
//!   目次の標題に合う記事とその構造をスナップショットにする
//!
//! パーサを直して出力が変わったら、`cargo insta review -p kanpo-parser`（または
//! `INSTA_UPDATE=always cargo test -p kanpo-parser --test archive_snapshots`）で差分を目視で
//! 確かめてから受け入れる。`tests/snapshots/` がスナップショット。

use kanpo_parser::{Block, Document, PageInput, Run};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

/// 1 ページの PDF を解析の入力にする。
fn read_page(pdf: &Path) -> PageInput {
    let mut pages = kanpo_parser::page_inputs(&std::fs::read(pdf).unwrap()).unwrap();
    assert_eq!(pages.len(), 1, "{}", pdf.display());
    pages.remove(0)
}

fn runs_text(runs: &[Run]) -> String {
    runs.iter().map(|r| r.text.as_str()).collect()
}

/// 複数行の文字列を字下げして書く。
fn indent(out: &mut String, prefix: &str, text: &str) {
    for (i, line) in text.lines().enumerate() {
        out.push_str(if i == 0 { prefix } else { "    " });
        out.push_str(line);
        out.push('\n');
    }
}

fn render_runs(out: &mut String, prefix: &str, runs: &[Run]) {
    let mut text = String::new();
    for r in runs {
        match &r.table {
            Some(t) => {
                if !text.is_empty() {
                    indent(out, prefix, &text);
                    text.clear();
                }
                let cols = t.rows.first().map(|r| r.len()).unwrap_or(0);
                out.push_str(&format!("{prefix}[表 {}x{cols}]\n", t.rows.len()));
                for row in &t.rows {
                    let cells: Vec<String> = row.iter().map(|c| runs_text(c).replace('\n', "⏎")).collect();
                    out.push_str(&format!("      | {} |\n", cells.join(" | ")));
                }
            }
            None => text.push_str(&r.text),
        }
    }
    if !text.is_empty() {
        indent(out, prefix, &text);
    }
}

/// 記事の構造を読みやすく書き下す（段落 P・新旧対照表の行・表のセル）。
fn render_document(out: &mut String, doc: &Document) {
    for b in &doc.blocks {
        match b {
            Block::Paragraph { runs } => render_runs(out, "  P ", runs),
            Block::Shinkyu { rows ,..} => {
                out.push_str(&format!("  [新旧対照表 {} 行]\n", rows.len()));
                for (i, r) in rows.iter().enumerate() {
                    out.push_str(&format!("   row {i}\n"));
                    render_runs(out, "    後 ", &r.after);
                    render_runs(out, "    前 ", &r.before);
                }
            }
        }
    }
}

/// `insta::glob!` は失敗の集計を共有するので、2 つのテストが並列に走ると失敗が他方のテストの
/// ものとして出る。スナップショットとの照合は順に走らせる。
static GLOB: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// `dir` の下の拡張子 `ext` のファイルをすべて集める。
fn files(dir: &Path, ext: &str, out: &mut Vec<PathBuf>) {
    for e in std::fs::read_dir(dir).unwrap() {
        let path = e.unwrap().path();
        if path.is_dir() {
            files(&path, ext, out);
        } else if path.extension().is_some_and(|x| x == ext) {
            out.push(path.canonicalize().unwrap());
        }
    }
}

/// 解析は 1 ファイルずつ独立なので、CPU の数だけのスレッドで先に済ませておく（順に解析すると
/// CI でこのテストだけで 5 分かかっていた）。照合は `insta::glob!` の中で順に行う。
fn render_all(dir: &str, ext: &str, render: impl Fn(&Path) -> String + Sync) -> HashMap<PathBuf, String> {
    let mut paths = Vec::new();
    files(&Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join(dir), ext, &mut paths);
    let next = AtomicUsize::new(0);
    let threads = std::thread::available_parallelism().map_or(1, |n| n.get());
    std::thread::scope(|s| {
        let workers: Vec<_> = (0..threads)
            .map(|_| {
                s.spawn(|| {
                    let mut done = Vec::new();
                    while let Some(path) = paths.get(next.fetch_add(1, Ordering::Relaxed)) {
                        done.push((path.clone(), render(path)));
                    }
                    done
                })
            })
            .collect();
        workers.into_iter().flat_map(|w| w.join().unwrap()).collect()
    })
}

fn rendered<'a>(all: &'a HashMap<PathBuf, String>, path: &Path) -> &'a str {
    &all[&path.canonicalize().unwrap()]
}

fn render_page(path: &Path) -> String {
    let page = read_page(path);
    let a = kanpo_parser::analyze_item(std::slice::from_ref(&page));
    // 1 つの語を 2 つの行（表のセルを含む）に組まない（決算公告の枠が二重に出ていた）。
    for p in &a.pages {
        let mut seen = std::collections::HashSet::new();
        let words = p.regions.iter().flat_map(|r| &r.lines).flat_map(|l| l.words.iter().chain(l.table.iter().flat_map(|t| t.cells.iter().flat_map(|c| &c.words))));
        let dup: Vec<&str> = words.filter(|&&w| !seen.insert(w)).map(|&w| p.words[w].text.as_str()).collect();
        assert!(dup.is_empty(), "{}: 2 回以上組んだ語 {dup:?}", path.display());
    }
    let mut out = String::new();
    let layouts: Vec<&str> = a.pages.iter().map(|p| p.layout).collect();
    out.push_str(&format!("layout: {}\nformat: {}\n\n--- 本文 ---\n{}\n", layouts.join(","), a.format, a.text));
    out.push_str("\n--- 記事 ---\n");
    for (i, art) in a.articles.iter().enumerate() {
        out.push_str(&format!("## {i} ({})\n", art.document.format));
        render_document(&mut out, &art.document);
    }
    out
}

fn render_item(path: &Path) -> String {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/archive/pages");
    let item: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let title = item["title"].as_str().unwrap();
    let pages: Vec<PageInput> = item["pages"].as_array().unwrap().iter().map(|p| read_page(&root.join(format!("{}.pdf", p.as_str().unwrap())))).collect();
    let a = kanpo_parser::analyze_item(&pages);
    let picked = kanpo_parser::pick_article(title, &a.articles);
    let mut out = format!("title: {title}\npages: {}\narticles: {}\npicked: {picked:?}\n\n", pages.len(), a.articles.len());
    for (i, art) in a.articles.iter().enumerate() {
        if Some(i) == picked {
            out.push_str(&format!("## {i} ({}) ← 標題の記事\n", art.document.format));
            render_document(&mut out, &art.document);
        } else {
            let head: String = art.text.lines().next().unwrap_or_default().chars().take(60).collect();
            out.push_str(&format!("## {i}: {head}\n"));
        }
    }
    out
}

#[test]
fn archive_pages() {
    let all = render_all("archive/pages", "pdf", render_page);
    let _guard = GLOB.lock().unwrap_or_else(|e| e.into_inner());
    insta::glob!("archive/pages/**/*.pdf", |path| {
        insta::assert_snapshot!(rendered(&all, path));
    });
}

#[test]
fn archive_items() {
    let all = render_all("archive/items", "json", render_item);
    let _guard = GLOB.lock().unwrap_or_else(|e| e.into_inner());
    insta::glob!("archive/items/*.json", |path| {
        insta::assert_snapshot!(rendered(&all, path));
    });
}
