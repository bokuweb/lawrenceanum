//! 蓄積した官報（`lawpub kanpo-archive` の出力）の全ページを本番と同じく座標付きテキストと罫線から
//! 解析し、ページごとの本文を `<out_dir>/<ページ>.txt` に書く（パーサを直す前後の出力を比べる用）。
//!
//! ```text
//! cargo run --release -p kanpo-parser --example textdump -- <archive_dir> <out_dir>
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
    let mut args = std::env::args().skip(1);
    let (root, out) = match (args.next(), args.next()) {
        (Some(r), Some(o)) => (PathBuf::from(r), PathBuf::from(o)),
        _ => panic!("usage: textdump <archive_dir> <out_dir>"),
    };
    std::fs::create_dir_all(&out).unwrap();
    let mut files = Vec::new();
    walk(&root, &mut files);
    files.sort();
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get());
    let chunk = files.len().div_ceil(threads).max(1);
    std::thread::scope(|s| {
        for part in files.chunks(chunk) {
            let out = &out;
            s.spawn(move || {
                for f in part {
                    let xhtml = std::fs::read_to_string(f).unwrap_or_default();
                    let rules = std::fs::read_to_string(f.to_string_lossy().replace(".bbox.html", ".rules.json"))
                        .ok()
                        .and_then(|s| serde_json::from_str(&s).ok());
                    let text = kanpo_parser::analyze_with_rules(&xhtml, &[rules]).text;
                    let name = f.file_name().unwrap().to_string_lossy().replace(".bbox.html", ".txt");
                    std::fs::write(out.join(name), text).unwrap();
                }
            });
        }
    });
    eprintln!("{} pages", files.len());
}
