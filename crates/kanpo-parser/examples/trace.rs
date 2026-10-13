//! 復元過程（領域・行と、その行を構成する語の座標、罫線の表）を表示するデバッグ用ダンプ。
//! 同じ名前の `.rules.json`（examples/prepare が作る）があれば罫線も使う。
//!
//! ```text
//! cargo run -p kanpo-parser --example trace -- page.bbox.html
//! ```

fn main() {
    let path = std::env::args().nth(1).expect("usage: trace <bbox.html>");
    let xhtml = std::fs::read_to_string(&path).unwrap();
    let rules_path = path.replace(".bbox.html", ".rules.json");
    let rules: Option<kanpo_parser::lines::PageRules> =
        std::fs::read_to_string(&rules_path).ok().and_then(|s| serde_json::from_str(&s).ok());
    let a = kanpo_parser::analyze_with_rules(&xhtml, &[rules]);
    for (pi, page) in a.pages.iter().enumerate() {
        println!("=== page {pi} layout={}", page.layout);
        for r in &page.regions {
            println!("--- {:?} unit={} part={}", r.role, r.unit, r.part);
            for l in &r.lines {
                if let Some(t) = &l.table {
                    println!("  [table {}x{} at x={:.0}-{:.0} y={:.0}-{:.0}]", t.rows, t.cols, t.x0, t.x1, t.y0, t.y1);
                    for c in &t.cells {
                        println!("    r{} c{} span {}x{}: {}", c.row, c.col, c.rowspan, c.colspan, c.text);
                    }
                    continue;
                }
                let w = &page.words[l.words[0]];
                println!("  [x={:.0} y={:.0}] {}", w.x0, w.y0, l.text);
            }
        }
    }
}
