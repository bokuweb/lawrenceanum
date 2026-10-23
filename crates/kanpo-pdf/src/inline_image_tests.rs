use crate::*;

fn pdf(content: &[u8], resources: &str) -> Vec<u8> {
    let mut stream = format!("<< /Length {} >>\nstream\n", content.len()).into_bytes();
    stream.extend_from_slice(content);
    stream.extend_from_slice(b"\nendstream");
    let objects = [
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
        format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Resources << {resources} >> /Contents 4 0 R >>").into_bytes(),
        stream,
    ];
    let mut out = b"%PDF-1.7\n".to_vec();
    let mut offsets = vec![0];
    for (i, object) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n", i + 1).as_bytes());
        out.extend_from_slice(object);
        out.extend_from_slice(b"\nendobj\n");
    }
    let xref = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", offsets.len()).as_bytes());
    for offset in &offsets[1..] {
        out.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            offsets.len()
        )
        .as_bytes(),
    );
    out
}
fn val<T: serde::Serialize + ?Sized>(value: &T) -> serde_json::Value {
    serde_json::to_value(value).unwrap()
}
fn line(x: f64, a: f64, b: f64, width: f64) -> PathItem {
    PathItem {
        subpaths: vec![SubPath {
            points: vec![[x, a], [x, b]],
            closed: false,
            curved: false,
            rect: false,
        }],
        stroke: true,
        fill: false,
        line_width: width,
        clip: None,
    }
}
fn owned_matches(bytes: &[u8], pages: &[Page]) {
    let owned = extract_pages_with_aj1_sources(bytes).unwrap();
    assert_eq!(
        val(pages),
        val(&owned.iter().map(|s| s.page()).collect::<Vec<_>>())
    );
    assert_eq!(
        bbox::bbox_layout(pages),
        bbox::bbox_layout(&owned.iter().map(|s| s.page().clone()).collect::<Vec<_>>())
    );
}
fn rooted(bytes: &[u8], suffix: bool) {
    let pages = extract_pages(bytes).unwrap();
    assert_eq!(pages.len(), 1);
    let p = &pages[0];
    assert_eq!([p.width, p.height], [100., 150.]);
    assert!(p.glyphs.is_empty() && p.fonts.is_empty());
    let mut expected = vec![PathItem {
        subpaths: vec![SubPath {
            points: vec![[0., 150.], [100., 150.], [100., 0.], [0., 0.]],
            closed: true,
            curved: false,
            rect: true,
        }],
        stroke: false,
        fill: true,
        line_width: 1.,
        clip: None,
    }];
    if suffix {
        expected.push(line(50., 110., 40., 2.));
    }
    assert_eq!(
        val(&p.paths),
        val(&expected),
        "image samples must not create public Paths"
    );
    let rules = page_rules(p);
    let expected_rules = if suffix {
        PageRules {
            vertical: vec![VRule {
                x: 50.,
                y0: 40.,
                y1: 110.,
            }],
            ..PageRules::default()
        }
    } else {
        PageRules::default()
    };
    assert_eq!(val(&rules), val(&expected_rules));
    owned_matches(bytes, &pages);
}
#[test]
fn actual_raw_payload_fake_ei_is_not_a_public_path() {
    rooted(
        include_bytes!("inline-image-fixtures/inline_raw_fake_EI_phantom.pdf"),
        false,
    );
}
#[test]
fn actual_true_suffix_public_path_and_rules_are_preserved() {
    rooted(
        include_bytes!("inline-image-fixtures/inline_raw_true_suffix_control.pdf"),
        true,
    );
}
fn content(header: &str, delimiter: &[u8], payload: &[u8], ending: &[u8], suffix: bool) -> Vec<u8> {
    let mut c = format!("BI {header} ID").into_bytes();
    c.extend_from_slice(delimiter);
    c.extend_from_slice(payload);
    c.extend_from_slice(ending);
    if suffix {
        c.extend_from_slice(b"\n0 G 1 w 80 20 m 80 180 l S\n");
    }
    c
}
fn correct(c: &[u8], suffix: bool) {
    let bytes = pdf(c, "");
    let pages = extract_pages(&bytes).unwrap();
    let paths = if suffix {
        vec![line(80., 180., 20., 1.)]
    } else {
        vec![]
    };
    assert_eq!(val(&pages[0].paths), val(&paths));
    let rules = if suffix {
        PageRules {
            vertical: vec![VRule {
                x: 80.,
                y0: 20.,
                y1: 180.,
            }],
            ..PageRules::default()
        }
    } else {
        PageRules::default()
    };
    assert_eq!(val(&page_rules(&pages[0])), val(&rules));
    owned_matches(&bytes, &pages);
}
fn evil(len: usize, at: usize) -> Vec<u8> {
    let mut p = vec![0x91; len];
    let fake = b" EI 0 G 3 w 31 20 m 31 70 l S %";
    p[at..at + fake.len()].copy_from_slice(fake);
    p
}
#[test]
fn fake_delimiters_dimensions_offsets_and_suffixes_are_data() {
    for len in [96, 187, 381] {
        for at in [1, 13, 41] {
            for suffix in [false, true] {
                correct(
                    &content(
                        &format!("/W {len} /H 1 /BPC 8 /CS /G"),
                        b"\n",
                        &evil(len, at),
                        b"\nEI\n",
                        suffix,
                    ),
                    suffix,
                );
            }
        }
    }
}
#[test]
fn device_components_bit_depth_and_row_padding() {
    for (header, len) in [
        ("/W 19 /H 9 /BPC 2 /CS /G", 45),
        (
            "/Width 17 /Height 7 /BitsPerComponent 1 /ColorSpace /DeviceRGB",
            49,
        ),
        ("/W 5 /H 2 /BPC 16 /CS /RGB", 60),
        ("/W 9 /H 3 /BPC 4 /CS /CMYK", 54),
        ("/W 7 /H 4 /BPC 8 /CS /DeviceRGB", 84),
        ("/W 7 /H 4 /BPC 8 /CS /DeviceCMYK", 112),
    ] {
        correct(&content(header, b" ", &evil(len, 1), b"\nEI\n", true), true);
    }
}
#[test]
fn exactly_one_id_separator_retains_initial_sample_whitespace() {
    for delimiter in [b" ".as_slice(), b"\n", b"\r", b"\r\n", b"\t", b"\0"] {
        for sample in [0, b' ', b'\n', b'\r', b'\t', b'\x0c'] {
            let mut payload = evil(96, 16);
            payload[0] = sample;
            correct(
                &content(
                    "/W 96 /H 1 /BPC 8 /CS /G",
                    delimiter,
                    &payload,
                    b"\r\nEI\n",
                    true,
                ),
                true,
            );
        }
    }
}
#[test]
fn all_single_byte_id_delimiters_and_crlf_cross_pixel_and_ei_whitespace() {
    // Six PDF whitespace bytes, plus the two-byte EOL compatibility form.
    for delimiter in [
        b" ".as_slice(),
        b"\n",
        b"\r",
        b"\r\n",
        b"\t",
        b"\0",
        b"\x0c",
    ] {
        for sample in [0, b' ', b'\n', b'\r', b'\t', b'\x0c'] {
            for gap in [
                b" ".as_slice(),
                b"\n",
                b"\r",
                b"\t",
                b"\0",
                b"\x0c",
                b"  ",
                b"\n\n",
                b"\r\n",
                b"   ",
                b"\n \t",
                b"\r\n ",
            ] {
                let mut payload = evil(96, 16);
                payload[0] = sample;
                let mut ending = gap.to_vec();
                ending.extend_from_slice(b"EI\n");
                correct(
                    &content(
                        "/W 96 /H 1 /BPC 8 /CS /G",
                        delimiter,
                        &payload,
                        &ending,
                        true,
                    ),
                    true,
                );
            }
        }
    }
}
fn actual_cr_fixture(bytes: &[u8], suffix: bool) {
    let mut payload = evil(96, 16);
    payload[0] = b'\n';
    let c = content(
        "/W 96 /H 1 /BPC 8 /CS /G",
        b"\r",
        &payload,
        b"\nEI\n",
        suffix,
    );
    assert_eq!(bytes, pdf(&c, ""));
    correct(&c, suffix);
}
#[test]
fn actual_cr_delimiter_lf_sample_phantom_matches_the_matrix_input() {
    actual_cr_fixture(
        include_bytes!("inline-image-fixtures/single_cr_lf_pixel_phantom.pdf"),
        false,
    );
}
#[test]
fn actual_cr_delimiter_lf_sample_true_suffix_matches_the_matrix_input() {
    actual_cr_fixture(
        include_bytes!("inline-image-fixtures/single_cr_lf_pixel_true_suffix.pdf"),
        true,
    );
}
#[test]
fn nondelimiter_ei_samples_and_real_ei_at_eof() {
    for sample in [b"aEI a".as_slice(), b" EIx ", b"\nEI/Name", b"abcS q Q"] {
        let mut payload = vec![0x91; 79];
        payload[8..8 + sample.len()].copy_from_slice(sample);
        correct(
            &content("/W 79 /H 1 /BPC 8 /CS /G", b" ", &payload, b"\nEI", false),
            false,
        );
    }
}
#[test]
fn harmless_supported_options_and_header_order() {
    correct(
        &content(
            "/H 1 /Interpolate true /CS /DeviceGray /BPC 8 /IM false /Width 96",
            b" ",
            &evil(96, 1),
            b"\nEI\n",
            true,
        ),
        true,
    );
}
fn unchanged(c: &[u8], resources: &str) {
    use sha2::Digest;
    let bytes = pdf(c, resources);
    let pages = extract_pages(&bytes).unwrap();
    let key = format!("{:x}", sha2::Sha256::digest(&bytes));
    // Captured once from immutable accepted55, never regenerated by this test.
    let golden: serde_json::Value = serde_json::from_str(include_str!(
        "inline-image-fixtures/legacy-unsupported.json"
    ))
    .unwrap();
    let expected = golden.get(&key).expect("missing immutable legacy fixture");
    assert_eq!(
        val(&pages),
        expected["pages"],
        "unsupported raw form must keep legacy extraction"
    );
    assert_eq!(
        bbox::bbox_layout(&pages),
        expected["bbox"].as_str().unwrap()
    );
    assert_eq!(val(&page_rules(&pages[0])), expected["rules"]);
    owned_matches(&bytes, &pages);
}

