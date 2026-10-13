//! 抽出結果を目視確認するためのデバッグ用ダンプ。
//!
//! `pdftotext -bbox-layout` の XHTML を渡すと、縦書き復元後の本文を行番号付きで出力する。
//! 同じ名前の `.rules.json`（examples/prepare が作る）があれば罫線も使う。
//!
//! ```text
//! pdftotext -bbox-layout -enc UTF-8 input.pdf out.html
//! cargo run -p kanpo-parser --example dump -- out.html
//! ```

use std::process::ExitCode;

fn main() -> ExitCode {
    let Some(path) = std::env::args().nth(1) else {
        eprintln!("usage: cargo run -p kanpo-parser --example dump -- <bbox.html>");
        return ExitCode::from(2);
    };
    let xhtml = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("read {path}: {e}");
            return ExitCode::from(1);
        }
    };
    let rules: Option<kanpo_parser::lines::PageRules> = std::fs::read_to_string(path.replace(".bbox.html", ".rules.json"))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok());
    let out = kanpo_parser::analyze_with_rules(&xhtml, &[rules]).text;
    for (i, line) in out.lines().enumerate() {
        println!("{i:3} {line}");
    }
    eprintln!("--- format: {:?}", kanpo_parser::detect_format_of(&out));
    ExitCode::SUCCESS
}
