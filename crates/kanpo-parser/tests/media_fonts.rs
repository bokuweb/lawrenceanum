use kanpo_parser::media::{self, Field};
use std::collections::BTreeSet;
#[test]
fn original_sparse_tables_keep_their_effective_eight_point_source_glyph_sizes() {
    let m = media::analyze_pdf(include_bytes!("data/workbook/case-04.pdf")).unwrap();
    let sources =
        kanpo_pdf::extract_pages_with_aj1_sources(include_bytes!("data/workbook/case-04.pdf"))
            .unwrap();
    for field in [Field::Paragraph, Field::After, Field::Before] {
        let text = m
            .documents
            .iter()
            .map(|d| d.field_text(field))
            .collect::<Vec<_>>();
        let mut claims = BTreeSet::new();
        for b in m.fonts.bindings.iter().filter(|b| b.field == field) {
            assert!(b
                .glyphs
                .iter()
                .all(|id| sources[b.page].page().glyphs[*id].size == b.points));
            assert_eq!(
                text[b.document]
                    .chars()
                    .skip(b.offset)
                    .take(b.length)
                    .collect::<String>(),
                b.text
            );
            for i in b.offset..b.offset + b.length {
                assert!(claims.insert((b.document, i)));
            }
        }
        let expected = text
            .iter()
            .enumerate()
            .flat_map(|(d, t)| (0..t.chars().count()).map(move |i| (d, i)))
            .collect::<BTreeSet<_>>();
        assert_eq!(
            claims, expected,
            "complete original field is covered without duplicate or guessed font positions"
        );
    }
    assert!(m.fonts.unresolved.is_empty());
    assert!(m
        .fonts
        .bindings
        .iter()
        .any(|b| b.field == Field::After && b.points == 8. && b.text.chars().count() > 10));
}

#[test]
fn every_workbook_font_binding_has_its_own_original_glyph_size_and_scalar_claim() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/workbook");
    let files = (1..=18)
        .map(|i| format!("case-{i:02}.pdf"))
        .chain((1..=3).map(|i| format!("case-10-listed-{i}.pdf")));
    for name in files {
        let bytes = std::fs::read(root.join(&name)).unwrap();
        let m = media::analyze_pdf(&bytes).unwrap();
        let sources = kanpo_pdf::extract_pages_with_aj1_sources(&bytes).unwrap();
        let mut claims = BTreeSet::new();
        for b in &m.fonts.bindings {
            assert!(!b.glyphs.is_empty());
            assert!(
                b.glyphs
                    .iter()
                    .all(|id| sources[b.page].page().glyphs[*id].size == b.points),
                "{name}"
            );
            let text = m.documents[b.document].field_text(b.field);
            assert_eq!(
                text.chars()
                    .skip(b.offset)
                    .take(b.length)
                    .collect::<String>(),
                b.text
            );
            let field = match b.field {
                Field::Paragraph => 0,
                Field::After => 1,
                Field::Before => 2,
            };
            for i in b.offset..b.offset + b.length {
                assert!(
                    claims.insert((b.document, field, i)),
                    "{name} duplicate source font scalar"
                );
            }
        }
        let mut style_claims = BTreeSet::new();
        for b in &m.fonts.styles {
            let value = m.documents[b.document].field_text(b.field);
            assert_eq!(
                value
                    .chars()
                    .skip(b.offset)
                    .take(b.length)
                    .collect::<String>(),
                b.text
            );
            let fi = match b.field {
                Field::Paragraph => 0,
                Field::After => 1,
                Field::Before => 2,
            };
            for i in b.offset..b.offset + b.length {
                assert!(
                    style_claims.insert((b.document, fi, i)),
                    "{name} repeated font style scalar"
                );
            }
            for &id in &b.glyphs {
                let page = sources[b.page].page();
                let g = &page.glyphs[id];
                assert!(!g.invisible);
                let font = page.fonts[g.font].name.to_ascii_lowercase();
                match b.style.family {
                    Some(media::fonts::Family::Gothic) => assert!(font.contains("gothic")),
                    Some(media::fonts::Family::Mincho) => assert!(font.contains("mincho")),
                    None => {}
                }
            }
        }
        let required_gothic = m
            .fonts
            .bindings
            .iter()
            .flat_map(|b| {
                let page = sources[b.page].page();
                b.glyphs
                    .iter()
                    .copied()
                    .filter(|&id| {
                        page.fonts[page.glyphs[id].font]
                            .name
                            .to_ascii_lowercase()
                            .contains("gothic")
                    })
                    .map(move |id| (b.page, id))
            })
            .collect::<BTreeSet<_>>();
        let actual_gothic = m
            .fonts
            .styles
            .iter()
            .filter(|b| b.style.family == Some(media::fonts::Family::Gothic))
            .flat_map(|b| b.glyphs.iter().map(move |&id| (b.page, id)))
            .collect::<BTreeSet<_>>();
        assert_eq!(
            actual_gothic, required_gothic,
            "{name} all original Gothic glyphs, including mixed-family words, are retained"
        );
        let expected = m
            .documents
            .iter()
            .flat_map(|d| {
                [Field::Paragraph, Field::After, Field::Before]
                    .map(|f| d.field_text(f).chars().count())
            })
            .sum::<usize>();
        assert_eq!(
            claims.len(),
            expected,
            "{name} all native field scalars have original font sizes"
        );
        assert!(
            m.fonts.unresolved.is_empty(),
            "{name} unresolved source fonts"
        );
        assert_eq!(
            m.fonts.rubies.len(),
            m.ruby.bindings.len(),
            "{name} every ruby has native size/raise"
        );
        eprintln!(
            "FONT-COVERAGE {name}: {}/{} scalars; {} unresolved words",
            claims.len(),
            expected,
            m.fonts.unresolved.len()
        );
    }
}

