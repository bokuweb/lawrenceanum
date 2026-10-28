use super::*;
struct Fixture {
    page: crate::trace::PageTrace,
    units: Vec<Unit>,
    html: String,
    rules: PageRules,
    grids: Vec<Grid>,
}
fn drop_attr(html: &str, key: &str) -> String {
    let mut s = html.to_string();
    let needle = format!(" {key}=\"");
    let at = s.find(&needle).unwrap();
    let a = at + needle.len();
    let z = a + s[a..].find('"').unwrap() + 1;
    s.replace_range(at..z, "");
    s
}
fn marker_attr(html: &str, key: &str) -> String {
    let a = html.find("data-native-outline-rectangle=").unwrap();
    let first = html[..a].rfind("<word ").unwrap();
    let after = a + html[a..].find("</word>").unwrap() + 7;
    format!(
        "{}{}{}",
        &html[..first],
        drop_attr(&html[first..after], key),
        &html[after..]
    )
}
fn fixture() -> Fixture {
    static BASE: std::sync::OnceLock<(crate::trace::PageTrace, String, PageRules)> =
        std::sync::OnceLock::new();
    let (p, h, r) = BASE.get_or_init(|| {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/archive/pages/20260807g00178/20260807g001780121.pdf");
        let i = crate::page_inputs(&std::fs::read(path).unwrap())
            .unwrap()
            .remove(0);
        let h = i.xhtml.clone();
        let r = i.rules.clone().unwrap();
        let mut plain = i;
        plain.xhtml = drop_attr(&plain.xhtml, "data-native-scene");
        let p = crate::analyze_item(&[plain]).pages.remove(0);
        assert_eq!(p.words[0].status, WordStatus::Empty);
        (p, h, r)
    });
    let units = p
        .regions
        .iter()
        .map(|r| Unit::Plain(r.lines.clone()))
        .collect();
    Fixture {
        page: p.clone(),
        units,
        html: h.clone(),
        rules: r.clone(),
        grids: crate::grid::find_grids(r, p.width.max(p.height), p.width.max(p.height)),
    }
}
fn apply(f: &mut Fixture) {
    let d = scraper::Html::parse_document(&f.html);
    let p = d
        .select(&scraper::Selector::parse("page").unwrap())
        .next()
        .unwrap();
    let s = &mut f.page;
    restore(
        &mut f.units,
        &mut s.words,
        &s.native_bounds,
        &s.native_axes,
        &s.native_styles,
        &s.native_content_styles,
        &f.grids,
        p,
        &f.rules,
    );
}
fn snapshot(f: &Fixture) -> serde_json::Value {
    serde_json::json!([
        f.page.words,
        f.units
            .iter()
            .map(|u| match u {
                Unit::Plain(ls) => serde_json::to_value(ls).unwrap(),
                _ => panic!("unexpected source fixture role"),
            })
            .collect::<Vec<_>>()
    ])
}
fn unchanged(change: impl FnOnce(&mut Fixture)) {
    let mut f = fixture();
    change(&mut f);
    let before = snapshot(&f);
    apply(&mut f);
    assert!(
        snapshot(&f) == before,
        "rejected proof must retain every original Word and table field"
    );
}
fn table(f: &mut Fixture) -> &mut Table {
    f.units
        .iter_mut()
        .find_map(|u| match u {
            Unit::Plain(ls) => ls
                .iter_mut()
                .filter_map(|l| l.table.as_mut())
                .find(|t| t.x0 == 437.75 && t.y0 == 101.75),
            _ => None,
        })
        .unwrap()
}
#[test]
fn original_source_wrap_restores_only_its_unique_empty_amount_slot() {
    let mut f = fixture();
    apply(&mut f);
    assert_eq!(f.page.words[0].text, "－");
    assert_eq!(
        table(&mut f)
            .cells
            .iter()
            .filter(|c| c.row == 4 && c.col == 1)
            .count(),
        1
    );
}
#[test]
fn symbol_font_identity_is_independent_of_the_numeric_font() {
    for id in [7, 319, 4097] {
        let mut f = fixture();
        let style = f.page.native_styles[0].as_mut().unwrap();
        *style = style.iter().map(|p| (id, p.1)).collect();
        apply(&mut f);
        assert_eq!(f.page.words[0].text, "－");
    }
}
#[test]
fn an_existing_value_is_never_overwritten() {
    unchanged(|f| {
        let t = table(f);
        let mut c = t
            .cells
            .iter()
            .find(|c| c.row == 5 && c.col == 1)
            .unwrap()
            .clone();
        c.row = 4;
        c.text = "16".into();
        c.words.clear();
        t.cells.push(c);
    });
}
#[test]
fn an_already_owned_rectangle_is_not_claimed_again() {
    unchanged(|f| table(f).cells[0].words.push(0));
}
#[test]
fn a_missing_outline_does_not_fall_back_to_a_raw_character_value() {
    unchanged(|f| f.html = marker_attr(&f.html, "data-native-outline-rectangle"));
}
#[test]
fn a_missing_terminal_does_not_fall_back_to_the_display_box() {
    unchanged(|f| f.html = marker_attr(&f.html, "data-native-terminal-glyph"));
}
#[test]
fn incomplete_scene_proof_keeps_the_entire_original_output() {
    unchanged(|f| f.html = drop_attr(&f.html, "data-native-scene"));
}
#[test]
fn an_unknown_symbol_size_is_not_inferred_from_neighboring_numbers() {
    unchanged(|f| f.page.native_styles[0] = None);
}
#[test]
fn incompatible_native_numeric_sizes_do_not_supply_a_witness() {
    unchanged(|f| {
        for c in &mut f.page.native_content_styles {
            if let ContentStyles::Glyphs(s) = c {
                *s = s
                    .iter()
                    .map(|&(id, bits)| (id, (f64::from_bits(bits) * 1.5).to_bits()))
                    .collect();
            }
        }
    });
}
#[test]
fn a_changed_original_partition_boundary_does_not_receive_the_old_cell() {
    unchanged(|f| {
        let g = f
            .grids
            .iter_mut()
            .find(|g| g.left() == 437.75 && g.top() == 101.75)
            .unwrap();
        g.xs[1] = g.xs[1].next_up();
    });
}
#[test]
fn duplicate_empty_logical_slots_do_not_authorize_a_choice() {
    unchanged(|f| {
        let t = table(f);
        let mut c = t
            .cells
            .iter()
            .find(|c| c.row == 5 && c.col == 1)
            .unwrap()
            .clone();
        c.row = 4;
        c.text.clear();
        c.words.clear();
        t.cells.push(c.clone());
        t.cells.push(c);
    });
}
#[test]
fn a_malformed_native_grid_is_not_silently_partially_used() {
    unchanged(|f| {
        let g = f
            .grids
            .iter_mut()
            .find(|g| g.left() == 437.75 && g.top() == 101.75)
            .unwrap();
        g.cells.push(crate::grid::GridCell {
            x0: 0,
            x1: usize::MAX,
            y0: 0,
            y1: 1,
        });
    });
}
#[test]
fn normalized_label_wording_is_not_a_certificate_or_a_veto() {
    for (a, b) in [("別の科目、", "続き。"), ("１２３", "４５６"), ("α", "β")] {
        let mut f = fixture();
        f.page.words[179].text = a.into();
        f.page.words[180].text = b.into();
        table(&mut f)
            .cells
            .iter_mut()
            .find(|c| c.row == 4 && c.col == 0)
            .unwrap()
            .text = format!("{a}{b}");
        apply(&mut f);
        assert_eq!(f.page.words[0].text, "－");
    }
}
#[test]
fn a_valid_existing_empty_subcell_retains_its_own_bounds() {
    let mut f = fixture();
    let mark = f.page.words[0].source_box;
    let t = table(&mut f);
    let mut c = t
        .cells
        .iter()
        .find(|c| c.row == 5 && c.col == 1)
        .unwrap()
        .clone();
    c.row = 4;
    c.text.clear();
    c.words.clear();
    c.y0 = mark[1];
    c.y1 = mark[3];
    let geometry = [c.x0, c.y0, c.x1, c.y1];
    t.cells.push(c);
    apply(&mut f);
    assert_eq!(f.page.words[0].text, "－");
    let c = table(&mut f)
        .cells
        .iter()
        .find(|c| c.row == 4 && c.col == 1)
        .unwrap();
    assert_eq!([c.x0, c.y0, c.x1, c.y1], geometry);
}
#[test]
fn an_empty_subcell_must_belong_to_the_native_parent_and_contain_the_symbol() {
    for outside in [false, true] {
        unchanged(|f| {
            let mark = f.page.words[0].source_box;
            let t = table(f);
            let mut c = t
                .cells
                .iter()
                .find(|c| c.row == 5 && c.col == 1)
                .unwrap()
                .clone();
            c.row = 4;
            c.text.clear();
            c.words.clear();
            c.y0 = mark[1];
            c.y1 = mark[3];
            if outside {
                c.x0 -= 1.;
            } else {
                c.x1 = mark[0];
            }
            t.cells.push(c);
        });
    }
}
