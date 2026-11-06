use super::*;
fn original_font() -> Vec<u8> {
    let doc =
        kanpo_pdf::Document::load(include_bytes!("../tests/data/workbook/case-04.pdf").to_vec())
            .unwrap();
    let fonts = doc.dict_get(&doc.pages()[0].resources, b"Font").unwrap();
    let font = doc.dict_get(fonts.as_dict().unwrap(), b"G3").unwrap();
    let children = doc
        .dict_get(font.as_dict().unwrap(), b"DescendantFonts")
        .unwrap();
    let child = doc.resolve(&children.as_array().unwrap()[0]);
    let descriptor = doc
        .dict_get(child.as_dict().unwrap(), b"FontDescriptor")
        .unwrap();
    let file = doc
        .dict_get(descriptor.as_dict().unwrap(), b"FontFile3")
        .unwrap();
    if let kanpo_pdf::Object::Stream(stream) = file {
        doc.decode_stream(&stream).unwrap()
    } else {
        panic!("source CFF stream missing")
    }
}
fn stream(bytes: &[u8]) -> Vec<u8> {
    let mut out = format!("<< /Length {} >>\nstream\n", bytes.len()).into_bytes();
    out.extend(bytes);
    out.extend(b"\nendstream");
    out
}
fn pdf(body: &str, x: f64, matrix: [i32; 4]) -> Vec<u8> {
    pdf_with_extra(body, x, matrix, "")
}
fn pdf_with_extra(body: &str, x: f64, matrix: [i32; 4], extra: &str) -> Vec<u8> {
    let [a, b, c, d] = matrix;
    let tx = 300 - 20 * a - 150 * c;
    let ty = 400 - 20 * b - 150 * d;
    let content=format!("q {a} {b} {c} {d} {tx} {ty} cm {extra} BT /Frame 8 Tf 1 0 0 1 20 150 Tm <626E> Tj /Text 6 Tf 1 0 0 1 {x} 150.75 Tm ({body}) Tj /Text 8 Tf 1 0 0 1 29.5 150 Tm (tail) Tj ET Q");
    let objects=vec![
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Count 1 /Kids [3 0 R] >>".to_vec(),
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 600 800] /Resources << /Font << /Frame 5 0 R /Text 10 0 R >> >> /Contents 4 0 R >>".to_vec(),
        stream(content.as_bytes()),
        b"<< /Type /Font /Subtype /Type0 /BaseFont /UnrelatedFrame /Encoding /Identity-H /DescendantFonts [6 0 R] /ToUnicode 9 0 R >>".to_vec(),
        b"<< /Type /Font /Subtype /CIDFontType0 /BaseFont /UnrelatedFrame /CIDSystemInfo << /Registry (Other) /Ordering (Other) /Supplement 0 >> /FontDescriptor 7 0 R /DW 1000 >>".to_vec(),
        b"<< /Type /FontDescriptor /FontName /UnrelatedFrame /Flags 4 /FontBBox [0 -120 1000 880] /Ascent 880 /Descent -120 /FontFile3 8 0 R >>".to_vec(),
        {let bytes=original_font();let mut s=format!("<< /Length {} /Subtype /CIDFontType0C >>\nstream\n",bytes.len()).into_bytes();s.extend(bytes);s.extend(b"\nendstream");s},
        stream(b"1 begincodespacerange <0000> <FFFF> endcodespacerange 1 beginbfchar <626E> <25A1> endbfchar"),
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>".to_vec(),
    ];
    let mut out = b"%PDF-1.7\n".to_vec();
    let mut offsets = vec![0];
    for (i, obj) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.extend(format!("{} 0 obj\n", i + 1).as_bytes());
        out.extend(obj);
        out.extend(b"\nendobj\n");
    }
    let start = out.len();
    out.extend(format!("xref\n0 {}\n0000000000 65535 f \n", offsets.len()).as_bytes());
    for offset in &offsets[1..] {
        out.extend(format!("{offset:010} 00000 n \n").as_bytes());
    }
    out.extend(
        format!(
            "trailer << /Size {} /Root 1 0 R >>\nstartxref\n{start}\n%%EOF\n",
            offsets.len()
        )
        .as_bytes(),
    );
    out
}

