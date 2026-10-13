//! 復元過程（領域・行と、その行を構成する語の座標）を表示するデバッグ用ダンプ。
//!
//! ```text
//! cargo run -p kanpo-parser --example trace -- out.bbox.html
//! ```

fn main() {
    let path = std::env::args().nth(1).expect("usage: trace <bbox.html>");
    let xhtml = std::fs::read_to_string(path).unwrap();
    let a = kanpo_parser::analyze(&xhtml);
    for (pi, page) in a.pages.iter().enumerate() {
        println!("=== page {pi} layout={}", page.layout);
        for r in &page.regions {
            println!("--- {:?} unit={} part={}", r.role, r.unit, r.part);
            for l in &r.lines {
                let w = &page.words[l.words[0]];
                println!("  [x={:.0} y={:.0}] {}", w.x0, w.y0, l.text);
            }
        }
    }
}
