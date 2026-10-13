//! 罫線も使って記事ごとの構造化結果（Document）を JSON で出す（コーパスの確認用）。
fn main() {
    let path = std::env::args().nth(1).expect("usage: docjson <bbox.html>");
    let xhtml = std::fs::read_to_string(&path).unwrap();
    let rules = std::fs::read_to_string(path.replace(".bbox.html", ".rules.json"))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok());
    let a = kanpo_parser::analyze_with_rules(&xhtml, &[rules]);
    let docs: Vec<_> = a.articles.iter().map(|a| &a.document).collect();
    println!("{}", serde_json::to_string_pretty(&docs).unwrap());
}