fn simple_pdf(content: &str) -> Vec<u8> {
    simple_pdf_font(content, "Helvetica")
}
fn simple_pdf_font(content: &str, font: &str) -> Vec<u8> {
    let objects=["<< /Type /Catalog /Pages 2 0 R >>".to_owned(),"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".into(),"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 1000 1000] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>".into(),format!("<< /Type /Font /Subtype /Type1 /BaseFont /{font} >>"),format!("<< /Length {} >>\nstream\n{}\nendstream",content.len(),content)];
    let mut bytes = b"%PDF-1.4\n".to_vec();
    let mut offsets = vec![0];
    for (i, o) in objects.iter().enumerate() {
        offsets.push(bytes.len());
        bytes.extend(format!("{} 0 obj\n{}\nendobj\n", i + 1, o).as_bytes());
    }
    let start = bytes.len();
    bytes.extend(format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes());
    for n in offsets.iter().skip(1) {
        bytes.extend(format!("{n:010} 00000 n \n").as_bytes());
    }
    bytes.extend(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{}\n%%EOF\n",
            objects.len() + 1,
            start
        )
        .as_bytes(),
    );
    bytes
}
#[test]
fn source_sizes_follow_native_text_and_page_transforms_without_bbox_font_estimates() {
    for scale in [0.5, 1., 2.] {
        let pdf=simple_pdf(&format!("q {scale} 0 0 {scale} 130 170 cm BT /F1 8 Tf 1 0 0 1 100 250 Tm (Arbitrary Material) Tj ET Q"));
        let m = media::analyze_pdf(&pdf).unwrap();
        assert!(!m.fonts.bindings.is_empty());
        assert!(m.fonts.bindings.iter().all(|b| b.points == 8. * scale));
    }
}

#[test]
fn changed_native_identity_or_complete_target_stream_cannot_borrow_a_font() {
    let pdf = simple_pdf("BT /F1 8 Tf 1 0 0 1 100 250 Tm (Material) Tj ET");
    let m = media::analyze_pdf(&pdf).unwrap();
    assert!(!m.fonts.bindings.is_empty());
    for change in 0..4 {
        let mut a = m.source_layout.clone();
        let w = a.pages[0]
            .words
            .iter_mut()
            .find(|w| w.status == kanpo_parser::trace::WordStatus::Used)
            .unwrap();
        match change {
            0 => w.raw.push('X'),
            1 => w.source_box[0] += 1.,
            2 => w.id += 100,
            _ => {}
        }
        let mut docs = m.documents.clone();
        if change == 3 {
            docs[0].blocks.push(media::Block::Paragraph {
                runs: vec![media::Inline::Text {
                    text: "Different".into(),
                    underline: false,
                }],
            });
        }
        let f = media::fonts::analyze_documents(&pdf, &a, &docs).unwrap();
        assert!(f.bindings.is_empty(), "untrusted identity/field {change}");
    }
}

#[test]
fn explicit_native_font_names_authorize_only_their_own_typography() {
    for (font, family, bold) in [
        ("ExampleGothic", Some(media::fonts::Family::Gothic), None),
        (
            "ExampleMincho-Light",
            Some(media::fonts::Family::Mincho),
            Some(false),
        ),
        ("Helvetica-Bold", None, Some(true)),
    ] {
        let pdf = simple_pdf_font(
            "BT /F1 8 Tf 1 0 0 1 100 250 Tm (Arbitrary Material) Tj ET",
            font,
        );
        let m = media::analyze_pdf(&pdf).unwrap();
        assert!(!m.fonts.styles.is_empty());
        assert!(m
            .fonts
            .styles
            .iter()
            .all(|b| b.style.family == family && b.style.bold == bold));
    }
}

#[test]
fn appended_ocr_keeps_complete_native_prefix_styles_without_styling_the_ocr_tail() {
    let pdf = simple_pdf_font(
        "BT /F1 8 Tf 1 0 0 1 100 250 Tm (Native Material) Tj ET",
        "ExampleGothic",
    );
    let source = media::analyze_pdf(&pdf).unwrap();
    let mut a = source.source_layout.clone();
    // Mirror the typed append policy: native traces stay untouched and OCR
    // articles are appended. Even equal spellings in that tail cannot borrow.
    let article = kanpo_parser::model::Document::from_text("Native Material OCR tail");
    a.articles.push(kanpo_parser::ArticleAnalysis {
        document: article,
        text: "Native Material OCR tail".into(),
        notices: vec![],
    });
    let docs = a
        .articles
        .iter()
        .map(|a| media::Document::from(&a.document))
        .collect::<Vec<_>>();
    let fonts = media::fonts::analyze_documents(&pdf, &a, &docs).unwrap();
    assert!(!fonts.styles.is_empty());
    assert!(fonts.styles.iter().all(|b| b.document == 0));
    assert!(fonts.bindings.iter().all(|b| b.document == 0));
    assert_eq!(
        fonts.styles.iter().map(|b| b.length).sum::<usize>(),
        source.documents[0]
            .field_text(Field::Paragraph)
            .chars()
            .count()
    );
    let mut changed = docs.clone();
    if let media::Block::Paragraph { runs } = &mut changed[0].blocks[0] {
        runs.push(media::Inline::Text {
            text: "Changed native".into(),
            underline: false,
        });
    }
    assert!(media::fonts::analyze_documents(&pdf, &a, &changed)
        .unwrap()
        .styles
        .is_empty());
}
