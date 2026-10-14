//! ページのコンテンツストリームを復号・展開して出す（調査用）。
//!
//! cargo run -p kanpo-pdf --example content -- page.pdf [page-number]

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let doc = kanpo_pdf::Document::load(std::fs::read(&args[0]).unwrap()).unwrap();
    let n: usize = args.get(1).map(|s| s.parse().unwrap()).unwrap_or(1);
    let page = &doc.pages()[n - 1];
    let contents = doc.dict_get(&page.dict, b"Contents").unwrap();
    let streams = match contents {
        kanpo_pdf::Object::Array(a) => a.iter().map(|x| doc.resolve(x)).collect(),
        o => vec![o],
    };
    for s in streams {
        let data = doc.decode_stream(s.as_stream().unwrap()).unwrap();
        print!("{}", String::from_utf8_lossy(&data));
    }
}
