use super::*;
fn doc() -> Document {
    Document::load(
        std::fs::read(
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../kanpo-parser/tests/data/native-font-aliases/20260917g002080090.pdf"),
        )
        .unwrap(),
    )
    .unwrap()
}
fn font(d: &Document, key: &[u8]) -> Font {
    let ps = d.pages();
    let fs = d.dict_get(&ps[0].resources, b"Font").unwrap();
    let f = d.dict_get(fs.as_dict().unwrap(), key).unwrap();
    Font::load(d, f.as_dict().unwrap())
}
fn code() -> CharCode {
    CharCode {
        code: 0x5a91,
        len: 2,
        cid: 0x5a91,
    }
}
#[test]
fn alias157_original_pua_and_identity_routes_refer_to_the_same_program_and_glyph() {
    let d = doc();
    let a = font(&d, b"C0_6").source_alias_key(&d, &code()).unwrap();
    let b = font(&d, b"C0_8").source_alias_key(&d, &code()).unwrap();
    assert_eq!(a.program, b.program);
    assert_eq!(a.cid, b.cid);
    assert_eq!(a.role, Role::PrivateUnicode);
    assert_eq!(b.role, Role::UnresolvedIdentity);
}
#[test]
fn alias157_arbitrary_font_names_neither_prove_nor_block_identity() {
    let d = doc();
    let mut a = font(&d, b"C0_6");
    let mut b = font(&d, b"C0_8");
    a.base_font = "Unrelated face A".into();
    b.base_font = "Entirely different name B".into();
    let a = a.source_alias_key(&d, &code()).unwrap();
    let b = b.source_alias_key(&d, &code()).unwrap();
    assert_eq!(a.program, b.program);
    assert_eq!(a.cid, b.cid);
}
#[test]
fn alias157_writing_modes_do_not_change_the_original_glyph_identity() {
    let d = doc();
    let mut a = font(&d, b"C0_6");
    let mut b = font(&d, b"C0_8");
    a.vertical = true;
    b.vertical = false;
    let a = a.source_alias_key(&d, &code()).unwrap();
    let b = b.source_alias_key(&d, &code()).unwrap();
    assert_eq!(a.program, b.program);
    assert_eq!(a.cid, b.cid);
}
#[test]
fn alias157_explicit_ordinary_and_multiscalar_unicode_keep_priority() {
    let d = doc();
    for text in ["6a0b", "6a0b53e3"] {
        let mut f = font(&d, b"C0_6");
        f.to_unicode=Some(CMap::parse(format!("1 begincodespacerange <0000> <ffff> endcodespacerange 1 beginbfchar <5a91> <{text}> endbfchar").as_bytes()));
        assert!(f.source_alias_key(&d, &code()).is_none());
    }
}
#[test]
fn alias157_a_missing_or_partial_tounicode_map_cannot_claim_a_private_mapping() {
    let d = doc();
    for partial in [false, true] {
        let mut f = font(&d, b"C0_6");
        f.to_unicode = if partial {
            Some(CMap::parse(b"1 beginbfchar <0001> <e000> endbfchar"))
        } else {
            None
        };
        assert!(f.source_alias_key(&d, &code()).is_none());
    }
}
#[test]
fn alias157_an_unsupported_encoding_is_not_an_original_cid_route() {
    let d = doc();
    for key in [b"C0_6", b"C0_8"] {
        let mut f = font(&d, key);
        f.identity_encoding = false;
        assert!(f.source_alias_key(&d, &code()).is_none());
    }
}
#[test]
fn alias157_simple_or_non_cff_cid_fonts_supply_no_program_authority() {
    let d = doc();
    for key in [b"C0_6", b"C0_8"] {
        for case in [0, 1] {
            let mut f = font(&d, key);
            if case == 0 {
                f.composite = false
            } else {
                f.source_cid0 = false
            };
            assert!(f.source_alias_key(&d, &code()).is_none());
        }
    }
}
#[test]
fn alias157_code_length_and_actual_cid_must_match_the_original_identity_route() {
    let d = doc();
    for key in [b"C0_6", b"C0_8"] {
        let f = font(&d, key);
        for c in [
            CharCode {
                code: 0x5a91,
                cid: 0x5a92,
                len: 2,
            },
            CharCode {
                code: 0x5a91,
                cid: 0x5a91,
                len: 1,
            },
            CharCode {
                code: 65536,
                cid: 65536,
                len: 2,
            },
        ] {
            assert!(f.source_alias_key(&d, &c).is_none());
        }
    }
}
#[test]
fn alias157_broken_or_declared_tounicode_cannot_masquerade_as_unresolved_identity() {
    let d = doc();
    for case in [0, 1] {
        let mut f = font(&d, b"C0_8");
        if case == 0 {
            f.declared_to_unicode = true
        } else {
            f.has_to_unicode = true
        };
        assert!(f.source_alias_key(&d, &code()).is_none());
    }
}
#[test]
fn alias157_other_collections_without_unicode_are_not_verified_identity_decoders() {
    let d = doc();
    for order in [None, Some("Japan1"), Some("Npb1")] {
        let mut f = font(&d, b"C0_8");
        f.ordering = order.map(String::from);
        assert!(f.source_alias_key(&d, &code()).is_none());
    }
}
#[test]
fn alias157_missing_empty_or_malformed_embedded_programs_supply_no_alias() {
    let d = doc();
    for case in [0, 1, 2] {
        let mut f = font(&d, b"C0_6");
        if case == 0 {
            f.source_cff = None
        } else {
            let s = f.source_cff.as_mut().unwrap();
            s.dict.remove(&b"Filter"[..]);
            s.data = if case == 1 { vec![] } else { vec![1, 0, 4, 4] };
        }
        assert!(f.source_alias_key(&d, &code()).is_none());
    }
}

