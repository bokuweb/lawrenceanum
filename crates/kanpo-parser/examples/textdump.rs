//! 蓄積した官報（`lawpub kanpo-archive` の出力）の全ページを本番と同じく座標付きテキストと罫線から
//! 解析し、ページごとの本文を `<out_dir>/<ページ>.txt` に書く（パーサを直す前後の出力を比べる用）。
//! 1 つの語が 2 つ以上の行（表のセルを含む）に組まれたページは標準エラーに出す。
//! `--pdf` なら、座標付きテキストと罫線を原本の PDF から今の kanpo-pdf で作り直して解析する
//! （蓄積した座標付きテキストは取得した日の抽出器の出力）。環境変数 `KANPO_TEXTDUMP_CELLS=1` なら、罫線の表を行ごとに
//! 欄を全角の空白でつないだ 1 行にせず、欄ごとに（欄の中の行ごとに）1 行ずつ書く（切れ目の分類器（kanpo-lm）で読み順を
//! 確かめる用。表の欄のつなぎ目を切れ目と見ないように）。
//!
//! ```text
//! cargo run --release -p kanpo-parser --example textdump -- [--pdf] <archive_dir> <out_dir>
//! diff -r before/ after/
//! ```

use std::path::{Path, PathBuf};

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            walk(&p, out);
        } else if p.to_string_lossy().ends_with(".bbox.html") {
            out.push(p);
        }
    }
}

fn main() {
    let mut args = std::env::args().skip(1).peekable();
    let from_pdf = args.next_if(|a| a == "--pdf").is_some();
    let (root, out) = match (args.next(), args.next()) {
        (Some(r), Some(o)) => (PathBuf::from(r), PathBuf::from(o)),
        _ => panic!("usage: textdump [--pdf] <archive_dir> <out_dir>"),
    };
    std::fs::create_dir_all(&out).unwrap();
    let mut files = Vec::new();
    walk(&root, &mut files);
    files.sort();
    let cells = std::env::var_os("KANPO_TEXTDUMP_CELLS").is_some();
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get());
    let chunk = files.len().div_ceil(threads).max(1);
    std::thread::scope(|s| {
        for part in files.chunks(chunk) {
            let out = &out;
            s.spawn(move || {
                for f in part {
                    let (xhtml, rules) = if from_pdf {
                        let pdf = std::fs::read(f.to_string_lossy().replace(".bbox.html", ".pdf")).unwrap_or_default();
                        (kanpo_parser::bbox_xhtml(&pdf).unwrap_or_default(), kanpo_parser::page_rules(&pdf).ok())
                    } else {
                        let rules = std::fs::read_to_string(f.to_string_lossy().replace(".bbox.html", ".rules.json"))
                            .ok()
                            .and_then(|s| serde_json::from_str(&s).ok());
                        (std::fs::read_to_string(f).unwrap_or_default(), rules)
                    };
                    let a = kanpo_parser::analyze_with_rules(&xhtml, &[rules]);
                    for page in &a.pages {
                        let mut seen = std::collections::HashSet::new();
                        let dup = page
                            .regions
                            .iter()
                            .flat_map(|r| &r.lines)
                            .flat_map(|l| l.words.iter().chain(l.table.iter().flat_map(|t| t.cells.iter().flat_map(|c| &c.words))))
                            .filter(|&&w| !seen.insert(w))
                            .count();
                        if dup > 0 {
                            eprintln!("{}\t{dup} 語を 2 回以上組んだ", f.display());
                        }
                    }
                    let text = if cells {
                        a.pages
                            .iter()
                            .flat_map(|p| &p.regions)
                            .flat_map(|r| &r.lines)
                            .flat_map(|l| match &l.table {
                                // 欄の前後に空行を入れる（欄の頭は前の欄の続きではない）。行ごとに左の欄から。
                                Some(t) => {
                                    let mut cs: Vec<_> = t.cells.iter().collect();
                                    cs.sort_by_key(|c| (c.row, c.col));
                                    cs
                                }
                                    .into_iter()
                                    .flat_map(|c| std::iter::once(String::new()).chain(c.text.lines().map(str::to_string)).chain([String::new()]))
                                    .collect(),
                                None => vec![l.text.clone()],
                            })
                            .filter(|l| l.is_empty() || !l.trim().is_empty())
                            .collect::<Vec<_>>()
                            .join("\n")
                    } else {
                        a.text
                    };
                    let name = f.file_name().unwrap().to_string_lossy().replace(".bbox.html", ".txt");
                    std::fs::write(out.join(name), text).unwrap();
                }
            });
        }
    });
    eprintln!("{} pages", files.len());
}
