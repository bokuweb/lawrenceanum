//! 隣り合う字の空きと、poppler（`<pdf>.poppler.html`）が同じ語にしたかを集計する（語の切れ目の調査用）。
//!
//! cargo run --release -p kanpo-pdf --example gap_stats -- <PDF のあるディレクトリ>

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

fn pdfs(dir: &Path, out: &mut Vec<PathBuf>) {
    for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let p = e.path();
        if p.is_dir() {
            pdfs(&p, out);
        } else if p.extension().is_some_and(|x| x == "pdf") {
            out.push(p);
        }
    }
}

fn words(xhtml: &str) -> Vec<[f64; 4]> {
    xhtml
        .split("<word ")
        .skip(1)
        .filter_map(|part| {
            let attrs = part.split('>').next()?;
            let num = |k: &str| -> Option<f64> {
                let at = attrs.find(&format!("{k}=\""))? + k.len() + 2;
                attrs[at..].split('"').next()?.parse().ok()
            };
            Some([num("xMin")?, num("yMin")?, num("xMax")?, num("yMax")?])
        })
        .collect()
}

fn main() {
    let mut files = Vec::new();
    pdfs(Path::new(&std::env::args().nth(1).unwrap()), &mut files);
    // (向き, 空き(0.01 em), Tc の有無, TJ の詰めの有無) -> (つないだ, 切った)
    let mut stats: BTreeMap<(String, i64, bool, bool), (u32, u32)> = BTreeMap::new();
    for f in files {
        let Ok(xhtml) = std::fs::read_to_string(f.with_extension("poppler.html")) else { continue };
        let Ok(pages) = kanpo_pdf::extract_pages(&std::fs::read(&f).unwrap()) else { continue };
        let Some(page) = pages.first() else { continue };
        let ws = words(&xhtml);
        // poppler の枠（縦書きは原点を左上とする正方形）の中心を含む語。
        let word_of = |g: &kanpo_pdf::Glyph| {
            let (ax, ay) = (g.end_x - g.x, g.end_y - g.y);
            let len = ax.hypot(ay).max(1e-9);
            let (ux, uy) = (ax / len, ay / len);
            let (cx, cy) = if g.vertical {
                (g.x + uy * g.size / 2.0 - ux * g.size / 2.0, g.y - ux * g.size / 2.0 - uy * g.size / 2.0)
            } else {
                ((g.bbox[0] + g.bbox[2]) / 2.0, (g.bbox[1] + g.bbox[3]) / 2.0)
            };
            ws.iter().position(|w| w[0] - 0.01 <= cx && cx <= w[2] + 0.01 && w[1] - 0.01 <= cy && cy <= w[3] + 0.01)
        };
        for w in page.glyphs.windows(2) {
            let (p, g) = (&w[0], &w[1]);
            if p.font != g.font || (p.size - g.size).abs() > 0.01 || g.text.trim().is_empty() || p.text.trim().is_empty() {
                continue;
            }
            let (ax, ay) = (p.end_x - p.x, p.end_y - p.y);
            let len = ax.hypot(ay);
            if len < 1e-9 {
                continue;
            }
            let (ux, uy) = (ax / len, ay / len);
            let (dx, dy) = (g.x - p.end_x, g.y - p.end_y);
            let along = (dx * ux + dy * uy) / g.size;
            let across = (-dx * uy + dy * ux) / g.size;
            if across.abs() > 0.05 || !(-0.05..=0.6).contains(&along) {
                continue;
            }
            let (a, b) = (word_of(p), word_of(g));
            if a.is_none() || b.is_none() {
                continue;
            }
            let dir = if g.vertical { "V" } else if uy.abs() > ux.abs() { "HD" } else { "H" };
            let key = (dir.to_string(), (along * 100.0).round() as i64, p.spacing.abs() > 0.01, g.kern.abs() > 0.01);
            let e = stats.entry(key).or_default();
            if a == b {
                e.0 += 1;
            } else {
                e.1 += 1;
            }
        }
    }
    println!("dir\tgap\tTc\tkern\tjoined\tsplit");
    for ((d, gap, sp, kern), (j, s)) in stats {
        if j + s >= 5 {
            println!("{d}\t{:.2}\t{sp}\t{kern}\t{j}\t{s}", gap as f64 / 100.0);
        }
    }
}
