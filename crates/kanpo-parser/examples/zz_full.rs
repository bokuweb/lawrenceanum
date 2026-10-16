use kanpo_parser::model::{Block, Run};
fn t(r: &[Run]) -> String { r.iter().map(|x| x.text.as_str()).collect::<String>().replace('\n', "⏎") }
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let pages: Vec<kanpo_parser::PageInput> = args.iter().map(|p| { let pdf = std::fs::read(p).unwrap(); kanpo_parser::PageInput { xhtml: kanpo_parser::bbox_xhtml(&pdf).unwrap(), rules: kanpo_parser::page_rules(&pdf).ok() } }).collect();
    let a = kanpo_parser::analyze_item(&pages);
    for art in &a.articles { for b in &art.document.blocks { if let Block::Shinkyu { rows } = b { for (k, r) in rows.iter().enumerate() { println!("R{k}\n  後 {}\n  前 {}", t(&r.after), t(&r.before)); } } } }
}