#[test]
fn source_frames_enclose_arbitrary_text_under_all_cardinal_rotations_and_reflections() {
    for matrix in [
        [1, 0, 0, 1],
        [-1, 0, 0, 1],
        [1, 0, 0, -1],
        [-1, 0, 0, -1],
        [0, 1, 1, 0],
        [0, 1, -1, 0],
        [0, -1, 1, 0],
        [0, -1, -1, 0],
    ] {
        for body in ["A", "9", "ii"] {
            let bytes = pdf(body, 21., matrix);
            let result = super::super::analyze_pdf(&bytes).unwrap();
            assert_eq!(
                result.frames.bindings.len(),
                1,
                "{matrix:?} {body}: {:?}",
                result.frames.unresolved
            );
            let b = &result.frames.bindings[0];
            assert_eq!(b.text, body);
            assert_eq!(b.removed_text.chars().count(), b.removed_length);
            assert_eq!(
                result.documents[b.document]
                    .field_text(b.field)
                    .chars()
                    .skip(b.offset)
                    .take(b.length)
                    .collect::<String>(),
                body
            );
            assert!(result.frames.unresolved.is_empty());
        }
    }
}

#[test]
fn a_glyph_crossing_the_frame_border_is_not_removed_from_body_text() {
    let bytes = pdf("A", 27., [1, 0, 0, 1]);
    let result = super::super::analyze_pdf(&bytes).unwrap();
    assert!(result.frames.bindings.is_empty());
    assert_eq!(result.frames.unresolved.len(), 1);
    assert_eq!(
        result
            .documents
            .iter()
            .map(|d| d.field_text(Field::Paragraph))
            .collect::<String>(),
        crate::source_underlines::field_text(&result.native, Field::Paragraph)
    );
}

#[test]
fn frame_scalar_deletions_preserve_non_bmp_text_and_reject_partial_projections() {
    let mut value = "甲𠀀□乙".to_string();
    remove(&mut value, &[(2, 1)]).unwrap();
    assert_eq!(value, "甲𠀀乙");
    assert_eq!(project(5, &[(1, 2), (4, 1)]), Some(2));
    assert_eq!(project(2, &[(1, 2)]), None);
    assert!(byte_span("甲𠀀□乙", 5, 1).is_none());
}

#[test]
fn unknown_whole_field_or_intervening_paint_cannot_consume_a_frame_character() {
    let bytes = pdf("A", 21., [1, 0, 0, 1]);
    let sources = kanpo_pdf::extract_pages_with_xobject_sources(&bytes).unwrap();
    let mut base = crate::analyze_item(&crate::page_inputs(&bytes).unwrap());
    if let crate::model::Block::Paragraph { runs } = base
        .articles
        .last_mut()
        .unwrap()
        .document
        .blocks
        .last_mut()
        .unwrap()
    {
        runs.push(crate::model::Run::plain("other complete field material"));
    } else {
        panic!("plain synthetic field missing")
    }
    let (observed, plans, pending) = apply(&base, &sources, None).unwrap();
    assert!(plans.is_empty());
    assert_eq!(pending.len(), 1);
    assert_eq!(
        serde_json::to_value(observed).unwrap(),
        serde_json::to_value(base).unwrap()
    );
    let bytes = pdf_with_extra("A", 21., [1, 0, 0, 1], "0.5 w 20 149 m 28 149 l S");
    let result = super::super::analyze_pdf(&bytes).unwrap();
    assert!(result.frames.bindings.is_empty());
    assert_eq!(result.frames.unresolved.len(), 1);
    assert!(result.frames.unresolved[0]
        .reason
        .contains("paint intersects"));
}

#[test]
fn frame_and_text_spans_can_share_one_raw_word_without_guessing_scalar_positions() {
    let bytes = include_bytes!("../tests/data/workbook/case-04.pdf");
    let sources = kanpo_pdf::extract_pages_with_xobject_sources(bytes).unwrap();
    let s = sources[0].native_sources();
    let raw = [
        s.glyph_bbox_text(834).unwrap(),
        s.glyph_bbox_text(835).unwrap(),
    ]
    .concat();
    let word = crate::trace::Word {
        id: 0,
        x0: 0.,
        y0: 0.,
        x1: 1.,
        y1: 1.,
        source_box: [0., 0., 1., 1.],
        raw,
        text: "□０".into(),
        status: WordStatus::Used,
    };
    assert_eq!(
        segment(&word, &[834, 835], &[834].into_iter().collect(), s),
        Some((0, 1, "□".into()))
    );
    assert_eq!(
        segment(&word, &[834, 835], &[835].into_iter().collect(), s),
        Some((1, 1, "０".into()))
    );
    let mut foreign = word;
    foreign.text = "□１".into();
    assert!(segment(&foreign, &[834, 835], &[835].into_iter().collect(), s).is_none());
}
