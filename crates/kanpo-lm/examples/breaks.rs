//! 本文のテキスト（`textdump` の出力のディレクトリか、1 つのファイル）の各行の切れ目の確率を出す（`train/` で fine-tune した
//! 分類器）。TSV: ファイル, 行, いちばん高い切れ目の確率, 切れ目（確率 0.5 以上）の前に「|」を入れた行。
//!
//! ```bash
//! cargo run --release -p kanpo-lm --example breaks -- ~/.cache/kanpo-lm/breaks-30m /tmp/after > breaks.tsv
//! # 確率の高い行から 100 行
//! cargo run --release -p kanpo-lm --example breaks -- --top 100 ~/.cache/kanpo-lm/breaks-30m /tmp/after
//! ```

use kanpo_lm::tagger::{find_breaks, BreakTagger};
use std::path::{Path, PathBuf};

fn files(path: &Path) -> anyhow::Result<Vec<PathBuf>> {
    if path.is_file() {
        return Ok(vec![path.to_path_buf()]);
    }
    let mut out: Vec<PathBuf> = std::fs::read_dir(path)?.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.is_file()).collect();
    out.sort();
    Ok(out)
}

fn main() -> anyhow::Result<()> {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let top = args.iter().position(|a| a == "--top").map(|i| {
        let n = args.get(i + 1).and_then(|n| n.parse::<usize>().ok()).unwrap_or(100);
        args.drain(i..(i + 2).min(args.len()));
        n
    });
    let [model, path] = args.as_slice() else {
        anyhow::bail!("usage: breaks [--top N] <分類器のディレクトリ> <textdump の出力のディレクトリかファイル>");
    };
    let tagger = BreakTagger::open(Path::new(model))?;
    let files = files(Path::new(path))?;
    // ページごとに並べて流す。
    let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1);
    let results: Vec<Vec<String>> = std::thread::scope(|s| {
        let handles: Vec<_> = files
            .chunks(files.len().div_ceil(threads).max(1))
            .map(|group| {
                let tagger = tagger.clone();
                s.spawn(move || -> anyhow::Result<Vec<(f32, String)>> {
                    let mut rows = Vec::new();
                    for file in group {
                        let text = std::fs::read_to_string(file)?;
                        let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
                        let name = file.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                        for b in find_breaks(&tagger, &lines, 360, 0.5)? {
                            let line = lines[b.line];
                            let mut marked = String::new();
                            for (i, c) in line.char_indices() {
                                if b.breaks.iter().any(|&(p, _)| p == i) {
                                    marked.push('|');
                                }
                                marked.push(c);
                            }
                            rows.push((b.max_prob, format!("{name}\t{}\t{:.3}\t{marked}", b.line, b.max_prob)));
                        }
                    }
                    Ok(rows)
                })
            })
            .collect();
        let mut all: Vec<(f32, String)> = Vec::new();
        for h in handles {
            all.extend(h.join().expect("thread")?);
        }
        if top.is_some() {
            all.sort_by(|a, b| b.0.total_cmp(&a.0));
        }
        Ok::<_, anyhow::Error>(vec![all.into_iter().map(|r| r.1).collect()])
    })?;
    for row in results.iter().flatten().take(top.unwrap_or(usize::MAX)) {
        println!("{row}");
    }
    Ok(())
}