fn keys(d: &Document) -> (FontGlyph, FontGlyph) {
    (
        font(d, b"C0_6").source_alias_key(d, &code()).unwrap(),
        font(d, b"C0_8").source_alias_key(d, &code()).unwrap(),
    )
}
#[test]
fn alias157_program_identity_links_private_to_original_identity_only() {
    let d = doc();
    let (a, b) = keys(&d);
    assert!(a.matches_identity_source(&b));
}
#[test]
fn alias157_same_font_names_do_not_equal_different_embedded_program_bytes() {
    let d = doc();
    let (a, mut b) = keys(&d);
    let mut bytes = b.program.to_vec();
    bytes[0] ^= 1;
    b.program = bytes.into();
    assert!(!a.matches_identity_source(&b));
}
#[test]
fn alias157_the_same_program_does_not_equal_different_cids() {
    let d = doc();
    let (a, mut b) = keys(&d);
    b.cid ^= 1;
    assert!(!a.matches_identity_source(&b));
}
#[test]
fn alias157_relation_is_directed_and_never_uses_another_private_mapping() {
    let d = doc();
    let (a, b) = keys(&d);
    assert!(!b.matches_identity_source(&a));
    assert!(!a.matches_identity_source(&a));
    assert!(!b.matches_identity_source(&b));
}
#[test]
fn alias157_structural_program_identity_grants_no_new_aj1_unicode_authority() {
    let d = doc();
    for key in [b"C0_6", b"C0_8"] {
        let f = font(&d, key);
        assert!(f.source_alias_key(&d, &code()).is_some());
        assert!(f.source_aj1_fallback(&d, &code()).is_none());
    }
}
#[test]
fn alias157_private_scalar_plus_another_scalar_is_not_one_unknown_glyph() {
    let d = doc();
    let mut f = font(&d, b"C0_6");
    f.to_unicode = Some(CMap::parse(b"1 beginbfchar <5a91> <e47f53e3> endbfchar"));
    assert!(f.source_alias_key(&d, &code()).is_none());
}
#[test]
fn alias157_cid_must_exist_in_the_actual_embedded_charset() {
    let d = doc();
    let f = font(&d, b"C0_8");
    assert!(f
        .source_alias_key(
            &d,
            &CharCode {
                code: 65535,
                cid: 65535,
                len: 2
            }
        )
        .is_none());
}
#[test]
fn alias157_stream_subtype_must_be_original_cid_cff() {
    let d = doc();
    let mut f = font(&d, b"C0_6");
    f.source_cff
        .as_mut()
        .unwrap()
        .dict
        .insert(b"Subtype".to_vec(), Object::Name(b"Type1C".to_vec()));
    assert!(f.source_alias_key(&d, &code()).is_none());
}
#[test]
fn alias157_contradictory_tounicode_flags_supply_no_candidate() {
    let d = doc();
    for case in [0, 1] {
        let mut f = font(&d, b"C0_6");
        if case == 0 {
            f.has_to_unicode = false
        } else {
            f.declared_to_unicode = false
        };
        assert!(f.source_alias_key(&d, &code()).is_none());
    }
}

