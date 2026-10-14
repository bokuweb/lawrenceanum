//! 自前の語（`bbox::words`）と `pdftotext -bbox-layout` の語を突き合わせる（検証用）。
//!
//! cargo run -p kanpo-pdf --example compare_bbox -- page.pdf page.bbox.html [-v]

#[derive(Debug, Clone)]
struct W {
    text: String,
    b: [f64; 4],
}

fn poppler_words(xhtml: &str) -> Vec<Vec<W>> {
    let mut pages = Vec::new();
    for page in xhtml.split("<page ").skip(1) {
        let mut words = Vec::new();
        for part in page.split("<word ").skip(1) {
            let (attrs, rest) = part.split_once('>').unwrap();
            let text = rest.split("</word>").next().unwrap();
            let num = |k: &str| -> f64 {
                let at = attrs.find(&format!("{k}=\"")).unwrap() + k.len() + 2;
                attrs[at..].split('"').next().unwrap().parse().unwrap()
            };
            let text = text
                .replace("&lt;", "<")
                .replace("&gt;", ">")
                .replace("&quot;", "\"")
                .replace("&apos;", "'")
                .replace("&amp;", "&");
            words.push(W {
                text,
                b: [num("xMin"), num("yMin"), num("xMax"), num("yMax")],
            });
        }
        pages.push(words);
    }
    pages
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let verbose = args.iter().any(|a| a == "-v");
    let pages = kanpo_pdf::extract_pages(&std::fs::read(&args[0]).unwrap()).unwrap();
    let theirs = poppler_words(&std::fs::read_to_string(&args[1]).unwrap());
    let (mut total, mut exact, mut text_diff, mut missing, mut extra) = (0, 0, 0, 0, 0);
    for (page, pw) in pages.iter().zip(&theirs) {
        let ours: Vec<W> = kanpo_pdf::bbox::words(page)
            .into_iter()
            .map(|w| W {
                text: w.text,
                b: [w.x_min, w.y_min, w.x_max, w.y_max],
            })
            .collect();
        let near = |a: &[f64; 4], b: &[f64; 4]| a.iter().zip(b).all(|(x, y)| (x - y).abs() < 0.02);
        let mut used = vec![false; ours.len()];
        for w in pw {
            total += 1;
            let hit = (0..ours.len())
                .find(|&i| !used[i] && ours[i].text == w.text && near(&ours[i].b, &w.b))
                .or_else(|| (0..ours.len()).find(|&i| !used[i] && near(&ours[i].b, &w.b)));
            match hit {
                Some(i) => {
                    used[i] = true;
                    if ours[i].text == w.text {
                        exact += 1;
                    } else {
                        text_diff += 1;
                        if verbose {
                            println!("TEXT  poppler={:?} ours={:?} {:?}", w.text, ours[i].text, w.b);
                        }
                    }
                }
                None => {
                    missing += 1;
                    if verbose {
                        println!("MISS  poppler={:?} {:?}", w.text, w.b);
                    }
                }
            }
        }
        for (i, w) in ours.iter().enumerate() {
            if !used[i] {
                extra += 1;
                if verbose {
                    println!("EXTRA ours={:?} {:?}", w.text, w.b);
                }
            }
        }
    }
    println!(
        "{}\twords={total}\texact={exact}\ttext_diff={text_diff}\tmissing={missing}\textra={extra}",
        args[0]
    );
}
