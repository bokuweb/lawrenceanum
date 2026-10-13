//! 蓄積した官報ページ（`lawpub kanpo-archive` の出力）を一括で解析し、怪しいページを洗い出す。
//!
//! ```text
//! cargo run -p kanpo-parser --example scan -- <archive_dir> > scan.tsv
//! ```
//!
//! 1 ページ 1 行の TSV（パス・形式・レイアウト・語数・捨てた語・フラグ）を出し、
//! 標準エラーに「1em 四方の単独語としてだけ現れ、通常の語の中には一度も出ない漢字」を
//! 出す。ToUnicode 無しの私用 CID は別フォントで組まれて単独語になり、無関係な漢字に
//! 化けるので、その候補になる。

use kanpo_parser::trace::{Role, WordStatus};
use std::collections::BTreeMap;
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
    let root = std::env::args().nth(1).expect("usage: scan <archive_dir>");
    let mut files = Vec::new();
    walk(Path::new(&root), &mut files);
    files.sort();
    // 漢字 -> (単独語としての出現数, 例, 通常の語の中での出現数)
    let mut lone: BTreeMap<char, (usize, String, usize)> = BTreeMap::new();
    println!("path\tformat\tlayout\twords\tnoise\tflags");
    for f in files {
        let xhtml = std::fs::read_to_string(&f).unwrap_or_default();
        let a = kanpo_parser::analyze(&xhtml);
        let Some(page) = a.pages.first() else { continue };
        let mut flags: Vec<String> = Vec::new();
        let count = |s| page.words.iter().filter(|w| w.status == s).count();
        let used = count(WordStatus::Used);
        let noise = count(WordStatus::Noise);
        // 横組みの語（幅が高さより十分大きい）が多いページ。縦書き前提の復元が合わない。
        let horizontal = page
            .words
            .iter()
            .filter(|w| w.status == WordStatus::Used && (w.x1 - w.x0) > (w.y1 - w.y0) * 2.0 && w.text.chars().count() >= 3)
            .count();
        if horizontal * 5 > used.max(1) {
            flags.push(format!("horizontal:{horizontal}"));
        }
        // 欄見出しラベルが出ているのに新旧対照表と判定されない／その逆。
        let labels = page.words.iter().filter(|w| w.status == WordStatus::HeaderLabel).count();
        let raw_has_header = page.words.iter().any(|w| w.text.contains("改正後")) || labels > 0;
        let has_bands = page.regions.iter().any(|r| r.role == Role::After);
        if raw_has_header && !has_bands {
            flags.push("header-but-no-bands".into());
        }
        if has_bands {
            let n = |role| page.regions.iter().filter(|r| r.role == role).map(|r| r.lines.len()).sum::<usize>();
            let (after, before) = (n(Role::After), n(Role::Before));
            if after == 0 || before == 0 || after.max(before) > 4 * after.min(before).max(1) {
                flags.push(format!("unbalanced:{after}/{before}"));
            }
            if page.layout == "headerless_shinkyu" {
                flags.push("headerless".into());
            }
            if page.regions.iter().any(|r| r.role == Role::After && r.part > 0) {
                flags.push("continuation-tier".into());
            }
            if page.regions.iter().any(|r| r.role == Role::Outside) {
                flags.push("outside".into());
            }
        }
        if noise > 0 {
            flags.push(format!("noise:{noise}"));
        }
        // 記事見出し（〇…）の数と記事分割の数。
        let heads = a.text.lines().filter(|l| l.starts_with('〇')).count();
        if heads > 0 && a.articles.len() < heads {
            flags.push(format!("articles:{}<{heads}", a.articles.len()));
        }
        for w in &page.words {
            let kanji: Vec<char> = w.raw.chars().filter(|c| ('\u{4E00}'..='\u{9FFF}').contains(c)).collect();
            let square = ((w.x1 - w.x0) - (w.y1 - w.y0)).abs() < 1.0;
            let n = w.raw.chars().count();
            if square && n <= 2 && kanji.len() == n {
                for c in kanji {
                    let e = lone.entry(c).or_insert((0, f.display().to_string(), 0));
                    e.0 += 1;
                }
            } else {
                for c in kanji {
                    lone.entry(c).or_insert((0, f.display().to_string(), 0)).2 += 1;
                }
            }
        }
        println!("{}\t{}\t{}\t{}\t{}\t{}", f.display(), a.format, page.layout, used, noise, flags.join(","));
    }
    let mut v: Vec<_> = lone.into_iter().filter(|(_, (n, _, inword))| *n >= 2 && *inword == 0).collect();
    v.sort_by_key(|(c, _)| *c);
    eprintln!("--- 単独語としてだけ現れる漢字（私用 CID の化け候補）");
    for (c, (n, ex, _)) in v {
        eprintln!("{c} U+{:04X}\t{n}\t{ex}", c as u32);
    }
}
