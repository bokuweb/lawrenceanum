use kanpo_parser::model::{Block, Run};
fn t(r: &[Run]) -> String { r.iter().map(|x| if x.table.is_some() { "[TABLE]".to_string() } else { x.text.clone() }).collect::<String>().replace('\n', "⏎") }
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let pages: Vec<kanpo_parser::PageInput> = args.iter().map(|p| { let pdf = std::fs::read(p).unwrap(); kanpo_parser::PageInput { xhtml: kanpo_parser::bbox_xhtml(&pdf).unwrap(), rules: kanpo_parser::page_rules(&pdf).ok() } }).collect();
    let a = kanpo_parser::analyze_item(&pages);
    for (i, art) in a.articles.iter().enumerate() { println!("## {i}"); for b in &art.document.blocks { match b { Block::Shinkyu { rows } => { println!("[table]"); for (k, r) in rows.iter().enumerate() { println!("R{k}\n  後 {}\n  前 {}", t(&r.after).chars().take(60).collect::<String>(), t(&r.before).chars().take(60).collect::<String>()); } } Block::Paragraph { runs } => println!("P {}", t(runs).chars().take(50).collect::<String>()) } } }
}
