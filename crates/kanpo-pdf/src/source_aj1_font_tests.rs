use super::*;
fn doc() -> Document {
    Document::load(
        include_bytes!(
            "../../kanpo-parser/tests/archive/pages/20260817g00183/20260817g001830048.pdf"
        )
        .to_vec(),
    )
    .unwrap()
}
fn source(d: &Document) -> Font {
    let p = d.pages();
    let fonts = d.dict_get(&p[0].resources, b"Font").unwrap();
    let font = d.dict_get(fonts.as_dict().unwrap(), b"C0_4").unwrap();
    Font::load(d, font.as_dict().unwrap())
}
#[test]
fn actual_CFF_and_real_CID_resolve() {
    let d = doc();
    assert_eq!(source(&d).source_aj1_fallback(&d, 18000, 18000), Some('瀀'));
}
#[test]
fn foreign_and_missing_PDF_ordering_refused() {
    let d = doc();
    for o in [None, Some("GB1"), Some("Korea1"), Some("Npb1")] {
        let mut f = source(&d);
        f.ordering = o.map(String::from);
        assert!(f.source_aj1_fallback(&d, 18000, 18000).is_none())
    }
}
#[test]
fn unsupported_encoding_and_CID_guess_refused() {
    let d = doc();
    let mut f = source(&d);
    f.identity_encoding = false;
    assert!(f.source_aj1_fallback(&d, 18000, 18000).is_none());
    assert!(source(&d).source_aj1_fallback(&d, 18001, 18000).is_none());
}
#[test]
fn declared_broken_empty_or_partial_ToUnicode_never_becomes_fallback() {
    let d = doc();
    let mut f = source(&d);
    f.declared_to_unicode = true;
    assert!(!f.has_to_unicode);
    assert!(f.source_aj1_fallback(&d, 18000, 18000).is_none());
}
#[test]
fn actual_ToUnicode_ordinary_or_multichar_keeps_priority() {
    let d = doc();
    for text in ["6551", "65516551"] {
        let mut f = source(&d);
        f.to_unicode=Some(CMap::parse(format!("1 begincodespacerange <0000> <ffff> endcodespacerange 1 beginbfchar <4650> <{text}> endbfchar").as_bytes()));
        f.declared_to_unicode = true;
        f.has_to_unicode = true;
        assert_eq!(
            f.unicode(&CharCode {
                code: 18000,
                len: 2,
                cid: 18000
            }),
            if text == "6551" { "救" } else { "救救" }
        );
        assert!(f.source_aj1_fallback(&d, 18000, 18000).is_none());
    }
}
#[test]
fn missing_and_malformed_embedded_CFF_refused() {
    let d = doc();
    let mut f = source(&d);
    f.source_cff = None;
    assert!(f.source_aj1_fallback(&d, 18000, 18000).is_none());
    let mut f = source(&d);
    let s = f.source_cff.as_mut().unwrap();
    s.dict.remove(&b"Filter"[..]);
    s.data = Vec::new();
    assert!(f.source_aj1_fallback(&d, 18000, 18000).is_none());
}
#[test]
fn standard_lookup_without_charset_membership_is_insufficient() {
    let d = doc();
    let f = source(&d);
    let bytes = d.decode_stream(f.source_cff.as_ref().unwrap()).unwrap();
    let set = crate::cff_aj1::standard_cids(&bytes).unwrap();
    let cid = (1..23060)
        .find(|&cid| aj1(cid).is_some() && set.binary_search(&(cid as u16)).is_err())
        .unwrap();
    assert!(f.source_aj1_fallback(&d, cid, cid).is_none());
}
#[test]
fn non_CID_and_extended_private_CID_not_guessed() {
    let d = doc();
    let mut f = source(&d);
    f.composite = false;
    assert!(f.source_aj1_fallback(&d, 18000, 18000).is_none());
    assert!(source(&d).source_aj1_fallback(&d, 25937, 25937).is_none());
}
