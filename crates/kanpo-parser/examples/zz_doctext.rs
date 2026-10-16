fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let pages: Vec<kanpo_parser::PageInput> = args.iter().map(|p| { let pdf = std::fs::read(p).unwrap(); kanpo_parser::PageInput { xhtml: kanpo_parser::bbox_xhtml(&pdf).unwrap(), rules: kanpo_parser::page_rules(&pdf).ok() } }).collect();
    let a = kanpo_parser::analyze_item(&pages);
    for (i, p) in a.pages.iter().enumerate() { println!("===== page {i} layout={}", p.layout); println!("{}", p.text); }
}
