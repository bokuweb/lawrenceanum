//! Images require the separate opt-in OCR CLI. Default native parsing must work
//! without an OCR binary/model and must not add image text to native articles.
fn scan_pixels() -> String {
    // Raster letters "SCAN", with no corresponding PDF text/font operation.
    let rows = [
        "11111 11111 01110 10001",
        "10000 10000 10001 11001",
        "10000 10000 10001 11001",
        "11111 10000 11111 10101",
        "00001 10000 10001 10011",
        "00001 10000 10001 10011",
        "11111 11111 10001 10001",
    ];
    let mut hex = String::new();
    for row in rows {
        for pixel in row.bytes() {
            hex.push_str(if pixel == b'1' { "00" } else { "FF" });
        }
    }
    hex.push('>');
    hex
}

fn pdf(text: bool, image: bool) -> Vec<u8> {
    let mut content = String::new();
    if text {
        content.push_str("BT /F1 12 Tf 1 0 0 1 100 600 Tm (Native text) Tj ET\n");
    }
    if image {
        content.push_str("q 184 0 0 56 100 300 cm /Scan Do Q\n");
    }
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_owned(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_owned(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 595 842] /Resources << /Font << /F1 4 0 R >> /XObject << /Scan 6 0 R >> >> /Contents 5 0 R >>".to_owned(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_owned(),
        format!("<< /Length {} >>\nstream\n{content}endstream", content.len()),
        format!("<< /Type /XObject /Subtype /Image /Width 23 /Height 7 /ColorSpace /DeviceGray /BitsPerComponent 8 /Filter /ASCIIHexDecode /Length {} >>\nstream\n{}\nendstream", scan_pixels().len(), scan_pixels()),
    ];
    let mut out = b"%PDF-1.4\n".to_vec();
    let mut offsets = vec![];
    for (i, body) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n{body}\nendobj\n", i + 1).as_bytes());
    }
    let xref = out.len();
    out.extend_from_slice(b"xref\n0 7\n0000000000 65535 f \n");
    for offset in offsets {
        out.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(
        format!("trailer\n<< /Size 7 /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n").as_bytes(),
    );
    out
}

fn check(text: bool) -> kanpo_parser::Analysis {
    let with_image = pdf(text, true);
    let sources = kanpo_pdf::extract_pages_with_xobject_sources(&with_image).unwrap();
    assert_eq!(sources.len(), 1);
    assert_eq!(
        sources[0].images().len(),
        1,
        "the fixture must draw an image"
    );
    assert_eq!(sources[0].images()[0].pixel_width, Some(23));
    assert_eq!(sources[0].images()[0].pixel_height, Some(7));
    assert_eq!(
        sources[0].images()[0].encoded_bytes(),
        scan_pixels().as_bytes()
    );
    assert!(sources[0].capture_gaps().is_empty());
    let analyze =
        |bytes: &[u8]| kanpo_parser::analyze_item(&kanpo_parser::page_inputs(bytes).unwrap());
    let actual = analyze(&with_image);
    assert_eq!(
        serde_json::to_value(&actual).unwrap(),
        serde_json::to_value(analyze(&pdf(text, false))).unwrap(),
        "adding an image must not change default native extraction or article ownership"
    );
    actual
}

#[test]
fn image_only_pages_are_not_automatically_ocrd() {
    let a = check(false);
    assert!(a.text.is_empty());
    assert_eq!(a.pages.len(), 1);
    assert!(a.pages[0].words.is_empty());
    assert!(a.articles.is_empty());
}

#[test]
fn images_do_not_augment_or_replace_text_on_hybrid_pages() {
    let a = check(true);
    assert!(a.text.contains("Native text"));
    assert!(!a.pages[0].words.is_empty());
}
