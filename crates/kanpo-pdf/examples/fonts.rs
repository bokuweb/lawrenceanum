//! PDF のページごとのフォントを一覧する（調査用）。
//!
//! cargo run -p kanpo-pdf --example fonts -- page.pdf

fn main() {
    for path in std::env::args().skip(1) {
        let doc = kanpo_pdf::Document::load(std::fs::read(&path).unwrap()).unwrap();
        for (i, page) in doc.pages().iter().enumerate() {
            println!("{path} p{} rotate={} media={:?}", i + 1, page.rotate, page.media_box);
            let Some(fonts) = doc.dict_get(&page.resources, b"Font") else {
                continue;
            };
            for (name, f) in fonts.as_dict().unwrap() {
                let f = doc.resolve(f);
                let d = f.as_dict().unwrap();
                let s = |k: &[u8]| {
                    doc.dict_get(d, k)
                        .and_then(|o| o.as_name().map(|n| String::from_utf8_lossy(n).into_owned()))
                };
                let desc = doc
                    .dict_get(d, b"DescendantFonts")
                    .and_then(|a| a.as_array().and_then(|a| a.first().map(|x| doc.resolve(x))));
                let csi = desc
                    .as_ref()
                    .and_then(|df| doc.dict_get(df.as_dict().unwrap(), b"CIDSystemInfo"));
                let ordering = csi
                    .as_ref()
                    .and_then(|c| doc.dict_get(c.as_dict().unwrap(), b"Ordering"))
                    .and_then(|o| o.as_str().map(|s| String::from_utf8_lossy(s).into_owned()));
                println!(
                    "  {} {:?} {:?} enc={:?} tounicode={} ordering={:?}",
                    String::from_utf8_lossy(name),
                    s(b"Subtype"),
                    s(b"BaseFont"),
                    s(b"Encoding"),
                    d.contains_key(&b"ToUnicode"[..]),
                    ordering
                );
            }
        }
    }
}
