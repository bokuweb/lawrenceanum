use super::*;
fn input(name: &str) -> crate::PageInput {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let path = if name == "702" {
        root.join("tests/data/native-table-scopes/20260724g001650702.pdf")
    } else {
        root.join("tests/data/native-lineage160")
            .join(format!("{name}160.pdf"))
    };
    crate::page_inputs(&std::fs::read(path).unwrap())
        .unwrap()
        .remove(0)
}
fn analysis(name: &str) -> crate::Analysis {
    crate::analyze_item(&[input(name)])
}
fn cells(a: &crate::Analysis) -> Vec<&crate::trace::TableCell> {
    a.pages
        .iter()
        .flat_map(|p| &p.regions)
        .flat_map(|r| &r.lines)
        .filter_map(|l| l.table.as_ref())
        .flat_map(|t| &t.cells)
        .collect()
}
fn material(t: &str) -> String {
    t.chars().filter(|c| !c.is_whitespace()).collect()
}
#[test]
fn actual_source_left_header_wrap_stays_with_its_own_label() {
    let a = analysis("702");
    let c = cells(&a)
        .into_iter()
        .filter(|c| c.text.contains("市及び福祉事務所"))
        .collect::<Vec<_>>();
    assert_eq!(c.len(), 2);
    assert!(
        material(&c[0].text).contains("市及び福祉事務所設置町村"),
        "{}",
        c[0].text
    );
}
#[test]
fn actual_source_right_Tstar_header_wrap_has_the_same_general_rule() {
    let a = analysis("702");
    let c = cells(&a)
        .into_iter()
        .filter(|c| c.text.contains("市及び福祉事務所"))
        .collect::<Vec<_>>();
    assert!(
        material(&c[1].text).contains("市及び福祉事務所設置町村"),
        "{}",
        c[1].text
    );
}
#[test]
fn ordinary_ASCII_source_flow_is_not_a_Japanese_length_or_vocabulary_rule() {
    let a = analysis("explicit-TD");
    let c = cells(&a)
        .into_iter()
        .find(|c| c.text.contains("alpha"))
        .unwrap();
    assert_eq!(material(&c.text), "alphaomegaside");
    let texts = c
        .words
        .iter()
        .map(|&i| a.pages[0].words[i].text.as_str())
        .collect::<Vec<_>>();
    assert_eq!(texts, vec!["alpha", "omega", "side"]);
}
#[test]
fn complete_and_empty_amount_rows_keep_the_entire_previous_Analysis() {
    for name in ["aligned-full", "aligned-empty-second", "independent-Tm"] {
        let a = analysis(name);
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let gold: serde_json::Value = serde_json::from_slice(
            &std::fs::read(
                root.join("tests/data/native-lineage160")
                    .join(format!("{name}-Analysis158.json")),
            )
            .unwrap(),
        )
        .unwrap();
        let actual: serde_json::Value =
            serde_json::from_str(&serde_json::to_string(&a).unwrap()).unwrap();
        assert_eq!(actual, gold, "{name}");
    }
}
fn without_coverage(s: &str) -> String {
    let key = " data-native-lineage-coverage=\"";
    if let Some(start) = s.find(key) {
        let after = start + key.len();
        let end = after + s[after..].find('"').unwrap() + 1;
        return format!("{}{}", &s[..start], &s[end..]);
    }
    s.into()
}
#[test]
fn stale_member_text_cannot_borrow_an_original_move() {
    let mut p = input("explicit-TD");
    p.xhtml = p.xhtml.replace(">alpha<", ">other<");
    let mut legacy = p.clone();
    legacy.xhtml = without_coverage(&legacy.xhtml);
    assert_eq!(
        serde_json::to_string(&crate::analyze_item(&[p])).unwrap(),
        serde_json::to_string(&crate::analyze_item(&[legacy])).unwrap()
    );
}
#[test]
fn missing_source_member_never_invents_the_missing_text() {
    let mut p = input("explicit-TD");
    let at = p.xhtml.find(">omega</word>").unwrap();
    let start = p.xhtml[..at].rfind("<word ").unwrap();
    let end = at + ">omega</word>".len();
    p.xhtml.replace_range(start..end, "");
    let mut legacy = p.clone();
    legacy.xhtml = without_coverage(&legacy.xhtml);
    assert_eq!(
        serde_json::to_string(&crate::analyze_item(&[p])).unwrap(),
        serde_json::to_string(&crate::analyze_item(&[legacy])).unwrap()
    );
}

