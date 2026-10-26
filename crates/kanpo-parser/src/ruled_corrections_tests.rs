use super::*;
use std::collections::BTreeSet;
#[test]
fn original_correction_section_has_its_own_article_and_all_source_cells() {
    let input = crate::page_inputs(include_bytes!(
        "../tests/archive/pages/20260731h01759/20260731h017590032.pdf"
    ))
    .unwrap();
    let a = crate::analyze_item(&input);
    let gold: serde_json::Value = serde_json::from_str(include_str!(
        "../tests/expected-ruled-corrections/20260731h017590032-gold.json"
    ))
    .unwrap();

    assert_eq!(
        a.articles.len(),
        14,
        "The printed correction section is independent"
    );
    let tables: Vec<_> = a.pages[0]
        .regions
        .iter()
        .flat_map(|r| &r.lines)
        .filter_map(|l| l.table.as_ref())
        .filter(|t| t.rows == 4 && t.cols == 5)
        .collect();
    assert_eq!(tables.len(), 1);
    let t = tables[0];
    assert_eq!(t.cells.len(), gold["cells"].as_array().unwrap().len());
    for (c, g) in t.cells.iter().zip(gold["cells"].as_array().unwrap()) {
        assert_eq!(
            (c.row, c.col, c.rowspan, c.colspan),
            (
                g["row"].as_u64().unwrap() as usize,
                g["col"].as_u64().unwrap() as usize,
                1,
                g["colspan"].as_u64().unwrap() as usize
            )
        );
        assert_eq!(c.text, g["text"].as_str().unwrap());
        assert_eq!(
            c.words,
            g["source_word_ids"]
                .as_array()
                .unwrap()
                .iter()
                .map(|n| n.as_u64().unwrap() as usize)
                .collect::<Vec<_>>()
        );
    }
    let text = crate::trace::join_page_texts(&a.pages, &|p| p.doc_text.as_str());
    let mut original = crate::segment_articles(&text);
    crate::wrapped_notice_titles::repair_wrapped_notice_titles(&mut original, &a.pages);
    let rules: Vec<_> = input.iter().map(|p| p.rules.clone()).collect();
    let parts = super::super::groups(&original, &text, &a.pages, &rules);
    let expected: BTreeSet<_> = gold["expected_source_group"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n.as_u64().unwrap() as usize)
        .collect();
    assert!(parts.iter().flatten().any(|p| p
        .sources
        .iter()
        .map(|(page, id)| {
            assert_eq!(*page, 0);
            *id
        })
        .collect::<BTreeSet<_>>()
        == expected));
}

