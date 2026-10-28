fn mixed_field_original_inputs() -> Vec<crate::PageInput> {
    let mut out = vec![];
    for bytes in [
        include_bytes!("../tests/data/native-role-prefix/20261015g002260018.pdf").as_slice(),
        include_bytes!("../tests/data/native-role-prefix/20261015g002260019.pdf").as_slice(),
        include_bytes!("../tests/data/native-role-prefix/20261015g002260020.pdf").as_slice(),
    ] {
        out.extend(crate::page_inputs(bytes).unwrap());
    }
    out
}
#[test]
fn original_mixed_footer_keeps_each_source_word_in_its_own_field() {
    let input = mixed_field_original_inputs();
    let a = crate::analyze_item(&input);
    let p = &a.pages[2];
    assert_eq!(
        p.regions
            .iter()
            .map(|r| (r.unit, r.role, r.lines.len()))
            .collect::<Vec<_>>(),
        vec![
            (0, Role::After, 2),
            (0, Role::Before, 2),
            (1, Role::Preamble, 7),
            (1, Role::After, 3),
            (1, Role::Before, 3)
        ]
    );
    assert_eq!(p.regions[0].lines[1].text, "（備考）　（略）");
    assert_eq!(p.regions[1].lines[1].text, "（備考）　（略）");
    assert_eq!(p.regions[0].lines[1].words, vec![124, 97]);
    assert_eq!(p.regions[1].lines[1].words, vec![56, 36]);
}

#[test]
fn exact_field_slices_reject_reordered_or_unowned_current_line_material() {
    let input = mixed_field_original_inputs();
    let mut pages = crate::analyze_item(&input[..2]).pages;
    pages.push(crate::analyze_item(&input[2..]).pages.remove(0));
    for case in 0..6 {
        let mut p = pages[2].clone();
        match case {
            0 => p.regions[0].lines[2].words.swap(0, 1),
            1 => p.regions[0].lines[2].words.swap(1, 2),
            2 => {
                p.regions[0].lines[2].text = p.regions[0].lines[2].text.replacen("備考", "考備", 1)
            }
            3 => p.words[124].text.insert(0, ' '),
            4 => p.regions[0].lines[2].words.clear(),
            _ => {
                p.regions[0].lines[2].words.pop();
            }
        }
        let mut state = frames::State::default();
        for i in 0..2 {
            prefix_observe(&mut state, &input[i], &pages[i], true);
        }
        assert!(
            prefix_observe(&mut state, &input[2], &p, false).is_none(),
            "case{case}"
        );
    }
}
#[test]
fn exact_source_word_slices_preserve_the_page_and_both_physical_tables() {
    let input = mixed_field_original_inputs();
    let old = crate::analyze_item(&input[2..]).pages.remove(0);
    let new = crate::analyze_item(&input).pages.remove(2);
    assert_eq!(
        serde_json::to_value(&old.words).unwrap(),
        serde_json::to_value(&new.words).unwrap()
    );
    assert_eq!(new.regions[0].lines[0], old.regions[0].lines[0]);
    assert_eq!(new.regions[1].lines[0], old.regions[0].lines[1]);
    assert_eq!(new.regions[2].lines, old.regions[0].lines[3..]);
    assert_eq!(new.regions[3].lines, old.regions[1].lines);
    assert_eq!(new.regions[4].lines, old.regions[2].lines);
    let mut own = new
        .regions
        .iter()
        .flat_map(|r| &r.lines)
        .flat_map(frame_ids)
        .collect::<Vec<_>>();
    own.sort_unstable();
    assert!(own.windows(2).all(|w| w[0] != w[1]));
}
