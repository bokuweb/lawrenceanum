//! kanpo-pdf の結合テスト。
//!
//! - 手で組んだ小さな PDF（暗号化なし・xref 表）で、文字の位置と罫線を確かめる
//! - kanpo-parser のコーパスの実官報（AES-256 で暗号化、縦書きの CID フォント）で、
//!   poppler（`pdftotext -bbox-layout` / `pdftocairo -svg`）の出力（`tests/poppler/`）と突き合わせる

use std::path::{Path, PathBuf};

/// オブジェクトを並べて xref 表と trailer を付けた PDF を作る。
fn build_pdf(objects: &[String]) -> Vec<u8> {
    let mut out = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::new();
    for (i, body) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n{body}\nendobj\n", i + 1).as_bytes());
    }
    let xref = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes());
    for off in offsets {
        out.extend_from_slice(format!("{off:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            objects.len() + 1
        )
        .as_bytes(),
    );
    out
}

fn sample_pdf() -> Vec<u8> {
    let content = "BT /F1 10 Tf 1 0 0 1 100 700 Tm (AB) Tj ET\n0.5 w 50 600 m 50 500 l S\n50 450 m 300 450 l S\n10 10 100 100 re f\n";
    build_pdf(&[
        "<< /Type /Catalog /Pages 2 0 R >>".into(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".into(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 595 842] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>".into(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /FirstChar 65 /LastChar 66 /Widths [667 667] >>".into(),
        format!("<< /Length {} >>\nstream\n{content}endstream", content.len()),
    ])
}

#[test]
fn reads_text_positions_from_a_simple_pdf() {
    let pages = kanpo_pdf::extract_pages(&sample_pdf()).unwrap();
    assert_eq!(pages.len(), 1);
    let p = &pages[0];
    assert_eq!((p.width, p.height), (595.0, 842.0));
    let text: String = p.glyphs.iter().map(|g| g.text.as_str()).collect();
    assert_eq!(text, "AB");
    // ページ座標は左上原点・y 下向き。「A」の原点は (100, 842 - 700)、字幅は 0.667 em。
    let a = &p.glyphs[0];
    assert!((a.x - 100.0).abs() < 1e-6 && (a.y - 142.0).abs() < 1e-6);
    assert!((a.end_x - 106.67).abs() < 1e-6);
    assert!((p.glyphs[1].x - 106.67).abs() < 1e-6);
    assert!(!a.vertical);
}

#[test]
fn extracts_rules_but_not_rectangles() {
    let pages = kanpo_pdf::extract_pages(&sample_pdf()).unwrap();
    let rules = kanpo_pdf::page_rules(&pages[0]);
    assert_eq!(
        rules.vertical,
        vec![kanpo_pdf::VRule {
            x: 50.0,
            y0: 242.0,
            y1: 342.0
        }]
    );
    assert_eq!(
        rules.horizontal,
        vec![kanpo_pdf::HRule {
            y: 392.0,
            x0: 50.0,
            x1: 300.0
        }]
    );
}

#[test]
fn builds_bbox_layout_xhtml() {
    let pages = kanpo_pdf::extract_pages(&sample_pdf()).unwrap();
    let xhtml = kanpo_pdf::bbox::bbox_layout(&pages);
    assert!(xhtml.contains(r#"<page width="595.000000" height="842.000000">"#));
    assert!(xhtml.contains(">AB</word>"));
}

// ---- 実官報 ----

fn corpus() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../kanpo-parser/tests/corpus")
}

/// poppler の出力（`tests/poppler/<case>.<page>.bbox.html` など）。
fn poppler(pdf: &Path, ext: &str) -> String {
    let case = pdf.parent().unwrap().file_name().unwrap().to_string_lossy();
    let page = pdf.file_stem().unwrap().to_string_lossy();
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("tests/poppler/{case}.{page}.{ext}"));
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// コーパスの全ページ（`page.pdf`・`page-2.pdf` …）。
fn corpus_pages() -> Vec<PathBuf> {
    let mut out = Vec::new();
    for e in std::fs::read_dir(corpus()).unwrap() {
        let dir = e.unwrap().path();
        for stem in std::iter::once("page".to_string()).chain((2..).map(|n| format!("page-{n}"))) {
            let pdf = dir.join(format!("{stem}.pdf"));
            if !pdf.exists() {
                break;
            }
            out.push(pdf);
        }
    }
    out.sort();
    out
}

#[test]
fn decrypts_kanpo_pdf_and_maps_japan1_cids() {
    let pdf = std::fs::read(corpus().join("2026-10-05-honshi1801-p2/page.pdf")).unwrap();
    let page = &kanpo_pdf::extract_pages(&pdf).unwrap()[0];
    // ToUnicode の無い Adobe-Japan1 の縦書きフォント。
    let text: String = page.glyphs.iter().take(5).map(|g| g.text.as_str()).collect();
    assert_eq!(text, "法規的告示");
    assert!(page.glyphs[0].vertical);
    assert_eq!(page.fonts[page.glyphs[0].font].ordering.as_deref(), Some("Japan1"));
    // 縦書き用の約物は縦書き用の字形の符号になる（︑ U+FE11）。
    assert!(page.glyphs.iter().any(|g| g.text == "\u{FE11}"));
}

#[test]
fn rules_match_pdftocairo_on_corpus() {
    for pdf in corpus_pages() {
        let expected: kanpo_pdf::PageRules =
            serde_json::from_str(&poppler(&pdf, "rules.json")).unwrap();
        let page = &kanpo_pdf::extract_pages(&std::fs::read(&pdf).unwrap()).unwrap()[0];
        let actual = kanpo_pdf::page_rules(page);
        let close = |a: f32, b: f32| (a - b).abs() < 0.05;
        assert_eq!(actual.vertical.len(), expected.vertical.len(), "{}", pdf.display());
        assert_eq!(actual.horizontal.len(), expected.horizontal.len(), "{}", pdf.display());
        for (a, e) in actual.vertical.iter().zip(&expected.vertical) {
            assert!(
                close(a.x, e.x) && close(a.y0, e.y0) && close(a.y1, e.y1),
                "{}: {a:?} != {e:?}",
                pdf.display()
            );
        }
        for (a, e) in actual.horizontal.iter().zip(&expected.horizontal) {
            assert!(
                close(a.y, e.y) && close(a.x0, e.x0) && close(a.x1, e.x1),
                "{}: {a:?} != {e:?}",
                pdf.display()
            );
        }
    }
}

/// `pdftotext -bbox-layout` の `<word>`（文字列と枠）。
fn poppler_words(xhtml: &str) -> Vec<(String, [f64; 4])> {
    xhtml
        .split("<word ")
        .skip(1)
        .map(|part| {
            let (attrs, rest) = part.split_once('>').unwrap();
            let num = |k: &str| -> f64 {
                let at = attrs.find(&format!("{k}=\"")).unwrap() + k.len() + 2;
                attrs[at..].split('"').next().unwrap().parse().unwrap()
            };
            let text = rest.split("</word>").next().unwrap().replace("&amp;", "&");
            (text, [num("xMin"), num("yMin"), num("xMax"), num("yMax")])
        })
        .collect()
}

/// Helvetica の A・B・C（字幅 667・667・722）で `content` を描いた 1 ページの語。
fn words_of(content: &str) -> Vec<String> {
    let pdf = build_pdf(&[
        "<< /Type /Catalog /Pages 2 0 R >>".into(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".into(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 595 842] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>".into(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /FirstChar 65 /LastChar 67 /Widths [667 667 722] >>".into(),
        format!("<< /Length {} >>\nstream\n{content}endstream", content.len()),
    ]);
    let pages = kanpo_pdf::extract_pages(&pdf).unwrap();
    kanpo_pdf::bbox::words(&pages[0]).into_iter().map(|w| w.text).collect()
}

#[test]
fn letter_spaced_glyphs_make_one_word() {
    // 字間（Tc）だけで 0.3 em 空けた字は 1 語。
    assert_eq!(words_of("BT /F1 10 Tf 3 Tc 100 700 Td (ABC) Tj ET\n"), ["ABC"]);
    // 同じ 0.3 em でも TJ の数値で空けた字は別の語。
    assert_eq!(words_of("BT /F1 10 Tf 100 700 Td [(A) -300 (B)] TJ ET\n"), ["A", "B"]);
    // 字間が 0.55 em を越えれば Tc でも別の語。
    assert_eq!(words_of("BT /F1 10 Tf 6 Tc 100 700 Td (AB) Tj ET\n"), ["A", "B"]);
    // 最後の字だけ別の演算子で描かれていても、前の字の字間と同じ空きで続けば同じ語。
    assert_eq!(words_of("BT /F1 10 Tf 5 Tc 100 700 Td (AB) Tj 23.34 0 Td (C) Tj ET\n"), ["ABC"]);
}

#[test]
fn rule_between_glyphs_splits_words() {
    // A（100〜106.67）と B（107.67〜）の間の 0.1 em は、普通は同じ語。
    let text = "BT /F1 10 Tf 100 700 Td [(A) -100 (B)] TJ ET\n";
    assert_eq!(words_of(text), ["AB"]);
    // 間に縦の罫線（表のセルの境目）があれば切る。
    assert_eq!(words_of(&format!("{text}0.5 w 107.17 690 m 107.17 720 l S\n")), ["A", "B"]);
    // 字に重ならない高さの線では切らない。
    assert_eq!(words_of(&format!("{text}0.5 w 107.17 600 m 107.17 650 l S\n")), ["AB"]);
}

/// コーパス全体で、poppler と文字列・枠まで一致する語の割合。語の切れ目は poppler と完全には
/// 一致しない（poppler 自身が同じ字間でもつないだり切ったりする）ので、割合で見る。
#[test]
fn words_mostly_match_pdftotext_on_corpus() {
    let (mut total, mut same) = (0usize, 0usize);
    for pdf in corpus_pages() {
        let theirs = poppler_words(&poppler(&pdf, "bbox.html"));
        let page = &kanpo_pdf::extract_pages(&std::fs::read(&pdf).unwrap()).unwrap()[0];
        let mut ours: Vec<(String, [f64; 4])> = kanpo_pdf::bbox::words(page)
            .into_iter()
            .map(|w| (w.text, [w.x_min, w.y_min, w.x_max, w.y_max]))
            .collect();
        total += theirs.len();
        for (text, b) in &theirs {
            let hit = ours
                .iter()
                .position(|(t, o)| t == text && o.iter().zip(b).all(|(x, y)| (x - y).abs() < 0.02));
            if let Some(i) = hit {
                ours.swap_remove(i);
                same += 1;
            }
        }
    }
    let ratio = same as f64 / total as f64;
    assert!(ratio > 0.9, "{same}/{total} = {ratio:.3}");
}

#[test]
fn rotated_page_words_match_pdftotext() {
    // /Rotate 270 のページ（縦書きの本文が表示では横に流れる）。
    let pdf = corpus().join("2026-10-13-gougai224-p69/page.pdf");
    let theirs = poppler_words(&poppler(&pdf, "bbox.html"));
    let page = &kanpo_pdf::extract_pages(&std::fs::read(&pdf).unwrap()).unwrap()[0];
    assert_eq!(page.rotate, 270);
    let mut ours = kanpo_pdf::bbox::words(page);
    assert_eq!(ours.len(), theirs.len());
    // 語の並び順は poppler の段組み解析の順と違うので、文字列と枠で突き合わせる。
    for (text, b) in &theirs {
        let hit = ours.iter().position(|w| {
            &w.text == text
                && [w.x_min, w.y_min, w.x_max, w.y_max]
                    .iter()
                    .zip(b)
                    .all(|(x, y)| (x - y).abs() < 0.02)
        });
        assert!(hit.is_some(), "{text} {b:?} が無い");
        ours.swap_remove(hit.unwrap());
    }
}
