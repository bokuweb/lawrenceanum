//! 本文のテキスト（`textdump` の出力のディレクトリか、1 つのファイル）をModernBERT で見直す。
//!
//! ```bash
//! # 補正（採った書き直し）を TSV で出す: ファイル, 行, 種類, 改善(nat), 元の行, 書き直した行
//! cargo run --release -p kanpo-lm --example check -- ~/.cache/kanpo-lm/modernbert-ja-30m /tmp/after
//! # 怪しい行を上から並べる: ファイル, 行, 1 字あたりの負の対数尤度, いちばん読みにくい字, 行
//! cargo run --release -p kanpo-lm --example check -- --scan 100 ~/.cache/kanpo-lm/modernbert-ja-30m /tmp/after
//! ```

use kanpo_lm::mlm::MlmScorer;
use kanpo_lm::{correct, scan, LineScore, Options};
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
    let top = match args.iter().position(|a| a == "--scan") {
        Some(i) => {
            let n = args.get(i + 1).and_then(|n| n.parse::<usize>().ok()).unwrap_or(100);
            args.drain(i..(i + 2).min(args.len()));
            Some(n)
        }
        None => None,
    };
    let mut opts = Options::default();
    // --suspect X（見直す行の 1 字あたりの負の対数尤度の下限）・--gain Y（書き直しを採る改善の下限）
    let mut flag = |name: &str| -> anyhow::Result<Option<f32>> {
        let Some(i) = args.iter().position(|a| a == name) else { return Ok(None) };
        let v = args.get(i + 1).and_then(|v| v.parse().ok()).ok_or_else(|| anyhow::anyhow!("{name} の値"))?;
        args.drain(i..i + 2);
        Ok(Some(v))
    };
    if let Some(v) = flag("--suspect")? {
        opts.suspect_nll = v;
    }
    if let Some(v) = flag("--gain")? {
        opts.min_gain = v;
    }
    let [model, path] = args.as_slice() else {
        anyhow::bail!("usage: check [--scan N] [--suspect X] [--gain Y] <ModernBERT のディレクトリ> <textdump の出力のディレクトリかファイル>");
    };
    let mut scorer = MlmScorer::open(Path::new(model))?;
    let mut ranked: Vec<(String, LineScore, String)> = Vec::new();
    for file in files(Path::new(path))? {
        let text = std::fs::read_to_string(&file)?;
        let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
        let name = file.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        match top {
            Some(_) => {
                for s in scan(&mut scorer, &lines, &opts)? {
                    if s.chars >= opts.min_chars {
                        let line = lines[s.line].to_string();
                        ranked.push((name.clone(), s, line));
                    }
                }
            }
            None => {
                for c in correct(&mut scorer, &lines, &opts)?.1 {
                    println!("{name}\t{}\t{}\t{:.1}\t{}\t{}", c.line, c.rule, c.gain, c.before, c.after);
                }
            }
        }
    }
    if let Some(n) = top {
        ranked.sort_by(|a, b| b.1.per_char().total_cmp(&a.1.per_char()));
        for (name, s, line) in ranked.iter().take(n) {
            println!("{name}\t{}\t{:.2}\t{}\t{line}", s.line, s.per_char(), s.worst);
        }
    }
    Ok(())
}