fn same_as_legacy(p: crate::PageInput) {
    let mut old = p.clone();
    old.xhtml = without_coverage(&old.xhtml);
    assert_eq!(
        serde_json::to_string(&crate::analyze_item(&[p])).unwrap(),
        serde_json::to_string(&crate::analyze_item(&[old])).unwrap()
    );
}
fn delete_word(p: &mut crate::PageInput, text: &str) {
    let tag = format!(">{text}</word>");
    let at = p.xhtml.find(&tag).unwrap();
    let start = p.xhtml[..at].rfind("<word ").unwrap();
    p.xhtml.replace_range(start..at + tag.len(), "");
}
#[test]
fn aligned_partner_with_an_interleaving_label_preserves_the_entire_previous_analysis() {
    let a = analysis("aligned-middle");
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let gold: serde_json::Value = serde_json::from_slice(
        &std::fs::read(root.join("tests/data/native-lineage160/aligned-middle-Analysis158.json"))
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&serde_json::to_string(&a).unwrap()).unwrap(),
        gold
    );
}
#[test]
fn deleted_aligned_amount_cannot_turn_a_column_into_a_paragraph() {
    let mut p = input("aligned-middle");
    delete_word(&mut p, "10");
    same_as_legacy(p);
}
#[test]
fn arbitrary_source_line_chain_is_not_limited_to_two_lines() {
    let a = analysis("three-lines");
    let c = cells(&a)
        .into_iter()
        .find(|c| c.text.contains("alpha"))
        .unwrap();
    assert_eq!(material(&c.text), "alphaomegathirdside");
}
#[test]
fn page_image_in_the_cell_declines_but_an_unrelated_margin_image_allows_the_proof() {
    same_as_legacy(input("image-inside"));
    let a = analysis("image-outside");
    let c = cells(&a)
        .into_iter()
        .find(|c| c.text.contains("alpha"))
        .unwrap();
    assert_eq!(material(&c.text), "alphaomegaside");
}
#[test]
fn unknown_content_and_missing_fonts_cannot_certify_absent_partners() {
    for k in [
        "unknown-restored-context",
        "missing-font-source",
        "image-unknown",
    ] {
        same_as_legacy(input(k));
    }
}
#[test]
fn duplicate_source_occurrence_declines_the_whole_inventory() {
    let mut p = input("explicit-TD");
    let at = p.xhtml.find(">omega</word>").unwrap();
    let start = p.xhtml[..at].rfind("<word ").unwrap();
    let copy = p.xhtml[start..at + ">omega</word>".len()].to_string();
    p.xhtml = p.xhtml.replace("</page>", &format!("{copy}</page>"));
    same_as_legacy(p);
}
#[test]
fn partial_line_move_metadata_is_not_a_complete_group() {
    let mut p = input("explicit-TD");
    let at = p.xhtml.find(">omega</word>").unwrap();
    let start = p.xhtml[..at].rfind(" data-native-line-moves=\"").unwrap();
    let end = start
        + " data-native-line-moves=\"".len()
        + p.xhtml[start + " data-native-line-moves=\"".len()..]
            .find('"')
            .unwrap()
        + 1;
    p.xhtml.replace_range(start..end, "");
    same_as_legacy(p);
}
fn detached_units(
    name: &str,
) -> (
    Scene,
    Vec<Unit>,
    Vec<Word>,
    Vec<NativeBounds>,
    Vec<Option<Origin>>,
) {
    let p = input(name);
    let h = scraper::Html::parse_document(&p.xhtml);
    let page = h
        .select(&scraper::Selector::parse("page").unwrap())
        .next()
        .unwrap();
    let scene = for_page(page).unwrap();
    let raw = page
        .select(&scraper::Selector::parse("word").unwrap())
        .collect::<Vec<_>>();
    let mut legacy = p.clone();
    legacy.xhtml = without_coverage(&legacy.xhtml);
    let a = crate::analyze_item(&[legacy]);
    let page = &a.pages[0];
    let origins = page
        .words
        .iter()
        .map(|w| {
            let r = raw
                .iter()
                .copied()
                .find(|e| {
                    e.text().collect::<String>() == w.raw
                        && ["xmin", "ymin", "xmax", "ymax"]
                            .map(|k| e.value().attr(k).unwrap().parse::<f32>().unwrap())
                            == w.source_box
                })
                .unwrap();
            crate::native_cell_origin::from_word(r, Some(scene.total))
        })
        .collect();
    (
        scene,
        vec![Unit::Plain(
            page.regions.iter().flat_map(|r| r.lines.clone()).collect(),
        )],
        page.words.clone(),
        page.native_bounds.clone(),
        origins,
    )
}
fn wire(units: &[Unit]) -> String {
    let mut out = vec![];
    visit(units, |l| {
        out.extend(l.iter().map(|l| serde_json::to_string(l).unwrap()))
    });
    out.join("\n")
}
#[test]
fn an_aligned_partner_owned_by_an_overlapping_other_cell_still_blocks_the_move() {
    for other_owner in [false, true] {
        let (scene, mut units, words, bounds, origins) = detached_units("aligned-middle");
        let amount = words.iter().position(|w| w.text == "10").unwrap();
        let Unit::Plain(lines) = &mut units[0] else {
            panic!()
        };
        let table = lines.iter_mut().find_map(|l| l.table.as_mut()).unwrap();
        let target = table
            .cells
            .iter_mut()
            .find(|c| c.text.contains("alpha"))
            .unwrap();
        target.words.retain(|&i| i != amount);
        target.text = target.text.replace("　10", "");
        let mut foreign = target.clone();
        foreign.words = vec![amount];
        foreign.text = "10".into();
        if other_owner {
            table.cells.push(foreign)
        }
        for l in lines.iter_mut() {
            if let Some(t) = &l.table {
                l.text = t.render()
            }
        }
        let before = wire(&units);
        restore(&mut units, &words, &bounds, &origins, Some(&scene));
        assert_eq!(wire(&units), before, "foreign owner present {other_owner}");
    }
}
#[test]
fn detached_source_binding_really_applies_a_complete_positive_group() {
    let (scene, mut units, words, bounds, origins) = detached_units("explicit-TD");
    let before = wire(&units);
    restore(&mut units, &words, &bounds, &origins, Some(&scene));
    assert_ne!(wire(&units), before);
    let Unit::Plain(lines) = &units[0] else {
        panic!()
    };
    let c = lines
        .iter()
        .filter_map(|l| l.table.as_ref())
        .flat_map(|t| &t.cells)
        .find(|c| c.text.contains("alpha"))
        .unwrap();
    assert_eq!(material(&c.text), "alphaomegaside");
}
#[test]
fn uniform_background_needs_no_ocr_but_a_mask_never_borrows_its_uniformity() {
    let a = analysis("uniform-background");
    let c = cells(&a)
        .into_iter()
        .find(|c| c.text.contains("alpha"))
        .unwrap();
    assert_eq!(material(&c.text), "alphaomegaside");
    for k in [
        "uniform",
        "masked-uniform",
        "truncated-uniform",
        "uniform-overpaint",
        "uniform-clipped-background",
    ] {
        same_as_legacy(input(k));
    }
}
#[test]
fn malformed_move_and_coverage_attributes_decline_without_changing_legacy_analysis() {
    for transform in [0, 1, 2, 3, 4, 5, 6, 7] {
        let mut p = input("explicit-TD");
        match transform {
            0 => {
                p.xhtml = p
                    .xhtml
                    .replace("data-native-glyph-count=\"", "data-native-glyph-count=\"9");
            }
            1 => {
                p.xhtml = p.xhtml.replace("0:5:10", "0:0:10");
            }
            2 => {
                p.xhtml = p.xhtml.replace("616c706861", "626c706861");
            }
            3 => {
                let k = " data-native-lineage-obstacles=\"";
                let start = p.xhtml.find(k).unwrap();
                let end = start + k.len() + p.xhtml[start + k.len()..].find('"').unwrap() + 1;
                p.xhtml.replace_range(start..end, "");
            }
            4 => {
                p.xhtml = p.xhtml.replace(
                    "data-native-flow-first-glyph=\"0\"",
                    "data-native-flow-first-glyph=\"1\"",
                );
            }
            5 => {
                p.xhtml = p.xhtml.replace(
                    "data-native-lineage-coverage=\"1 ",
                    "data-native-lineage-coverage=\"2 ",
                );
            }
            6 => {
                let k = " data-native-lineage-ink=\"";
                let start = p.xhtml.find(k).unwrap();
                let end = start + k.len() + p.xhtml[start + k.len()..].find('"').unwrap() + 1;
                p.xhtml.replace_range(start..end, "");
            }
            _ => {
                p.xhtml = p.xhtml.replace(
                    "data-native-lineage-ink=\"1 ",
                    "data-native-lineage-ink=\"9 ",
                );
            }
        }
        same_as_legacy(p);
    }
}

