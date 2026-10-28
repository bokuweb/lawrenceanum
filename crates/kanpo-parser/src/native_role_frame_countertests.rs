fn frame_ids(l: &Line) -> Vec<usize> {
    line_ids(std::slice::from_ref(l))
}
fn unlabelled_pages() -> (Vec<crate::PageInput>, Vec<PageTrace>) {
    let input = original_inputs();
    let mut pages = crate::analyze_item(&input).pages;
    let lines = pages[2]
        .regions
        .iter()
        .flat_map(|r| r.lines.clone())
        .collect();
    pages[2].regions = vec![Region {
        role: Role::Body,
        unit: 0,
        part: 0,
        lines,
    }];
    pages[2].native_comparison_labels.fill(None);
    (input, pages)
}
fn step(state: &mut frames::State, input: &crate::PageInput, page: &PageTrace) -> Option<Unit> {
    let dom = scraper::Html::parse_document(&input.xhtml);
    let node = dom
        .select(&scraper::Selector::parse("page").unwrap())
        .next()
        .unwrap();
    state.step(page, node, input.rules.as_ref().unwrap())
}
fn seeded(input: &[crate::PageInput], pages: &[PageTrace]) -> frames::State {
    let mut state = frames::State::default();
    for i in 0..2 {
        assert!(step(&mut state, &input[i], &pages[i]).is_none());
    }
    state
}
fn scene_edit(input: &mut crate::PageInput, change: impl FnOnce(&mut serde_json::Value)) {
    let dom = scraper::Html::parse_document(&input.xhtml);
    let node = dom
        .select(&scraper::Selector::parse("page").unwrap())
        .next()
        .unwrap();
    let mut packet: serde_json::Value =
        serde_json::from_str(node.value().attr("data-native-scene").unwrap()).unwrap();
    change(&mut packet);
    let encoded = serde_json::to_string(&packet)
        .unwrap()
        .replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;");
    let start = input.xhtml.find(" data-native-scene=\"").unwrap() + " data-native-scene=\"".len();
    let end = start + input.xhtml[start..].find('"').unwrap();
    input.xhtml.replace_range(start..end, &encoded);
}
#[test]
fn inferred_current_roles_cannot_create_printed_provenance() {
    let (input, pages) = unlabelled_pages();
    let mut state = frames::State::default();
    for i in 1..3 {
        assert!(step(&mut state, &input[i], &pages[i]).is_none());
    }
}
#[test]
fn missing_intermediate_source_scene_closes_scope_even_when_current_roles_look_correct() {
    let (mut input, pages) = unlabelled_pages();
    input[1].xhtml = input[1]
        .xhtml
        .replace(" data-native-scene=", " data-missing-scene=");
    let mut state = seeded(&input, &pages);
    assert!(step(&mut state, &input[2], &pages[2]).is_none());
}
#[test]
fn missing_current_scene_closes_scope_and_cannot_be_skipped() {
    let (input, pages) = unlabelled_pages();
    let mut bad = input[2].clone();
    bad.xhtml = bad
        .xhtml
        .replace(" data-native-scene=", " data-missing-scene=");
    let mut state = seeded(&input, &pages);
    assert!(step(&mut state, &bad, &pages[2]).is_none());
    assert!(step(&mut state, &input[2], &pages[2]).is_none());
}
#[test]
fn body_or_outside_interruption_consumes_the_previous_frame() {
    for outside in [false, true] {
        let (input, mut pages) = unlabelled_pages();
        let mut state = frames::State::default();
        assert!(step(&mut state, &input[0], &pages[0]).is_none());
        if outside {
            pages[1].regions[0].role = Role::Outside;
        } else {
            pages[1].regions[0].unit += 1;
        }
        assert!(step(&mut state, &input[1], &pages[1]).is_none());
        assert!(step(&mut state, &input[2], &pages[2]).is_none());
    }
}
#[test]
fn unbound_heading_text_cannot_restart_or_keep_a_matching_scope() {
    let (input, mut pages) = unlabelled_pages();
    let mut state = seeded(&input, &pages);
    pages[2].regions[0].lines[0].text = "〇新しい告示".into();
    assert!(step(&mut state, &input[2], &pages[2]).is_none());
    assert!(step(&mut state, &input[2], &pages[2]).is_none());
}
#[test]
fn missing_original_divider_is_not_replaced_by_legacy_grid_geometry() {
    let (mut input, pages) = unlabelled_pages();
    let mut state = seeded(&input, &pages);
    scene_edit(&mut input[2], |packet| {
        packet["lines"]
            .as_array_mut()
            .unwrap()
            .retain(|line| line[0][0].as_f64() != Some(425.) || line[0][2].as_f64() != Some(425.))
    });
    assert!(step(&mut state, &input[2], &pages[2]).is_none());
}
#[test]
fn two_distinct_original_dividers_between_the_root_fields_are_ambiguous() {
    let (mut input, pages) = unlabelled_pages();
    scene_edit(&mut input[0], |packet| {
        packet["lines"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::json!([
                [46., 426., 222.75, 426.],
                [45.5, 425.5, 223.25, 426.5]
            ]))
    });
    let mut state = seeded(&input, &pages);
    assert!(step(&mut state, &input[2], &pages[2]).is_none());
}
#[test]
fn raw_boxes_axes_native_bounds_status_and_ids_remain_bound_to_their_own_source() {
    for mutation in 0..6 {
        let (input, mut pages) = unlabelled_pages();
        let mut state = seeded(&input, &pages);
        let id = frame_ids(&pages[2].regions[0].lines[0])[0];
        match mutation {
            0 => pages[2].words[id].raw.push('x'),
            1 => pages[2].words[id].source_box[0] += 1.,
            2 => pages[2].native_axes[id] = NativeAxis::Unknown,
            3 => {
                let NativeBounds::Glyphs(ref mut b) = pages[2].native_bounds[id] else {
                    panic!()
                };
                b[0] += 1.;
            }
            4 => pages[2].words[id].status = WordStatus::Noise,
            _ => pages[2].words[id].id += 1,
        }
        assert!(
            step(&mut state, &input[2], &pages[2]).is_none(),
            "mutation {mutation}"
        );
        assert!(step(&mut state, &input[2], &pages[2]).is_none());
    }
}
#[test]
fn retained_line_and_cell_text_cannot_borrow_unrelated_original_occurrences() {
    for table in [false, true] {
        let (input, mut pages) = unlabelled_pages();
        let mut state = seeded(&input, &pages);
        if table {
            let l = pages[2].regions[0]
                .lines
                .iter_mut()
                .find(|l| l.table.is_some())
                .unwrap();
            let t = l.table.as_mut().unwrap();
            t.cells[0].text.push('x');
            l.text = t.render();
        } else {
            pages[2].regions[0].lines[0].text.push('x');
        }
        assert!(step(&mut state, &input[2], &pages[2]).is_none());
    }
}
#[test]
fn one_existing_line_cannot_mix_material_from_both_source_fields() {
    let (input, mut pages) = unlabelled_pages();
    let mut state = seeded(&input, &pages);
    let all = pages[2].regions[0]
        .lines
        .iter()
        .flat_map(frame_ids)
        .collect::<Vec<_>>();
    let text = all
        .iter()
        .map(|&id| pages[2].words[id].text.as_str())
        .collect();
    pages[2].regions[0].lines = vec![Line::text(text, all)];
    assert!(step(&mut state, &input[2], &pages[2]).is_none());
}
#[test]
fn a_missing_owned_line_or_duplicate_occurrence_cannot_leave_partial_proof() {
    for duplicate in [false, true] {
        let (input, mut pages) = unlabelled_pages();
        let mut state = seeded(&input, &pages);
        if duplicate {
            let l = pages[2].regions[0].lines[0].clone();
            pages[2].regions[0].lines.push(l);
        } else {
            pages[2].regions[0].lines.remove(0);
        }
        assert!(step(&mut state, &input[2], &pages[2]).is_none());
    }
}
#[test]
fn printed_source_labels_override_inferred_legacy_metadata() {
    let (input, mut pages) = unlabelled_pages();
    pages[0]
        .native_comparison_labels
        .fill(Some(crate::ComparisonLabels::CHANGE));
    let mut state = seeded(&input, &pages);
    let Some(Unit::Shinkyu { labels, .. }) = step(&mut state, &input[2], &pages[2]) else {
        panic!("source frame")
    };
    assert_eq!(labels, Some(crate::ComparisonLabels::AMENDMENT));
}

#[test]
fn missing_original_page_extents_cannot_borrow_display_defaults() {
    for key in ["width", "height"] {
        let (input, pages) = unlabelled_pages();
        let mut state = seeded(&input, &pages);
        let mut bad = input[2].clone();
        bad.xhtml = bad
            .xhtml
            .replacen(&format!(" {key}="), &format!(" data-missing-{key}="), 1);
        assert!(
            step(&mut state, &bad, &pages[2]).is_none(),
            "missing original {key}"
        );
        assert!(
            step(&mut state, &input[2], &pages[2]).is_none(),
            "an unproved page must close scope"
        );
    }
}
