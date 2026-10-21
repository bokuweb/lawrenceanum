//! Synthetic valid CFF1 source-code completeness controls; no rendering claim.
use crate::*;

fn index(parts: &[Vec<u8>]) -> Vec<u8> {
    if parts.is_empty() {
        return vec![0, 0];
    }
    let mut out = (parts.len() as u16).to_be_bytes().to_vec();
    out.push(2);
    let mut offset = 1u16;
    out.extend(offset.to_be_bytes());
    for p in parts {
        offset += p.len() as u16;
        out.extend(offset.to_be_bytes());
    }
    for p in parts {
        out.extend(p);
    }
    out
}
fn number(n: i32) -> Vec<u8> {
    let mut out = vec![29];
    out.extend(n.to_be_bytes());
    out
}
// Same valid two-GID CID-keyed CFF1 construction as cff_aj1_tests::font:
// explicit charset, both CharStrings, FDArray Private, complete FDSelect.
pub(crate) fn embedded_cff(cid: u8, format: u8, registry: &str) -> Vec<u8> {
    let names = index(&[b"CompletenessControl".to_vec()]);
    let strings = index(&[registry.as_bytes().to_vec(), b"Japan1".to_vec()]);
    let charset = match format {
        0 => vec![0, 0, cid],
        1 => vec![1, 0, cid, 0],
        2 => vec![2, 0, cid, 0, 0],
        _ => unreachable!(),
    };
    let top = |c: i32, cs: i32, fd: i32, fds: i32| {
        let mut out = Vec::new();
        for n in [391, 392, 6] {
            out.extend(number(n));
        }
        out.extend([12, 30]);
        for (n, op) in [
            (c, vec![15]),
            (cs, vec![17]),
            (fd, vec![12, 36]),
            (fds, vec![12, 37]),
        ] {
            out.extend(number(n));
            out.extend(op);
        }
        out
    };
    let charstrings = index(&[vec![14], vec![14]]);
    let fdarray = index(&[[number(0), number(0), vec![18]].concat()]);
    let head = vec![1, 0, 4, 4];
    let start =
        (head.len() + names.len() + index(&[top(0, 0, 0, 0)]).len() + strings.len() + 2) as i32;
    let cs = start + charset.len() as i32;
    let fd = cs + charstrings.len() as i32;
    let fds = fd + fdarray.len() as i32;
    [
        head,
        names,
        index(&[top(start, cs, fd, fds)]),
        strings,
        vec![0, 0],
        charset,
        charstrings,
        fdarray,
        vec![0, 0, 0],
    ]
    .concat()
}
fn stream(bytes: &[u8], extra: &str) -> Vec<u8> {
    let mut out = format!("<< /Length {} {extra} >>\nstream\n", bytes.len()).into_bytes();
    out.extend(bytes);
    out.extend(b"\nendstream");
    out
}
pub(crate) fn fixture(
    cid: u8,
    vertical: bool,
    format: u8,
    body: &str,
    tounicode: bool,
    registry: &str,
) -> Vec<u8> {
    let encoding = if vertical { "Identity-V" } else { "Identity-H" };
    let mut objects=vec![
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Resources << /Font << /F1 5 0 R >> >> /Contents 4 0 R >>".to_vec(),
        stream(format!("BT /F1 12 Tf 20 150 Td {body} ET\n").as_bytes(),""),
        format!("<< /Type /Font /Subtype /Type0 /BaseFont /CompletenessControl /Encoding /{encoding} /DescendantFonts [6 0 R] {} >>",if tounicode {"/ToUnicode 9 0 R"} else {""}).into_bytes(),
        b"<< /Type /Font /Subtype /CIDFontType0 /BaseFont /CompletenessControl /CIDSystemInfo << /Registry (Adobe) /Ordering (Identity) /Supplement 0 >> /FontDescriptor 7 0 R /DW 1000 >>".to_vec(),
        b"<< /Type /FontDescriptor /FontName /CompletenessControl /Flags 4 /FontBBox [0 -120 1000 880] /Ascent 880 /Descent -120 /CapHeight 880 /StemV 80 /FontFile3 8 0 R >>".to_vec(),
        stream(&embedded_cff(cid,format,registry),"/Subtype /CIDFontType0C"),
    ];
    if tounicode {
        objects.push(stream(format!("1 begincodespacerange <0000> <ffff> endcodespacerange 1 beginbfchar <00{cid:02x}> <0041> endbfchar").as_bytes(),""));
    }
    let mut out = b"%PDF-1.7\n".to_vec();
    let mut offsets = vec![0];
    for (i, o) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.extend(format!("{} 0 obj\n", i + 1).as_bytes());
        out.extend(o);
        out.extend(b"\nendobj\n");
    }
    let x = out.len();
    out.extend(format!("xref\n0 {}\n0000000000 65535 f \n", offsets.len()).as_bytes());
    for off in &offsets[1..] {
        out.extend(format!("{off:010} 00000 n \n").as_bytes());
    }
    out.extend(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{x}\n%%EOF\n",
            offsets.len()
        )
        .as_bytes(),
    );
    out
}
pub(crate) fn cids() -> Vec<u8> {
    let values: Vec<_> = (33..=255u32)
        .filter(|&v| {
            let raw = char::from_u32(v).unwrap();
            font::aj1(v).is_some_and(|c| !c.is_control() && !c.is_whitespace() && c != raw)
                && !raw.is_control()
                && !raw.is_whitespace()
        })
        .collect();
    (0..6)
        .map(|i| values[i * (values.len() - 1) / 5] as u8)
        .collect()
}
fn value<T: serde::Serialize + ?Sized>(x: &T) -> serde_json::Value {
    serde_json::to_value(x).unwrap()
}
fn same_native(full: &[u8], short: &[u8]) {
    let a = extract_pages(full).unwrap();
    let b = extract_pages(short).unwrap();
    assert_eq!(
        value(&a),
        value(&b),
        "native Page/Glyph/Font equality does not prove source code length"
    );
    assert_eq!(bbox::words(&a[0]), bbox::words(&b[0]));
    assert_eq!(bbox::bbox_layout(&a), bbox::bbox_layout(&b));
    for (bytes, pages) in [(full, a), (short, b)] {
        let owned = extract_pages_with_aj1_sources(bytes).unwrap();
        assert_eq!(
            value(&pages),
            value(&owned.iter().map(|s| s.page()).collect::<Vec<_>>())
        );
    }
}
fn candidate(bytes: &[u8], expected: bool) {
    let owned = extract_pages_with_aj1_sources(bytes).unwrap();
    let html = bbox::bbox_layout_with_aj1_sources(&owned);
    assert_eq!(
        html.contains("data-source-aj1-text="),
        expected,
        "owned source html={html}"
    );
}
#[test]
fn complete_identity_codes_and_single_byte_tails_need_distinct_certificates() {
    for cid in cids() {
        for vertical in [false, true] {
            for format in [0, 1, 2] {
                let full = fixture(
                    cid,
                    vertical,
                    format,
                    &format!("<00{cid:02x}> Tj"),
                    false,
                    "Adobe",
                );
                let short = fixture(
                    cid,
                    vertical,
                    format,
                    &format!("<{cid:02x}> Tj"),
                    false,
                    "Adobe",
                );
                same_native(&full, &short);
                candidate(&full, true);
                candidate(&short, false);
            }
        }
    }
}
#[test]
fn a_complete_prefix_does_not_certify_its_incomplete_tail_word() {
    for cid in cids() {
        for vertical in [false, true] {
            let full = fixture(
                cid,
                vertical,
                0,
                &format!("<00{cid:02x}00{cid:02x}00{cid:02x}> Tj"),
                false,
                "Adobe",
            );
            let short = fixture(
                cid,
                vertical,
                0,
                &format!("<00{cid:02x}00{cid:02x}{cid:02x}> Tj"),
                false,
                "Adobe",
            );
            same_native(&full, &short);
            candidate(&full, true);
            candidate(&short, false);
        }
    }
}
#[test]
fn tj_array_string_boundaries_cannot_join_partial_codes() {
    for cid in cids() {
        for vertical in [false, true] {
            let full = fixture(
                cid,
                vertical,
                1,
                &format!("[<00{cid:02x}> 0 <00{cid:02x}>] TJ"),
                false,
                "Adobe",
            );
            let short = fixture(
                cid,
                vertical,
                1,
                &format!("[<{cid:02x}> 0 <{cid:02x}>] TJ"),
                false,
                "Adobe",
            );
            same_native(&full, &short);
            candidate(&full, true);
            candidate(&short, false);
        }
    }
}
#[test]
fn invisible_glyphs_do_not_shift_visible_code_provenance() {
    for cid in cids() {
        for vertical in [false, true] {
            for (first, last, expected) in [
                (format!("00{cid:02x}"), format!("{cid:02x}"), false),
                (format!("{cid:02x}"), format!("00{cid:02x}"), true),
            ] {
                let bytes = fixture(
                    cid,
                    vertical,
                    0,
                    &format!("3 Tr <{first}> Tj 30 0 Td 0 Tr <{last}> Tj"),
                    false,
                    "Adobe",
                );
                let pages = extract_pages(&bytes).unwrap();
                assert_eq!(pages[0].glyphs.len(), 2);
                assert!(pages[0].glyphs[0].invisible);
                assert!(!pages[0].glyphs[1].invisible);
                candidate(&bytes, expected);
            }
        }
    }
}
#[test]
fn declared_tounicode_stays_authoritative_for_both_code_lengths() {
    for cid in cids() {
        for vertical in [false, true] {
            for complete in [false, true] {
                let hex = if complete {
                    format!("00{cid:02x}")
                } else {
                    format!("{cid:02x}")
                };
                let bytes = fixture(cid, vertical, 0, &format!("<{hex}> Tj"), true, "Adobe");
                let pages = extract_pages(&bytes).unwrap();
                assert_eq!(
                    pages[0].glyphs[0].text,
                    if complete {
                        "A".to_string()
                    } else {
                        char::from_u32(cid as u32).unwrap().to_string()
                    }
                );
                candidate(&bytes, false);
            }
        }
    }
}
#[test]
fn foreign_cff_registry_never_obtains_a_certificate() {
    for cid in cids() {
        for complete in [false, true] {
            let hex = if complete {
                format!("00{cid:02x}")
            } else {
                format!("{cid:02x}")
            };
            candidate(
                &fixture(cid, false, 0, &format!("<{hex}> Tj"), false, "Private"),
                false,
            );
        }
    }
}

#[test]
fn complete_decoded_bytes_survive_odd_hex_and_escaped_literal_spelling() {
    // PDF hex strings pad an odd trailing nibble with zero; literal octal
    // escapes decode before Font::decode. All spellings are [0x00, 0x30].
    for vertical in [false, true] {
        for format in [0, 1, 2] {
            let full = fixture(0x30, vertical, format, "<0030> Tj", false, "Adobe");
            let expected =
                bbox::bbox_layout_with_aj1_sources(&extract_pages_with_aj1_sources(&full).unwrap());
            candidate(&full, true);
            for body in ["<003> Tj", r"(\000\060) Tj"] {
                let alternate = fixture(0x30, vertical, format, body, false, "Adobe");
                same_native(&full, &alternate);
                candidate(&alternate, true);
                assert_eq!(
                    bbox::bbox_layout_with_aj1_sources(
                        &extract_pages_with_aj1_sources(&alternate).unwrap()
                    ),
                    expected
                );
            }
        }
    }
}
