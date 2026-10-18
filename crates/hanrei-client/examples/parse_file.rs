//! 保存済み HTML のパース確認用: `cargo run -p hanrei-client --example parse_file -- list|detail FILE URL`
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let html = std::fs::read_to_string(&a[2]).unwrap();
    if a[1] == "list" {
        let items = hanrei_client::parse_list(&html, &a[3]);
        println!("total={:?} rows={}", hanrei_client::parse_total(&html), items.len());
        println!("{:#?}", &items[..items.len().min(2)]);
    } else {
        println!("{:#?}", hanrei_client::parse_detail(&html, &a[3]).unwrap());
    }
}
