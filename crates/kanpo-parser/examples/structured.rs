//! bbox xhtml → 構造化 Document を JSON で出力するデモ。
fn main() {
    let path = std::env::args().nth(1).expect("usage: structured <bbox.html>");
    let xhtml = std::fs::read_to_string(path).unwrap();
    let text = kanpo_parser::reconstruct_vertical(&xhtml);
    let doc = kanpo_parser::Document::from_text(&text);
    println!("{}", serde_json::to_string_pretty(&doc).unwrap());
}
