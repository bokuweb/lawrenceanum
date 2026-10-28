fn prefix_original_inputs() -> Vec<crate::PageInput> {
    let mut input = original_inputs();
    input.extend(
        crate::page_inputs(include_bytes!(
            "../tests/data/native-role-prefix/20260828g001920028.pdf"
        ))
        .unwrap(),
    );
    input
}
#[test]
fn original_old_comparison_prefix_closes_before_the_enactment_and_new_notice() {
    let input = prefix_original_inputs();
    let a = crate::analyze_item(&input);
    let p = &a.pages[3];
    assert_eq!(
        p.regions
            .iter()
            .map(|r| (r.unit, r.role, r.lines.len()))
            .collect::<Vec<_>>(),
        vec![
            (0, Role::After, 3),
            (0, Role::Before, 3),
            (1, Role::Preamble, 10),
            (1, Role::After, 6),
            (1, Role::Before, 6)
        ]
    );
}

fn prefix_source_pages() -> (Vec<crate::PageInput>, Vec<PageTrace>) {
    let input = prefix_original_inputs();
    let mut pages = crate::analyze_item(&input[..3]).pages;
    pages.push(crate::analyze_item(&input[3..]).pages.remove(0));
    (input, pages)
}
fn prefix_observe(
    state: &mut frames::State,
    input: &crate::PageInput,
    p: &PageTrace,
    has_next: bool,
) -> Option<Vec<Unit>> {
    let dom = scraper::Html::parse_document(&input.xhtml);
    let node = dom
        .select(&scraper::Selector::parse("page").unwrap())
        .next()
        .unwrap();
    state
        .observe(p, node, input.rules.as_ref().unwrap(), has_next)
        .units
}
fn prefix_seed(input: &[crate::PageInput], pages: &[PageTrace]) -> frames::State {
    let mut state = frames::State::default();
    for i in 0..3 {
        prefix_observe(&mut state, &input[i], &pages[i], true);
    }
    state
}
#[test]
fn prefix_needs_an_earlier_printed_root_and_cannot_skip_missing_source() {
    let (mut input, pages) = prefix_source_pages();
    assert!(prefix_observe(&mut frames::State::default(), &input[3], &pages[3], false).is_none());
    input[1].xhtml = input[1]
        .xhtml
        .replace(" data-native-scene=", " data-missing-scene=");
    let mut state = prefix_seed(&input, &pages);
    assert!(prefix_observe(&mut state, &input[3], &pages[3], false).is_none());
}
#[test]
fn prefix_keeps_every_original_line_cell_and_suffix_in_either_end_position() {
    let (input, pages) = prefix_source_pages();
    for next in [false, true] {
        let mut state = prefix_seed(&input, &pages);
        let units = prefix_observe(&mut state, &input[3], &pages[3], next).unwrap();
        let regions = units
            .into_iter()
            .enumerate()
            .flat_map(|(i, u)| u.regions(i))
            .collect::<Vec<_>>();
        let original = &pages[3].regions;
        assert_eq!(regions[0].lines, original[0].lines[..3]);
        assert_eq!(regions[1].lines, original[0].lines[3..6]);
        assert_eq!(regions[2].lines, original[0].lines[6..]);
        assert_eq!(regions[3].lines, original[1].lines);
        assert_eq!(regions[4].lines, original[2].lines);
    }
}
#[test]
fn missing_current_prefix_scene_consumes_the_old_scope() {
    let (input, pages) = prefix_source_pages();
    let mut bad = input[3].clone();
    bad.xhtml = bad
        .xhtml
        .replace(" data-native-scene=", " data-missing-scene=");
    let mut state = prefix_seed(&input, &pages);
    assert!(prefix_observe(&mut state, &bad, &pages[3], true).is_none());
    assert!(prefix_observe(&mut state, &input[3], &pages[3], false).is_none());
}
#[test]
fn prefix_requires_all_original_fences_and_will_not_bridge_a_material_gap() {
    let (input, pages) = prefix_source_pages();
    for gap in [false, true] {
        let mut bad = input[3].clone();
        scene_edit(&mut bad, |packet| {
            let lines = packet["lines"].as_array_mut().unwrap();
            if gap {
                // A real interval is removed; a paint hull must not replace it.
                for line in lines {
                    if line[0][0] == 425.0 && line[0][2] == 425.0 && line[0][1] == 46.0 {
                        line[0][3] = serde_json::json!(60.0);
                    }
                }
            } else {
                lines.retain(|l| l[0][0] != 425.0 || l[0][2] != 425.0);
            }
        });
        let mut state = prefix_seed(&input, &pages);
        assert!(prefix_observe(&mut state, &bad, &pages[3], false).is_none());
    }
}
#[test]
fn prefix_rejects_stale_material_duplicate_owners_and_noncanonical_regions() {
    let (input, pages) = prefix_source_pages();
    for case in 0..6 {
        let mut p = pages[3].clone();
        match case {
            0 => p.regions[0].lines[0].text.push('変'),
            1 => {
                let l = p.regions[0].lines[0].clone();
                p.regions[0].lines.insert(1, l);
            }
            2 => {
                p.regions[0].lines.remove(0);
            }
            3 => p.regions[0].part = 1,
            4 => p.regions[1].part = 1,
            _ => p.native_comparison_labels.clear(),
        }
        let mut state = prefix_seed(&input, &pages);
        assert!(
            prefix_observe(&mut state, &input[3], &p, false).is_none(),
            "case{case}"
        );
    }
}
#[test]
fn prefix_rejects_a_whole_line_that_contains_both_source_fields() {
    let (input, pages) = prefix_source_pages();
    let mut p = pages[3].clone();
    let other = p.regions[0].lines[3].clone();
    let first = &mut p.regions[0].lines[0];
    let mut words = frame_ids(first);
    words.extend(frame_ids(&other));
    first.table = None;
    first.words = words;
    first.text.push_str(&other.text);
    p.regions[0].lines.remove(3);
    let mut state = prefix_seed(&input, &pages);
    assert!(prefix_observe(&mut state, &input[3], &p, false).is_none());
}
#[test]
fn prefix_source_identity_is_bound_to_its_own_raw_box_axis_and_extent() {
    let (input, pages) = prefix_source_pages();
    let id = frame_ids(&pages[3].regions[0].lines[0])[0];
    for case in 0..5 {
        let mut p = pages[3].clone();
        let mut source = input[3].clone();
        match case {
            0 => p.words[id].raw.push('変'),
            1 => p.words[id].source_box[0] += 1.,
            2 => p.native_axes[id] = NativeAxis::Unknown,
            3 => p.native_bounds[id] = NativeBounds::Unknown,
            _ => {
                source.xhtml = source
                    .xhtml
                    .replace("width=\"595.000000\"", "data-missing-width=\"595.000000\"")
            }
        }
        let mut state = prefix_seed(&input, &pages);
        assert!(
            prefix_observe(&mut state, &source, &p, false).is_none(),
            "case{case}"
        );
    }
}

#[test]
fn a_line_spanning_the_old_frame_and_following_body_cannot_end_a_partial_prefix() {
    let (input, pages) = prefix_source_pages();
    let mut p = pages[3].clone();
    let next = p.regions[0].lines.remove(6);
    let line = &mut p.regions[0].lines[5];
    let mut words = frame_ids(line);
    words.extend(frame_ids(&next));
    line.table = None;
    line.words = words;
    line.text.push('\n');
    line.text.push_str(&next.text);
    let mut state = prefix_seed(&input, &pages);
    assert!(
        prefix_observe(&mut state, &input[3], &p, false).is_none(),
        "partial fields cannot hide unproved old material inside the following line"
    );
}

#[test]
fn a_missing_physical_end_cap_is_not_replaced_by_an_unsupported_following_line() {
    let (input, pages) = prefix_source_pages();
    let mut bad = input[3].clone();
    scene_edit(&mut bad, |packet| {
        packet["lines"]
            .as_array_mut()
            .unwrap()
            .retain(|l| l[0] != serde_json::json!([69.5, 329.605, 780.5, 329.605]));
    });
    let mut state = prefix_seed(&input, &pages);
    assert!(prefix_observe(&mut state, &bad, &pages[3], false).is_none());
}
