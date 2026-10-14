//! 官報 PDF を poppler（`pdftotext -bbox-layout`）と kanpo-pdf の両方で読み、パーサの出力と
//! 字の過不足を突き合わせる（kanpo-pdf の精度・退行の調査用。手元に poppler が要る）。
//!
//! ```text
//! cargo run --release -p kanpo-parser --example poppler_diff -- <PDF のあるディレクトリ>... > diff.tsv
//! ```
//!
//! 1 ページ 1 行で `path / text_same（本文が同じか）/ missing（poppler にだけある字）/ extra
//! （kanpo-pdf にだけある字）/ 例 / sorted_text_same（語を座標順に並べ直すと本文が同じか）` を出す。
//! text_same が false で sorted_text_same が true なら、違いは語の並び順（パーサの順序依存）による。
//! 最後の 2 列は poppler の語のうち文字列・枠まで一致する数と、poppler の語の数。
//! 罫線は両方とも kanpo-pdf のもの（pdftocairo と一致する）。

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

fn pdfs(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            pdfs(&p, out);
        } else if p.extension().is_some_and(|x| x == "pdf") {
            out.push(p);
        }
    }
}

/// `<word>` の字（空白を除く）を数える。
fn chars(xhtml: &str) -> BTreeMap<char, i64> {
    let mut m = BTreeMap::new();
    for part in xhtml.split("<word ").skip(1) {
        let text = part.split_once('>').map(|(_, r)| r.split("</word>").next().unwrap_or("")).unwrap_or("");
        let text = text.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&apos;", "'").replace("&amp;", "&");
        for c in text.chars().filter(|c| !c.is_whitespace()) {
            *m.entry(c).or_insert(0) += 1;
        }
    }
    m
}

/// 語を座標順（x 降順・y 昇順）に並べ直した XHTML（パーサの語の入力順への依存を除いて比べるため）。
fn sorted(xhtml: &str) -> String {
    let Some(start) = xhtml.find("<flow>") else { return xhtml.to_string() };
    let end = xhtml.rfind("</page>").unwrap_or(xhtml.len());
    let mut words: Vec<(f64, f64, f64, f64, String)> = xhtml[start..end]
        .split("<word ")
        .skip(1)
        .filter_map(|part| {
            let (attrs, rest) = part.split_once('>')?;
            let num = |k: &str| -> Option<f64> {
                let at = attrs.find(&format!("{k}=\""))? + k.len() + 2;
                attrs[at..].split('"').next()?.parse().ok()
            };
            Some((num("xMin")?, num("yMin")?, num("xMax")?, num("yMax")?, format!("<word {attrs}>{}</word>", rest.split("</word>").next()?)))
        })
        .collect();
    words.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.total_cmp(&b.1)).then(a.2.total_cmp(&b.2)).then(a.3.total_cmp(&b.3)).then(a.4.cmp(&b.4)));
    let body: String = words.iter().map(|w| format!("<flow><block><line>{}</line></block></flow>\n", w.4)).collect();
    format!("{}{body}{}", &xhtml[..start], &xhtml[end..])
}

/// `<word>` の文字列と枠。
fn words(xhtml: &str) -> Vec<(String, [f64; 4])> {
    xhtml
        .split("<word ")
        .skip(1)
        .filter_map(|part| {
            let (attrs, rest) = part.split_once('>')?;
            let num = |k: &str| -> Option<f64> {
                let at = attrs.find(&format!("{k}=\""))? + k.len() + 2;
                attrs[at..].split('"').next()?.parse().ok()
            };
            Some((rest.split("</word>").next()?.to_string(), [num("xMin")?, num("yMin")?, num("xMax")?, num("yMax")?]))
        })
        .collect()
}

/// poppler の語のうち、文字列と枠（0.02 pt 以内）まで一致する kanpo-pdf の語がある数。
fn exact_words(theirs: &str, ours: &str) -> (usize, usize) {
    let mut by_text: std::collections::HashMap<String, Vec<[f64; 4]>> = std::collections::HashMap::new();
    for (t, b) in words(ours) {
        by_text.entry(t).or_default().push(b);
    }
    let theirs = words(theirs);
    let mut same = 0;
    for (t, b) in &theirs {
        if let Some(v) = by_text.get_mut(t) {
            if let Some(i) = v.iter().position(|o| o.iter().zip(b).all(|(x, y)| (x - y).abs() < 0.02)) {
                v.swap_remove(i);
                same += 1;
            }
        }
    }
    (same, theirs.len())
}

