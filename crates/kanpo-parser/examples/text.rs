//! 罫線（同じ名前の `.rules.json` があれば）も使って本文を出す（コーパスの期待出力の確認用）。
fn main() {
    let path = std::env::args().nth(1).expect("usage: text <bbox.html>");
    let xhtml = std::fs::read_to_string(&path).unwrap();
    let rules = std::fs::read_to_string(path.replace(".bbox.html", ".rules.json"))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok());
    println!("{}", kanpo_parser::analyze_with_rules(&xhtml, &[rules]).text);
}
