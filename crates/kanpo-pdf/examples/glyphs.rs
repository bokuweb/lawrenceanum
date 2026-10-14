//! ページの文字を 1 行ずつ出す（調査用）。
//!
//! cargo run -p kanpo-pdf --example glyphs -- page.pdf

fn main() {
    let path = std::env::args().nth(1).expect("pdf path");
    let pages = kanpo_pdf::extract_pages(&std::fs::read(path).unwrap()).unwrap();
    for (i, p) in pages.iter().enumerate() {
        println!("# page {} {}x{} rotate={} glyphs={} paths={}", i + 1, p.width, p.height, p.rotate, p.glyphs.len(), p.paths.len());
        for (k, f) in p.fonts.iter().enumerate() {
            println!("# font {k} {} vertical={} ordering={:?} to_unicode={}", f.name, f.vertical, f.ordering, f.has_to_unicode);
        }
        for g in &p.glyphs {
            println!(
                "{:?}\tcid={}\tf={}\tv={}\tsize={:.2}\torigin=({:.3},{:.3})\tend=({:.3},{:.3})\tbbox=[{:.3},{:.3},{:.3},{:.3}]\tsp={:.3}\tkern={:.3}\tshow={}",
                g.text, g.cid, g.font, g.vertical as u8, g.size, g.x, g.y, g.end_x, g.end_y, g.bbox[0], g.bbox[1], g.bbox[2], g.bbox[3], g.spacing, g.kern, g.show
            );
        }
    }
}