fn row(path: &Path) -> String {
    let pdf = std::fs::read(path).unwrap();
    let ours = match std::panic::catch_unwind(|| kanpo_parser::page_inputs(&pdf)) {
        Ok(Ok(p)) => p,
        Ok(Err(e)) => return format!("{}\tERROR\t\t\t{e}", path.display()),
        Err(_) => return format!("{}\tPANIC\t\t\t", path.display()),
    };
    // poppler の出力は PDF の隣に `.poppler.html` として置いておき、次からはそれを読む。
    let cache = path.with_extension("poppler.html");
    let theirs = std::fs::read_to_string(&cache).unwrap_or_else(|_| {
        let out = Command::new("pdftotext").args(["-bbox-layout", "-enc", "UTF-8"]).arg(path).arg("-").output().unwrap();
        let s = String::from_utf8_lossy(&out.stdout).into_owned();
        let _ = std::fs::write(&cache, &s);
        s
    });
    // 本文: そのまま（kanpo-pdf / poppler）と、語を座標順に並べ直したもの（kanpo-pdf / poppler）。
    // 官報の PDF は 1 ページずつなので、1 ページ目の罫線を使う。
    let rules = ours.first().and_then(|p| p.rules.clone());
    let ours_xhtml = ours.first().map(|p| p.xhtml.clone()).unwrap_or_default();
    let texts: Vec<String> = [ours_xhtml.clone(), theirs.clone(), sorted(&ours_xhtml), sorted(&theirs)]
        .into_iter()
        .map(|xhtml| kanpo_parser::analyze_item(&[kanpo_parser::PageInput { xhtml, rules: rules.clone() }]).text)
        .collect();
    let (a, b) = (chars(&theirs), chars(&ours.iter().map(|p| p.xhtml.as_str()).collect::<String>()));
    let mut missing = 0;
    let mut extra = 0;
    let mut examples = Vec::new();
    for c in a.keys().chain(b.keys()).collect::<std::collections::BTreeSet<_>>() {
        let d = b.get(c).copied().unwrap_or(0) - a.get(c).copied().unwrap_or(0);
        if d < 0 {
            missing -= d;
            examples.push(format!("-{c}{}({:04X})", -d, *c as u32));
        } else if d > 0 {
            extra += d;
            examples.push(format!("+{c}{d}({:04X})", *c as u32));
        }
    }
    let (same, total) = exact_words(&theirs, &ours_xhtml);
    if let Ok(dir) = std::env::var("DIFF_DUMP") {
        if texts[2] != texts[3] {
            let name = path.file_stem().unwrap().to_string_lossy();
            let _ = std::fs::write(Path::new(&dir).join(format!("{name}.poppler.txt")), &texts[3]);
            let _ = std::fs::write(Path::new(&dir).join(format!("{name}.ours.txt")), &texts[2]);
        }
    }
    format!("{}\t{}\t{missing}\t{extra}\t{}\t{}\t{same}\t{total}", path.display(), texts[0] == texts[1], examples.join(" "), texts[2] == texts[3])
}

fn main() {
    let mut files = Vec::new();
    for d in std::env::args().skip(1) {
        pdfs(Path::new(&d), &mut files);
    }
    files.sort();
    let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
    let chunks: Vec<&[PathBuf]> = files.chunks(files.len().div_ceil(threads).max(1)).collect();
    let rows: Vec<String> = std::thread::scope(|s| {
        let hs: Vec<_> = chunks.iter().map(|c| s.spawn(move || c.iter().map(|p| row(p)).collect::<Vec<_>>())).collect();
        hs.into_iter().flat_map(|h| h.join().unwrap()).collect()
    });
    println!("path\ttext_same\tmissing\textra\texamples\tsorted_text_same\twords_exact\twords");
    for r in rows {
        println!("{r}");
    }
}
