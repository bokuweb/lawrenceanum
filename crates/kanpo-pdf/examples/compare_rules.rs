//! 自前の罫線と `page.rules.json`（`pdftocairo -svg` から作ったもの）を突き合わせる（検証用）。
//!
//! cargo run -p kanpo-pdf --example compare_rules -- page.pdf page.rules.json [-v]

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let verbose = args.iter().any(|a| a == "-v");
    let pages = kanpo_pdf::extract_pages(&std::fs::read(&args[0]).unwrap()).unwrap();
    let ours = kanpo_pdf::page_rules(&pages[0]);
    let theirs: kanpo_pdf::PageRules = serde_json::from_str(&std::fs::read_to_string(&args[1]).unwrap()).unwrap();
    let key_v = |r: &kanpo_pdf::VRule| {
        (
            (r.x * 10.0).round() as i64,
            (r.y0 * 10.0).round() as i64,
            (r.y1 * 10.0).round() as i64,
        )
    };
    let key_h = |r: &kanpo_pdf::HRule| {
        (
            (r.y * 10.0).round() as i64,
            (r.x0 * 10.0).round() as i64,
            (r.x1 * 10.0).round() as i64,
        )
    };
    let diff = |a: Vec<(i64, i64, i64)>, b: Vec<(i64, i64, i64)>, label: &str| -> (usize, usize) {
        let mut b2 = b.clone();
        let mut missing = 0;
        for k in &a {
            if let Some(i) = b2.iter().position(|x| x == k) {
                b2.remove(i);
            } else {
                missing += 1;
                if verbose {
                    println!("MISS {label} {k:?}");
                }
            }
        }
        if verbose {
            for k in &b2 {
                println!("EXTRA {label} {k:?}");
            }
        }
        (missing, b2.len())
    };
    let (mv, ev) = diff(
        theirs.vertical.iter().map(key_v).collect(),
        ours.vertical.iter().map(key_v).collect(),
        "v",
    );
    let (mh, eh) = diff(
        theirs.horizontal.iter().map(key_h).collect(),
        ours.horizontal.iter().map(key_h).collect(),
        "h",
    );
    println!(
        "{}\tv={}/{}\th={}/{}\tmissing={}\textra={}",
        args[0],
        ours.vertical.len(),
        theirs.vertical.len(),
        ours.horizontal.len(),
        theirs.horizontal.len(),
        mv + mh,
        ev + eh
    );
}