#[test]
fn unsupported_filter_resource_color_mask_and_decode_keep_legacy() {
    for extra in [
        "/F /AHx",
        "/Filter /FlateDecode",
        "/DP << >>",
        "/CS /Indexed",
        "/CS /Unknown",
        "/IM true",
        "/Intent /AbsoluteColorimetric",
        "/D [1 0]",
    ] {
        unchanged(
            &content(
                &format!("/W 96 /H 1 /BPC 8 /CS /G {extra}"),
                b" ",
                &evil(96, 1),
                b"\nEI\n",
                true,
            ),
            "",
        );
    }
}
#[test]
fn duplicate_aliases_and_bad_dimensions_keep_legacy() {
    for extra in [
        "/Width 96",
        "/W 9",
        "/Height 1",
        "/BitsPerComponent 8",
        "/ColorSpace /G",
    ] {
        unchanged(
            &content(
                &format!("/W 96 /H 1 /BPC 8 /CS /G {extra}"),
                b" ",
                &evil(96, 1),
                b"\nEI\n",
                true,
            ),
            "",
        );
    }
    for header in [
        "/W 0 /H 1 /BPC 8 /CS /G",
        "/W -4 /H 1 /BPC 8 /CS /G",
        "/W 96.0 /H 1 /BPC 8 /CS /G",
        "/W 9223372036854775807 /H 2 /BPC 16 /CS /CMYK",
        "/W 96 /H 1 /BPC 3 /CS /G",
        "/W 96 /BPC 8 /CS /G",
    ] {
        unchanged(&content(header, b" ", &evil(96, 1), b"\nEI\n", true), "");
    }
}
#[test]
fn truncated_payload_or_unproven_boundary_keeps_legacy() {
    unchanged(
        &content(
            "/W 400 /H 1 /BPC 8 /CS /G",
            b" ",
            &evil(96, 1),
            b"\nEI\n",
            true,
        ),
        "",
    );
    for ending in [
        b"EI\n".as_slice(),
        b"\nEIx\n",
        b"\nEI/Unknown\n",
        b"\n%comment\nEI\n",
    ] {
        unchanged(
            &content("/W 96 /H 1 /BPC 8 /CS /G", b" ", &evil(96, 1), ending, true),
            "",
        );
    }
}
#[test]
fn default_device_color_replacements_keep_legacy() {
    for r in [
        "/ColorSpace << /DefaultGray /DeviceRGB >>",
        "/ColorSpace << /DefaultRGB /DeviceGray >>",
        "/ColorSpace << /DefaultCMYK /DeviceGray >>",
        "/ColorSpace 99",
    ] {
        unchanged(
            &content(
                "/W 96 /H 1 /BPC 8 /CS /G",
                b" ",
                &evil(96, 1),
                b"\nEI\n",
                true,
            ),
            r,
        );
    }
}
#[test]
fn unexpected_bi_operands_keep_legacy() {
    let mut c = b"17 ".to_vec();
    c.extend(content(
        "/W 96 /H 1 /BPC 8 /CS /G",
        b" ",
        &evil(96, 1),
        b"\nEI\n",
        true,
    ));
    unchanged(&c, "");
}
#[test]
fn named_resource_color_space_keeps_legacy_without_alias_duplication() {
    unchanged(
        &content(
            "/W 96 /H 1 /BPC 8 /CS /Custom",
            b" ",
            &evil(96, 1),
            b"\nEI\n",
            true,
        ),
        "/ColorSpace << /Custom /DeviceGray >>",
    );
}
#[test]
fn successive_images_resume_at_each_true_suffix() {
    let mut c = content(
        "/W 96 /H 1 /BPC 8 /CS /G",
        b" ",
        &evil(96, 1),
        b"\nEI\n",
        false,
    );
    c.extend(content(
        "/W 84 /H 1 /BPC 8 /CS /RGB",
        b"\r\n",
        &evil(252, 13),
        b"\nEI\n",
        true,
    ));
    correct(&c, true);
}