use crate::trace::{Region, WordStatus};
struct Fixture {
    units: Vec<Unit>,
    words: Vec<Word>,
    styles: Vec<Option<NativeStyles>>,
    content: Vec<ContentStyles>,
    bounds: Vec<NativeBounds>,
    rules: PageRules,
}
impl Fixture {
    fn put(&mut self, text: &str, b: Rect, font: usize, size: f64) -> usize {
        let id = self.words.len();
        self.words.push(Word {
            source_box: [b[0], b[1], b[2], b[3]],
            id,
            raw: text.into(),
            text: text.into(),
            x0: b[0],
            y0: b[1],
            x1: b[2],
            y1: b[3],
            status: WordStatus::Used,
        });
        let st = NativeStyles::from([(font, size.to_bits())]);
        self.styles.push(Some(st.clone()));
        self.content.push(ContentStyles::Glyphs(
            if text.chars().any(char::is_alphanumeric) {
                st
            } else {
                NativeStyles::new()
            },
        ));
        self.bounds.push(NativeBounds::Glyphs(b));
        id
    }
    fn line(&mut self, text: &str, x: f32, y: f32, font: usize, size: f64, split: bool) {
        let ids = if split {
            text.chars()
                .enumerate()
                .map(|(i, c)| {
                    self.put(
                        &c.to_string(),
                        [
                            x,
                            y + i as f32 * size as f32,
                            x + size as f32,
                            y + (i + 1) as f32 * size as f32,
                        ],
                        font,
                        size,
                    )
                })
                .collect()
        } else {
            vec![self.put(
                text,
                [
                    x,
                    y,
                    x + size as f32,
                    y + text.chars().count() as f32 * size as f32,
                ],
                font,
                size,
            )]
        };
        if let Unit::Plain(ls) = &mut self.units[0] {
            ls.push(Line::text(text.into(), ids));
        }
    }
    fn run(&mut self) {
        self.run_previous(&[])
    }
    fn run_previous(&mut self, previous: &[PageTrace]) {
        restore_ruled_corrections(
            &mut self.units,
            &self.words,
            &self.styles,
            &self.content,
            &self.bounds,
            &self.rules,
            previous,
        )
    }
    fn snapshot(&self) -> serde_json::Value {
        serde_json::json!({"units":self.units.iter().map(|u|serde_json::json!({"text":u.text(),"lines":unit_lines(u)})).collect::<Vec<_>>(),"words":self.words})
    }
    fn table(&self) -> Option<&Table> {
        self.units
            .iter()
            .flat_map(unit_lines)
            .find_map(|l| l.table.as_ref())
    }
    fn page(&self) -> PageTrace {
        let mut tables = vec![];
        let doc = self
            .units
            .iter()
            .map(|u| u.doc_text(&mut tables, 0))
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("\n");
        PageTrace {native_comparison_labels:vec![],native_horizontal_baselines:vec![],native_atomic_columns:vec![],native_axes:vec![],native_rulings:vec![],
            width: 10000.,
            height: 10000.,
            layout: "tiers",
            words: self.words.clone(),
            native_styles: self.styles.clone(),
            native_bounds: self.bounds.clone(),
            native_content_styles: self.content.clone(),
            regions: self
                .units
                .iter()
                .enumerate()
                .map(|(i, u)| Region {
                    role: Role::Body,
                    unit: i,
                    part: 0,
                    lines: unit_lines(u).into_iter().cloned().collect(),
                })
                .collect(),
            text: self
                .units
                .iter()
                .map(Unit::text)
                .collect::<Vec<_>>()
                .join("\n"),
            doc_text: doc,
            tables,
        }
    }
}
fn fixture(records: usize, caption: &str, value: &str, split: bool) -> Fixture {
    let reference = format!("令和八年七月一日（任意の刊号）任意の規則{value}");
    let h = (caption.chars().count() * 12 + 100)
        .max(value.chars().count() * 60 + 200)
        .max(reference.chars().count() * 8 + 100)
        .max(700) as f32;
    let top = 100.;
    let bottom = top + h;
    let edges: Vec<_> = (0..=FIELDS.len())
        .map(|i| top + h * i as f32 / FIELDS.len() as f32)
        .collect();
    let data_right = records as f32 * 50. + 50.;
    let header_left = data_right + 60.;
    let header_right = header_left + 20.;
    let cap_left = header_right + 20.;
    let cap_right = cap_left + 20.;
    let mut f = Fixture {
        units: vec![Unit::Plain(vec![])],
        words: vec![],
        styles: vec![],
        content: vec![],
        bounds: vec![],
        rules: PageRules::default(),
    };
    f.rules.vertical = vec![
        crate::lines::VRule {
            x: cap_left,
            y0: top + 1.,
            y1: bottom - 1.,
        },
        crate::lines::VRule {
            x: cap_right,
            y0: top + 1.,
            y1: bottom - 1.,
        },
    ];
    for &y in &edges[1..edges.len() - 1] {
        for (x0, x1) in [(10., data_right), (header_left, header_right)] {
            f.rules.horizontal.push(crate::lines::HRule { y, x0, x1 });
        }
    }
    for y in [top, bottom] {
        f.rules.horizontal.push(crate::lines::HRule {
            y,
            x0: 10.,
            x1: cap_right + 50.,
        });
    }
    f.line(caption, cap_left + 5., top + 10., 7, 9., split);
    for (c, label) in FIELDS.iter().enumerate() {
        f.line(label, header_left + 5., edges[c] + 5., 13, 8., split);
    }
    f.line(&reference, header_left - 20., top + 5., 13, 8., split);
    for r in 0..records {
        let x = data_right - 18. - r as f32 * 50.;
        for (c, text) in [
            (0, (r + 1).to_string()),
            (2, "任意欄".into()),
            (3, format!("旧{value}")),
            (4, format!("新{value}")),
        ] {
            f.line(&text, x, edges[c] + 5., 13, 8., split);
        }
    }
    f
}
fn text_cells(t: &Table) -> Vec<(usize, usize, usize, String)> {
    t.cells
        .iter()
        .map(|c| (c.row, c.col, c.colspan, c.text.clone()))
        .collect()
}
#[test]
fn record_count_and_caption_citation_and_values_do_not_have_caps() {
    for records in [1, 2, 4, 17, 41] {
        let mut f = fixture(records, "任意の訂正文書", "本文", false);
        f.run();
        let t = f.table().expect("Proven source table");
        assert_eq!((t.rows, t.cols), (records + 2, FIELDS.len()));
    }
    for (caption, value) in [
        ("注".into(), "A".into()),
        ("任意の正式標題".repeat(40), "未知の固有値".repeat(30)),
    ] {
        let mut f = fixture(3, &caption, &value, false);
        f.run();
        assert_eq!(f.table().unwrap().rows, 5);
    }
}
#[test]
fn word_splitting_and_source_line_fragmentation_within_cells_preserve_the_table() {
    let mut a = fixture(4, "任意標題", "任意値", false);
    a.run();
    let want = text_cells(a.table().unwrap());
    let mut b = fixture(4, "任意標題", "任意値", true);
    if let Unit::Plain(ls) = &mut b.units[0] {
        let before = std::mem::take(ls);
        for l in before {
            for id in l.words {
                ls.push(Line::text(b.words[id].text.clone(), vec![id]));
            }
        }
    }
    b.run();
    assert_eq!(text_cells(b.table().unwrap()), want);
}
#[test]
fn independent_axis_scale_translation_and_font_id_bijections_preserve_cells() {
    let mut baseline = fixture(3, "任意標題", "任意値", false);
    baseline.run();
    let want = text_cells(baseline.table().unwrap());
    for (kx, ky, tx, ty) in [
        (0.125, 8., -100., -200.),
        (8., 0.125, 1000., 700.),
        (1., 1., -500., 10000.),
    ] {
        let mut f = fixture(3, "任意標題", "任意値", false);
        let tr = |b: Rect| {
            [
                b[0] * kx + tx,
                b[1] * ky + ty,
                b[2] * kx + tx,
                b[3] * ky + ty,
            ]
        };
        for w in &mut f.words {
            let b = tr([w.x0, w.y0, w.x1, w.y1]);
            [w.x0, w.y0, w.x1, w.y1] = b;
        }
        for b in &mut f.bounds {
            if let NativeBounds::Glyphs(r) = b {
                *r = tr(*r);
            }
        }
        for h in &mut f.rules.horizontal {
            h.x0 = h.x0 * kx + tx;
            h.x1 = h.x1 * kx + tx;
            h.y = h.y * ky + ty;
        }
        for v in &mut f.rules.vertical {
            v.x = v.x * kx + tx;
            v.y0 = v.y0 * ky + ty;
            v.y1 = v.y1 * ky + ty;
        }
        let transform = |s: &NativeStyles| {
            s.iter()
                .map(|&(id, bits)| (id + 901, (f64::from_bits(bits) * ky as f64).to_bits()))
                .collect()
        };
        for s in f.styles.iter_mut().flatten() {
            *s = transform(s);
        }
        for c in &mut f.content {
            if let ContentStyles::Glyphs(s) = c {
                *s = transform(s);
            }
        }
        f.run();
        assert_eq!(text_cells(f.table().unwrap()), want);
    }
}
#[test]
fn repeated_permuted_and_fragmented_native_strokes_have_the_same_meaning() {
    let mut baseline = fixture(3, "任意標題", "値", false);
    baseline.run();
    let want = text_cells(baseline.table().unwrap());
    for mode in ["duplicate", "fragment", "fragment-band-at-caption"] {
        let mut f = fixture(3, "任意標題", "値", false);
        if mode == "duplicate" {
            f.rules.horizontal.extend(f.rules.horizontal.clone());
            f.rules.vertical.extend(f.rules.vertical.clone());
        } else {
            let old = std::mem::take(&mut f.rules.horizontal);
            let caption_axis = mid(f.rules.vertical[0].x, f.rules.vertical[1].x);
            for h in old {
                let x = if mode == "fragment-band-at-caption"
                    && h.x0 <= caption_axis
                    && caption_axis < h.x1
                {
                    caption_axis
                } else {
                    mid(h.x0, h.x1)
                };
                f.rules.horizontal.push(crate::lines::HRule { x1: x, ..h });
                f.rules.horizontal.push(crate::lines::HRule { x0: x, ..h });
            }
        }
        f.rules.horizontal.reverse();
        f.rules.vertical.reverse();
        f.run();
        assert_eq!(text_cells(f.table().unwrap()), want, "{mode}");
    }
}
#[test]
fn all_words_native_arrays_characters_and_ownership_are_conserved_and_repair_is_idempotent() {
    let mut f = fixture(5, "任意標題", "任意値", true);
    let before = f.snapshot();
    let st = f.styles.clone();
    let bounds = f.bounds.clone();
    let content = f.content.clone();
    let words = serde_json::to_value(&f.words).unwrap();
    let before_text = f.units[0].text();
    f.run();
    assert!(f.table().is_some());
    assert_eq!(serde_json::to_value(&f.words).unwrap(), words);
    assert_eq!(f.styles, st);
    assert_eq!(f.bounds, bounds);
    assert_eq!(f.content, content);
    assert_eq!(chars(&before_text), chars(&f.units[0].text()));
    let ids: Vec<_> = f
        .units
        .iter()
        .flat_map(unit_lines)
        .flat_map(line_ids)
        .collect();
    assert_eq!(ids.len(), f.words.len());
    assert_eq!(
        ids.iter().copied().collect::<BTreeSet<_>>(),
        (0..f.words.len()).collect()
    );
    let after = f.snapshot();
    assert_ne!(before, after);
    f.run();
    assert_eq!(f.snapshot(), after);
    let p = f.page();
    let lines = source_lines(&p).unwrap();
    assert_eq!(section_starts(&p, &f.rules, &lines), vec![0]);
}
#[test]
fn incomplete_or_nonfinite_native_evidence_cannot_authorize_a_table() {
    for mode in [
        "bounds",
        "compatibility",
        "styles",
        "content",
        "content-compatibility",
        "style-NaN",
        "style-zero",
        "bound-NaN",
    ] {
        let mut f = fixture(2, "任意標題", "値", false);
        match mode {
            "bounds" => f.bounds[1] = NativeBounds::Unknown,
            "compatibility" => f.bounds[1] = NativeBounds::Compatibility,
            "styles" => f.styles[1] = None,
            "content" => f.content[1] = ContentStyles::Unknown,
            "content-compatibility" => f.content[1] = ContentStyles::Compatibility,
            "bound-NaN" => f.bounds[1] = NativeBounds::Glyphs([f32::NAN, 1., 2., 3.]),
            _ => {
                let bits = if mode == "style-NaN" {
                    f64::NAN.to_bits()
                } else {
                    0f64.to_bits()
                };
                let st = NativeStyles::from([(42, bits)]);
                f.styles[0] = Some(st.clone());
                f.content[0] = ContentStyles::Glyphs(st);
            }
        }
        let before = f.snapshot();
        f.run();
        assert_eq!(f.snapshot(), before, "{mode}");
    }
}
#[test]
fn field_crossings_and_overprinted_or_unproved_column_axes_are_not_guessed() {
    for mode in [
        "cross-field",
        "overprint",
        "axis-nearly-overprinted",
        "cross-caption",
    ] {
        let mut f = fixture(2, "任意標題", "値", false);
        let id = match mode {
            "cross-field" => f.words.iter().position(|w| w.text == "旧値").unwrap(),
            "cross-caption" => 0,
            _ => f.words.iter().position(|w| w.text == "新値").unwrap(),
        };
        let NativeBounds::Glyphs(mut b) = f.bounds[id] else {
            unreachable!()
        };
        if mode == "cross-field" {
            b[1] = f.rules.horizontal[4].y - 1.;
            b[3] = f.rules.horizontal[4].y + 1.;
            f.bounds[id] = NativeBounds::Glyphs(b);
        } else if mode == "cross-caption" {
            b[0] = f.rules.vertical[0].x - 1.;
            f.bounds[id] = NativeBounds::Glyphs(b);
        } else {
            if mode == "axis-nearly-overprinted" {
                b[0] += 0.25;
                b[2] += 0.25;
            }
            let extra = f.put("X", b, 13, 8.);
            if let Unit::Plain(ls) = &mut f.units[0] {
                ls.push(Line::text("X".into(), vec![extra]));
            }
        }
        let before = f.snapshot();
        f.run();
        assert_eq!(f.snapshot(), before, "{mode}");
    }
}
#[test]
fn missing_or_contradictory_separator_signatures_are_not_replaced_by_title_text() {
    for mode in [
        "caption-rail",
        "band",
        "field-stroke",
        "different-field-stroke",
        "nonfinite",
    ] {
        let mut f = fixture(2, "正誤", "値", false);
        match mode {
            "caption-rail" => {
                f.rules.vertical.pop();
            }
            "band" => {
                f.rules.horizontal.pop();
            }
            "field-stroke" => {
                f.rules.horizontal.remove(0);
            }
            "different-field-stroke" => f.rules.horizontal[0].y += 1.,
            _ => f.rules.horizontal[0].y = f32::INFINITY,
        };
        let before = f.snapshot();
        f.run();
        assert_eq!(f.snapshot(), before, "{mode}");
    }
    for mode in ["no-citation", "ordinary-body", "wrong-header"] {
        let mut f = fixture(2, "任意標題", "値", false);
        let id = if mode == "wrong-header" { 1 } else { 6 };
        let text = if mode == "wrong-header" {
            "別の欄"
        } else if mode == "no-citation" {
            "令和八年七月一日"
        } else {
            "普通の本文中で訂正を説明している。"
        };
        f.words[id].text = text.into();
        f.words[id].raw = text.into();
        if let Unit::Plain(ls) = &mut f.units[0] {
            ls[id].text = text.into();
        }
        let before = f.snapshot();
        f.run();
        assert_eq!(f.snapshot(), before, "{mode}");
    }
}
#[test]
fn enclosing_publications_and_quoted_examples_keep_their_original_scope() {
    for mode in ["parent", "quotes", "shinkyu"] {
        let mut f = fixture(2, "任意標題", "値", false);
        if mode == "parent" {
            f.rules.enclosures.push([0., 0., 2000., 2000.]);
        } else if mode == "shinkyu" {
            let Unit::Plain(ls) = f.units.remove(0) else {
                unreachable!()
            };
            f.units.push(Unit::Shinkyu {labels:None,
                preamble: ls,
                after: vec![],
                before: vec![],
                outside: vec![],
            });
        } else {
            let x = f.rules.vertical[1].x + 50.;
            let a = f.put("「", [x, 200., x + 8., 208.], 13, 8.);
            let b = f.put("」", [x, 220., x + 8., 228.], 13, 8.);
            if let Unit::Plain(ls) = &mut f.units[0] {
                ls.insert(0, Line::text("「".into(), vec![a]));
                ls.push(Line::text("」".into(), vec![b]));
            }
        }
        let before = f.snapshot();
        f.run();
        assert_eq!(f.snapshot(), before, "{mode}");
    }
}
#[test]
fn continuing_page_quotes_abstain_and_later_pages_can_resume() {
    let mut f = fixture(2, "任意標題", "値", false);
    let mut previous = f.page();
    previous.text = "「".into();
    let before = f.snapshot();
    f.run_previous(&[previous.clone()]);
    assert_eq!(f.snapshot(), before);
    let mut closed = previous.clone();
    closed.text = "本文」".into();
    f.run_previous(&[previous, closed]);
    assert!(f.table().is_some());
}
#[test]
fn actual_repeated_pages_use_coherent_global_table_markers() {
    let p = crate::page_inputs(include_bytes!(
        "../tests/archive/pages/20260731h01759/20260731h017590032.pdf"
    ))
    .unwrap();
    let both = [p[0].clone(), p[0].clone()];
    let a = crate::analyze_item(&both);
    assert_eq!(a.pages.len(), 2);
    assert_eq!(
        a.pages.iter().map(|p| p.tables.len()).collect::<Vec<_>>(),
        vec![1, 1]
    );
    for (i, p) in a.pages.iter().enumerate() {
        assert!(p.doc_text.contains(&crate::trace::table_marker(i)));
    }
    assert_eq!(a.articles.len(), 28);
}

