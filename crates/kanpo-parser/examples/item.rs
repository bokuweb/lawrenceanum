//! 複数ページの項目をまとめて解析し、記事ごとの構造（段落 P・新旧対照表 S{行数}）と本文を出す。
//! 各ページの `.bbox.html` と同じ名前の `.rules.json` があれば罫線も使う。
//!
//! ```text
//! cargo run -p kanpo-parser --example item -- p1.bbox.html p2.bbox.html ...
//! ```

fn main() {
    let pages: Vec<kanpo_parser::PageInput> = std::env::args()
        .skip(1)
        .map(|path| kanpo_parser::PageInput {
            xhtml: std::fs::read_to_string(&path).unwrap(),
            rules: std::fs::read_to_string(path.replace(".bbox.html", ".rules.json"))
                .ok()
                .and_then(|s| serde_json::from_str(&s).ok()),
        })
        .collect();
    let a = kanpo_parser::analyze_item(&pages);
    for (i, art) in a.articles.iter().enumerate() {
        let shape: Vec<String> = art
            .document
            .blocks
            .iter()
            .map(|b| match b {
                kanpo_parser::Block::Paragraph { .. } => "P".to_string(),
                kanpo_parser::Block::Shinkyu { rows, .. } => format!("S{}", rows.len()),
            })
            .collect();
        println!("=== article {i}: {}", shape.join(" "));
        println!("{}", art.text);
    }
}
