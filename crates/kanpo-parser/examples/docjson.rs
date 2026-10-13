//! 罫線も使って記事ごとの構造化結果（Document）を JSON で出す（コーパスの確認用）。
//! 複数の `.bbox.html` を渡すと 1 つの項目としてまとめて解析する。
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
    let docs: Vec<_> = a.articles.iter().map(|a| &a.document).collect();
    println!("{}", serde_json::to_string_pretty(&docs).unwrap());
}