#[test]
fn distinct_cell_columns_can_be_ragged_without_vertical_overlap() {
    let mut f = fixture(2, "任意標題", "値", false);
    let id = f.words.iter().position(|w| w.text == "新値").unwrap();
    let NativeBounds::Glyphs(mut b) = f.bounds[id] else {
        unreachable!()
    };
    let y = b[3];
    b[0] -= 16.;
    b[2] -= 16.;
    b[1] = y;
    b[3] = y + 8.;
    let next = f.put("続", b, 13, 8.);
    if let Unit::Plain(ls) = &mut f.units[0] {
        ls.push(Line::text("続".into(), vec![next]));
    }
    f.run();
    assert!(f.table().unwrap().cells.iter().any(|c| c.text == "新値続"));
}
#[test]
fn content_fonts_must_belong_to_the_same_original_glyph_styles() {
    let mut f = fixture(2, "任意標題", "値", false);
    f.content[0] = ContentStyles::Glyphs(NativeStyles::from([(12345, 9f64.to_bits())]));
    let before = f.snapshot();
    f.run();
    assert_eq!(f.snapshot(), before);
}
#[test]
fn original_word_emission_order_does_not_select_a_different_table() {
    let mut input = crate::page_inputs(include_bytes!(
        "../tests/archive/pages/20260731h01759/20260731h017590032.pdf"
    ))
    .unwrap();
    let before = crate::analyze_item(&input);
    let mut ws = input[0]
        .xhtml
        .lines()
        .filter(|l| l.trim_start().starts_with("<word "))
        .map(str::to_owned)
        .collect::<Vec<_>>();
    ws.reverse();
    let mut next = ws.into_iter();
    input[0].xhtml = input[0]
        .xhtml
        .lines()
        .map(|l| {
            if l.trim_start().starts_with("<word ") {
                next.next().unwrap()
            } else {
                l.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert!(next.next().is_none());
    assert_eq!(
        serde_json::to_value(before).unwrap(),
        serde_json::to_value(crate::analyze_item(&input)).unwrap()
    );
}
#[test]
fn original_font_ids_are_labels_not_known_faces() {
    let mut input = crate::page_inputs(include_bytes!(
        "../tests/archive/pages/20260731h01759/20260731h017590032.pdf"
    ))
    .unwrap();
    let before = crate::analyze_item(&input);
    for attr in ["data-native-styles", "data-native-content-styles"] {
        let prefix = format!(" {attr}=\"");
        let source = input[0].xhtml.clone();
        let mut rest = source.as_str();
        let mut out = String::new();
        while let Some(start) = rest.find(&prefix) {
            out.push_str(&rest[..start + prefix.len()]);
            rest = &rest[start + prefix.len()..];
            let end = rest.find('"').unwrap();
            let value = &rest[..end];
            if value == "unknown" {
                out.push_str(value);
            } else {
                out.push_str(
                    &value
                        .split_whitespace()
                        .map(|p| {
                            let (f, b) = p.split_once(':').unwrap();
                            format!("{}:{b}", f.parse::<usize>().unwrap() + 4096)
                        })
                        .collect::<Vec<_>>()
                        .join(" "),
                );
            }
            rest = &rest[end..];
        }
        out.push_str(rest);
        input[0].xhtml = out;
    }
    assert_eq!(
        serde_json::to_value(before).unwrap(),
        serde_json::to_value(crate::analyze_item(&input)).unwrap()
    );
}
#[test]
fn removing_original_field_font_provenance_leaves_the_uncertainty_unresolved() {
    let mut input = crate::page_inputs(include_bytes!(
        "../tests/archive/pages/20260731h01759/20260731h017590032.pdf"
    ))
    .unwrap();
    let mut changed = 0;
    input[0].xhtml = input[0]
        .xhtml
        .lines()
        .map(|line| {
            if line.contains(">ページ</word>") {
                changed += 1;
                let prefix = " data-native-styles=\"";
                let start = line.find(prefix).unwrap();
                let end =
                    start + prefix.len() + line[start + prefix.len()..].find('"').unwrap() + 1;
                format!("{}{}", &line[..start], &line[end..])
            } else {
                line.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert_eq!(changed, 1);
    let a = crate::analyze_item(&input);
    assert_eq!(a.articles.len(), 13);
    assert!(a.pages[0].tables.is_empty());
}
#[test]
fn an_unresolved_source_word_inside_the_table_is_not_a_certified_blank_cell() {
    for mode in [
        "empty",
        "noise",
        "ruby",
        "margin",
        "header-label",
        "unowned-used",
    ] {
        let mut f = fixture(2, "任意標題", "値", false);
        let target = f.words.iter().position(|w| w.text == "旧値").unwrap();
        let NativeBounds::Glyphs(mut b) = f.bounds[target] else {
            unreachable!()
        };
        b[0] -= 35.;
        b[2] -= 5.;
        let id = f.put("未解決の元文字", b, 13, 8.);
        f.words[id].text.clear();
        f.words[id].status = match mode {
            "empty" => WordStatus::Empty,
            "noise" => WordStatus::Noise,
            "ruby" => WordStatus::Ruby,
            "margin" => WordStatus::Margin,
            "header-label" => WordStatus::HeaderLabel,
            _ => WordStatus::Used,
        };
        f.content[id] = ContentStyles::Unknown;
        let before = f.snapshot();
        f.run();
        assert_eq!(
            f.snapshot(),
            before,
            "{mode}: raw content in the selected source area cannot silently become an empty cell"
        );
    }
}
#[test]
fn unrelated_unresolved_source_does_not_invalidate_a_source_complete_table() {
    let mut f = fixture(2, "任意標題", "値", false);
    let x = f.rules.vertical[1].x + 100.;
    let id = f.put("未解決の元文字", [x, 200., x + 8., 240.], 13, 8.);
    f.words[id].text.clear();
    f.words[id].status = WordStatus::Empty;
    f.content[id] = ContentStyles::Unknown;
    f.run();
    assert!(f.table().is_some());
}

#[test]
fn the_structured_document_retains_the_cited_publication_and_every_correction_cell() {
    let input = crate::page_inputs(include_bytes!(
        "../tests/archive/pages/20260731h01759/20260731h017590032.pdf"
    ))
    .unwrap();
    let a = crate::analyze_item(&input);
    let gold: serde_json::Value = serde_json::from_str(include_str!(
        "../tests/expected-ruled-corrections/20260731h017590032-gold.json"
    ))
    .unwrap();
    fn collect<'a>(v: &'a serde_json::Value, out: &mut Vec<&'a serde_json::Value>) {
        match v {
            serde_json::Value::Object(m) => {
                if let Some(t) = m.get("table") {
                    out.push(t);
                }
                for (k, v) in m {
                    if k != "table" {
                        collect(v, out);
                    }
                }
            }
            serde_json::Value::Array(v) => {
                for e in v {
                    collect(e, out)
                }
            }
            _ => {}
        }
    }
    let docs = serde_json::to_value(&a.articles).unwrap();
    let mut tables = vec![];
    collect(&docs, &mut tables);
    assert_eq!(tables.len(), 1);
    let table = tables[0];
    assert_eq!(table["rows"].as_array().unwrap().len(), 4);
    for g in gold["cells"].as_array().unwrap() {
        let row = g["row"].as_u64().unwrap() as usize;
        let col = g["col"].as_u64().unwrap() as usize;
        let text = table["rows"][row][col]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["text"].as_str().unwrap())
            .collect::<String>();
        assert_eq!(text, g["text"]);
    }
    assert_eq!(
        table["merges"],
        serde_json::json!([{"row":1,"col":0,"rowspan":1,"colspan":5}])
    );
}

#[test]
fn unresolved_source_in_caption_padding_is_inside_the_physical_source_claim() {
    for inside in [true, false] {
        let mut f = fixture(2, "任意標題", "値", false);
        let rail = f.rules.vertical[1].x;
        let x = if inside { rail - 3. } else { rail + 1. };
        let id = f.put("未解決の元文字", [x, 300., x + 2., 308.], 13, 8.);
        f.words[id].text.clear();
        f.words[id].status = WordStatus::Empty;
        f.content[id] = ContentStyles::Unknown;
        let before = f.snapshot();
        f.run();
        if inside {
            assert_eq!(
                f.snapshot(),
                before,
                "The paired native rails, including blank caption padding, delimit the claim"
            );
        } else {
            assert!(f.table().is_some());
        }
    }
}
#[test]
fn other_original_correction_publications_keep_every_manually_assigned_cell_and_source_group() {
    let cases: &[(&[u8], &str)] = &[
        (
            include_bytes!("../tests/archive/pages/20260727h01755/20260727h017550032.pdf"),
            include_str!("../tests/expected-ruled-corrections/20260727h017550032-gold.json"),
        ),
        (
            include_bytes!("../tests/archive/pages/20260810h01765/20260810h017650032.pdf"),
            include_str!("../tests/expected-ruled-corrections/20260810h017650032-gold.json"),
        ),
        (
            include_bytes!("../tests/archive/pages/20260826h01776/20260826h017760032.pdf"),
            include_str!("../tests/expected-ruled-corrections/20260826h017760032-gold.json"),
        ),
        (
            include_bytes!("../tests/archive/pages/20260902h01781/20260902h017810032.pdf"),
            include_str!("../tests/expected-ruled-corrections/20260902h017810032-gold.json"),
        ),
        (
            include_bytes!("../tests/archive/pages/20260914h01789/20260914h017890032.pdf"),
            include_str!("../tests/expected-ruled-corrections/20260914h017890032-gold.json"),
        ),
        (
            include_bytes!("../tests/archive/pages/20260918h01793/20260918h017930032.pdf"),
            include_str!("../tests/expected-ruled-corrections/20260918h017930032-gold.json"),
        ),
        (
            include_bytes!("../tests/archive/pages/20261013h01806/20261013h018060032.pdf"),
            include_str!("../tests/expected-ruled-corrections/20261013h018060032-gold.json"),
        ),
    ];
    for (pdf, gold) in cases {
        let gold: serde_json::Value = serde_json::from_str(gold).unwrap();
        let input = crate::page_inputs(pdf).unwrap();
        let a = crate::analyze_item(&input);
        let ts: Vec<_> = a.pages[0]
            .regions
            .iter()
            .flat_map(|r| &r.lines)
            .filter_map(|l| l.table.as_ref())
            .collect();
        assert_eq!(ts.len(), 1, "{}", gold["page"]);
        let t = ts[0];
        assert_eq!(
            (t.rows, t.cols),
            (gold["rows"].as_u64().unwrap() as usize, 5)
        );
        assert_eq!(t.cells.len(), gold["cells"].as_array().unwrap().len());
        for (c, g) in t.cells.iter().zip(gold["cells"].as_array().unwrap()) {
            assert_eq!(
                (c.row, c.col, c.rowspan, c.colspan),
                (
                    g["row"].as_u64().unwrap() as usize,
                    g["col"].as_u64().unwrap() as usize,
                    1,
                    g["colspan"].as_u64().unwrap() as usize
                )
            );
            assert_eq!(c.text, g["text"].as_str().unwrap());
            assert_eq!(
                c.words,
                g["source_word_ids"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|n| n.as_u64().unwrap() as usize)
                    .collect::<Vec<_>>()
            );
        }
        let text = crate::trace::join_page_texts(&a.pages, &|p| p.doc_text.as_str());
        let mut original = crate::segment_articles(&text);
        crate::wrapped_notice_titles::repair_wrapped_notice_titles(&mut original, &a.pages);
        let rules: Vec<_> = input.iter().map(|p| p.rules.clone()).collect();
        let parts = super::super::groups(&original, &text, &a.pages, &rules);
        assert_eq!(parts.iter().map(Vec::len).sum::<usize>(), a.articles.len());
        let expected: BTreeSet<_> = gold["expected_source_group"]
            .as_array()
            .unwrap()
            .iter()
            .map(|n| n.as_u64().unwrap() as usize)
            .collect();
        assert!(
            parts.iter().flatten().any(|p| p
                .sources
                .iter()
                .map(|(page, id)| {
                    assert_eq!(*page, 0);
                    *id
                })
                .collect::<BTreeSet<_>>()
                == expected),
            "{}",
            gold["page"]
        );
    }
}
#[test]
fn locator_content_without_a_page_anchor_does_not_silently_become_a_continuation() {
    let mut f = fixture(2, "任意標題", "値", false);
    let id = f.words.iter().position(|w| w.text == "2").unwrap();
    f.words[id].raw = " ".into();
    f.words[id].text.clear();
    f.words[id].status = WordStatus::Empty;
    f.content[id] = ContentStyles::Glyphs(NativeStyles::new());
    if let Unit::Plain(lines) = &mut f.units[0] {
        lines.retain(|l| !l.words.contains(&id));
    }
    let before = f.snapshot();
    f.run();
    assert_eq!(
        f.snapshot(),
        before,
        "A disconnected locator block is not proved to be the previous record's wrapped content"
    );
}