fn captured(d: &Document) -> (Vec<crate::page::Glyph>, Vec<Option<FontGlyph>>) {
    let page = crate::page::extract_page(d, &d.pages()[0]);
    let mut keys = vec![None; page.glyphs.len()];
    let (target, source) = keys_from_original(d);
    for (i, g) in page.glyphs.iter().enumerate() {
        if g.code == code().code && g.cid == code().cid {
            if g.text == "\u{e47f}" {
                keys[i] = Some(target.clone());
            } else if g.text == char::from_u32(code().cid).unwrap().to_string() {
                keys[i] = Some(source.clone());
            }
        }
    }
    (page.glyphs, keys)
}
fn keys_from_original(d: &Document) -> (FontGlyph, FontGlyph) {
    keys(d)
}
fn pair(glyphs: &[crate::page::Glyph]) -> (usize, usize) {
    (
        glyphs.iter().position(|g| g.text == "\u{e47f}").unwrap(),
        glyphs
            .iter()
            .position(|g| g.cid == code().cid && g.text != "\u{e47f}")
            .unwrap(),
    )
}
#[test]
fn alias157_capture_links_actual_visible_occurrences_not_names_or_modes() {
    let d = doc();
    let (g, k) = captured(&d);
    let (t, s) = pair(&g);
    let a = Aliases::capture(&g, &k);
    assert_eq!(a.links.get(t), Some(&Some((s, code().cid as u16))));
    assert_eq!(a.links.get(s), Some(&None));
}
#[test]
fn alias157_capture_requires_both_original_occurrences() {
    let d = doc();
    let (g, k) = captured(&d);
    let (t, s) = pair(&g);
    for id in [t, s] {
        let mut k = k.clone();
        k[id] = None;
        assert!(Aliases::capture(&g, &k).links.iter().all(Option::is_none));
    }
}
#[test]
fn alias157_capture_refuses_invisible_or_nonfinite_or_collapsed_originals() {
    let d = doc();
    let (g, k) = captured(&d);
    let (t, s) = pair(&g);
    for id in [t, s] {
        for case in 0..5 {
            let mut g = g.clone();
            match case {
                0 => g[id].invisible = true,
                1 => g[id].x = f64::NAN,
                2 => g[id].size = 0.,
                3 => g[id].bbox[2] = g[id].bbox[0],
                _ => g[id].end_y = f64::INFINITY,
            };
            assert!(Aliases::capture(&g, &k).links.iter().all(Option::is_none));
        }
    }
}
#[test]
fn alias157_capture_refuses_stale_keys_or_missing_original_glyphs() {
    let d = doc();
    let (g, k) = captured(&d);
    let (t, s) = pair(&g);
    for id in [t, s] {
        for case in 0..4 {
            let mut g = g.clone();
            match case {
                0 => g[id].code += 1,
                1 => g[id].cid += 1,
                2 => g[id].text = "ordinary".into(),
                _ => g[id].text.push('x'),
            };
            assert!(Aliases::capture(&g, &k).links.iter().all(Option::is_none));
        }
    }
    assert!(Aliases::capture(&g, &k[..k.len() - 1])
        .links
        .iter()
        .all(Option::is_none));
}
#[test]
fn alias157_capture_keeps_different_programs_and_cids_unrelated() {
    let d = doc();
    let (g, k) = captured(&d);
    let (t, _) = pair(&g);
    for case in 0..2 {
        let mut k = k.clone();
        let a = k[t].as_mut().unwrap();
        if case == 0 {
            let mut p = a.program.to_vec();
            p[0] ^= 1;
            a.program = p.into();
        } else {
            a.cid ^= 1;
        }
        assert!(Aliases::capture(&g, &k).links.iter().all(Option::is_none));
    }
}

#[test]
fn alias157_source_xhtml_carries_relation_and_preserves_raw_native_page() {
    let d = doc();
    let bytes = std::fs::read(
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../kanpo-parser/tests/data/native-font-aliases/20260917g002080090.pdf"),
    )
    .unwrap();
    let original = crate::page::extract_page(&d, &d.pages()[0]);
    let owned = crate::extract_pages_with_aj1_sources(&bytes).unwrap();
    assert_eq!(format!("{:?}", original), format!("{:?}", owned[0].page()));
    let native = crate::bbox::bbox_layout_with_native_styles(&owned);
    assert!(native.contains("data-native-font-aliases=\"1 475 477 162.750000 98.859700 178.750000 107.043700 0:58495:385:23185\""),"actual glyph relation missing");
    let legacy = crate::bbox::bbox_layout_with_aj1_sources(&owned);
    assert!(!legacy.contains("data-native-font-aliases"));
    assert!(native.contains("\u{e47f}口</word>"));
}

#[test]
fn alias157_capture_target_raw_scalar_must_match_the_actual_tounicode_mapping() {
    let d = doc();
    let (mut g, k) = captured(&d);
    let (t, _) = pair(&g);
    g[t].text = "\u{e480}".into();
    assert!(Aliases::capture(&g, &k).links.iter().all(Option::is_none));
}
#[test]
fn alias157_producer_word_relation_requires_contiguous_single_scalar_originals() {
    let d = doc();
    let (g, k) = captured(&d);
    let a = Aliases::capture(&g, &k);
    let page = crate::page::extract_page(&d, &d.pages()[0]);
    let word = crate::bbox::words(&page)
        .into_iter()
        .find(|w| w.text == "\u{e47f}口")
        .unwrap();
    assert_eq!(
        a.word_attribute(&page, &word).as_deref(),
        Some("1 475 477 162.750000 98.859700 178.750000 107.043700 0:58495:385:23185")
    );
    for case in 0..6 {
        let mut w = word.clone();
        match case {
            0 => w.glyphs.reverse(),
            1 => w.glyphs[1] = w.glyphs[0],
            2 => w.glyphs.pop().map(|_| ()).unwrap(),
            3 => w.text.push('x'),
            4 => w.glyphs[0] = usize::MAX,
            _ => w.x_min += 1.,
        };
        assert!(a.word_attribute(&page, &w).is_none());
    }
}
