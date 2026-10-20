//! 議案キャッシュ (`.cache/gian/{session}/*.json`) 全体で改め文の解析率を集計する。
//!
//! ```sh
//! cargo run -p gian-client --example amendment_report -- .cache/gian [--unparsed]
//! ```

use gian_client::{parse_bill_amendments, Bill};
use std::path::Path;

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let root = Path::new(args.get(1).map(String::as_str).unwrap_or(".cache/gian"));
    let show_unparsed = args.iter().any(|a| a == "--unparsed");
    let (mut bills, mut with_text, mut parsed) = (0usize, 0usize, 0usize);
    let (mut targets, mut sentences, mut ops, mut unparsed, mut clean) = (0, 0, 0, 0, 0);
    for session in std::fs::read_dir(root)? {
        let session = session?.path();
        if !session.is_dir() {
            continue;
        }
        for entry in std::fs::read_dir(&session)? {
            let path = entry?.path();
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if name.starts_with("._") || path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let Ok(bill) = serde_json::from_slice::<Bill>(&std::fs::read(&path)?) else {
                continue;
            };
            bills += 1;
            if bill.documents.iter().any(|d| d.kind == "bill_text") {
                with_text += 1;
            }
            let Some(a) = parse_bill_amendments(&bill) else {
                continue;
            };
            parsed += 1;
            targets += a.stats.targets;
            sentences += a.stats.sentences;
            ops += a.stats.ops;
            unparsed += a.stats.unparsed_clauses;
            for t in &a.targets {
                for s in &t.sentences {
                    if s.unparsed.is_empty() && !s.ops.is_empty() {
                        clean += 1;
                    }
                    if show_unparsed {
                        for u in &s.unparsed {
                            let short: String = u.chars().take(120).collect();
                            println!(
                                "UNPARSED {}/{} {}: {short}",
                                a.session, a.bill_id, t.law_title
                            );
                        }
                        if s.ops.is_empty() && s.unparsed.is_empty() {
                            let short: String = s.text.chars().take(120).collect();
                            println!("EMPTY {}/{}: {short}", a.session, a.bill_id);
                        }
                    }
                }
            }
        }
    }
    println!(
        "bills={bills} with_text={with_text} with_amendments={parsed} targets={targets} \
         sentences={sentences} clean_sentences={clean} ({:.1}%) ops={ops} unparsed_clauses={unparsed}",
        100.0 * clean as f64 / sentences.max(1) as f64
    );
    Ok(())
}