#[test]
fn original_school_fee_numeric_rows_keep_complete_previous_analysis() {
    let a = analysis("source-663");
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let gold: serde_json::Value = serde_json::from_slice(
        &std::fs::read(root.join("tests/data/native-lineage160/source-663-Analysis158.json"))
            .unwrap(),
    )
    .unwrap();
    let actual: serde_json::Value =
        serde_json::from_str(&serde_json::to_string(&a).unwrap()).unwrap();
    assert_eq!(actual, gold);
}
#[test]
fn shared_glyph_row_does_not_require_identical_numeric_baselines() {
    let a = analysis("aligned-offset");
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let gold: serde_json::Value = serde_json::from_slice(
        &std::fs::read(root.join("tests/data/native-lineage160/aligned-offset-Analysis158.json"))
            .unwrap(),
    )
    .unwrap();
    let actual: serde_json::Value =
        serde_json::from_str(&serde_json::to_string(&a).unwrap()).unwrap();
    assert_eq!(actual, gold);
}
#[test]
fn a_normalized_empty_source_mark_still_blocks_a_missing_row_partner() {
    let (scene, mut units, mut words, bounds, origins) = detached_units("aligned-middle");
    let amount = words.iter().position(|w| w.text == "10").unwrap();
    words[amount].text.clear();
    words[amount].status = crate::trace::WordStatus::Empty;
    let Unit::Plain(lines) = &mut units[0] else {
        panic!()
    };
    let table = lines.iter_mut().find_map(|l| l.table.as_mut()).unwrap();
    let target = table
        .cells
        .iter_mut()
        .find(|c| c.text.contains("alpha"))
        .unwrap();
    target.words.retain(|&i| i != amount);
    target.text = target.text.replace("　10", "");
    for l in lines.iter_mut() {
        if let Some(t) = &l.table {
            l.text = t.render()
        }
    }
    let before = wire(&units);
    restore(&mut units, &words, &bounds, &origins, Some(&scene));
    assert_eq!(
        wire(&units),
        before,
        "normalization to an empty string does not prove the original glyph was not a row value"
    );
}

#[test]
fn a_source_vector_row_mark_cannot_be_inferred_absent_from_glyph_coverage() {
    for k in ["vector-row-dash", "vector-filled-row-mark"] {
        let a = analysis(k);
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let gold: serde_json::Value = serde_json::from_slice(
            &std::fs::read(
                root.join("tests/data/native-lineage160")
                    .join(format!("{k}-Analysis158.json")),
            )
            .unwrap(),
        )
        .unwrap();
        let actual: serde_json::Value =
            serde_json::from_str(&serde_json::to_string(&a).unwrap()).unwrap();
        assert_eq!(actual, gold, "{k}");
    }
}
