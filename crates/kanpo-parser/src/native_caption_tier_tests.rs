use super::*;
use crate::{
    lines::{HRule, VRule},
    trace::{NativeStyles, WordStatus},
};
use std::collections::BTreeSet;
#[test]
fn a_glyph_bearing_cannot_overlap_an_unrelated_native_occurrence_outside_the_field() {
    let mut f = fixture();
    let b = [9.75, 30., 18., 210.];
    f.bounds[6] = NativeBounds::Glyphs(b);
    f.flows[6] = source_flow(b);
    let outside = f.add("外", [9.5, 30., 9.9, 210.], 66);
    f.tiers.push(vec![outside]);
    unchanged(f);
}
#[test]
fn an_open_field_edge_uses_the_original_vertical_path_not_a_glyph_bearing_tolerance() {
    let mut f = fixture();
    let b = [9.75, 30., 18., 210.];
    f.bounds[6] = NativeBounds::Glyphs(b);
    f.flows[6] = source_flow(b);
    assert!(f.flows[6].unwrap().vertical_path(b).is_some());
    assert_eq!(f.run(), 1);
}
#[test]
fn a_closed_left_wall_or_unbound_source_path_cannot_authorize_protruding_glyphs() {
    for missing in [false, true] {
        let mut f = fixture();
        let b = [9.75, 30., 18., 210.];
        f.bounds[6] = NativeBounds::Glyphs(b);
        f.flows[6] = source_flow(b);
        if missing {
            f.flows[6] = None;
        } else {
            f.rules.vertical.push(VRule {
                x: 10.,
                y0: 20.,
                y1: 420.,
            });
        }
        unchanged(f);
    }
    let mut f = fixture();
    let b = [9.75, 30., 18., 210.];
    f.bounds[6] = NativeBounds::Glyphs(b);
    f.flows[6] = source_flow([0., 30., 8., 210.]);
    unchanged(f);
}
#[test]
fn an_opaque_active_occurrence_cannot_be_ignored_because_its_compatibility_box_is_elsewhere() {
    let mut f = fixture();
    let extra = f.add("出典不明", [-100., 30., -90., 210.], 4);
    f.bounds[extra.id] = NativeBounds::Unknown;
    f.tiers.push(vec![extra]);
    unchanged(f);
}
#[test]
fn a_native_certificate_cannot_be_rebound_to_different_column_text() {
    let mut f = fixture();
    let mut tiers = std::mem::take(&mut f.tiers);
    let r = merge(&mut tiers, f.source(), Some(&f.rules));
    assert_eq!(r.certificates.len(), 1);
    assert!(r.certificates[0].matches(&tiers[0]));
    tiers[0][0].text = "別の原字".into();
    assert!(!r.certificates[0].matches(&tiers[0]));
}
#[test]
fn a_native_certificate_cannot_be_rebound_to_different_column_geometry() {
    let mut f = fixture();
    let mut tiers = std::mem::take(&mut f.tiers);
    let r = merge(&mut tiers, f.source(), Some(&f.rules));
    assert_eq!(r.certificates.len(), 1);
    tiers[0][0].x += 1.;
    assert!(!r.certificates[0].matches(&tiers[0]));
}
#[test]
fn source_occurrences_marked_outside_active_input_cannot_certify_native_fields() {
    for id in [0, 6] {
        let mut f = fixture();
        f.words[id].status = WordStatus::Margin;
        unchanged(f);
    }
}
fn source_flow(b: [f32; 4]) -> Option<crate::native_column_flow::AxisFlow> {
    let x = (b[0] as f64 + b[2] as f64) / 2.;
    let xml=format!("<word xmin='{}' ymin='{}' xmax='{}' ymax='{}' data-native-xmin='{}' data-native-ymin='{}' data-native-xmax='{}' data-native-ymax='{}' data-wmode='1' data-native-axis='down' data-native-flow-start='{}' data-native-flow-end='{}' data-native-flow-baseline='{}' data-native-flow-leading='{}' data-native-flow-trailing='0' data-native-flow-prefix='0' data-native-flow-first-glyph='0' data-native-flow-after-glyph='1'>X</word>",b[0],b[1],b[2],b[3],b[0],b[1],b[2],b[3],b[1],b[3],-x,b[3]-b[1]);
    let h = scraper::Html::parse_fragment(&xml);
    crate::native_column_flow::from_word(
        h.select(&scraper::Selector::parse("word").unwrap())
            .next()
            .unwrap(),
        Some(1),
    )
}
struct Fixture {
    words: Vec<Word>,
    bounds: Vec<NativeBounds>,
    axes: Vec<NativeAxis>,
    styles: Vec<ContentStyles>,
    flows: Vec<Option<crate::native_column_flow::AxisFlow>>,
    tiers: Vec<Vec<Col>>,
    rules: PageRules,
}
impl Fixture {
    fn source(&self) -> Source<'_> {
        Source {
            words: &self.words,
            bounds: &self.bounds,
            axes: &self.axes,
            styles: &self.styles,
            flows: &self.flows,
        }
    }
    fn run(&mut self) -> usize {
        let mut tiers = std::mem::take(&mut self.tiers);
        let n = merge(&mut tiers, self.source(), Some(&self.rules));
        self.tiers = tiers;
        n.merged
    }
    fn add(&mut self, text: &str, b: [f32; 4], font: usize) -> Col {
        let id = self.words.len();
        self.words.push(Word {
            id,
            source_box: b,
            x0: b[0],
            y0: b[1],
            x1: b[2],
            y1: b[3],
            raw: text.into(),
            text: text.into(),
            status: WordStatus::Used,
        });
        self.bounds.push(NativeBounds::Glyphs(b));
        self.axes.push(NativeAxis::Down);
        self.flows.push(source_flow(b));
        self.styles.push(ContentStyles::Glyphs(BTreeSet::from([(
            font,
            8f64.to_bits(),
        )])));
        Col {
            id,
            x: b[0],
            y: b[1],
            x1: b[2],
            y1: b[3],
            text: text.into(),
        }
    }
}
fn fixture() -> Fixture {
    let mut f = Fixture {
        words: vec![],
        bounds: vec![],
        axes: vec![],
        styles: vec![],
        flows: vec![],
        tiers: vec![],
        rules: PageRules::default(),
    };
    let mut go = vec![];
    let mut mae = vec![];
    for (t, y) in [("改", 40.), ("正", 100.), ("後", 180.)] {
        go.push(f.add(t, [216., y, 224., y + 8.], 41));
    }
    for (t, y) in [("改", 240.), ("正", 300.), ("前", 380.)] {
        mae.push(f.add(t, [216., y, 224., y + 8.], 41));
    }
    let upper = f.add("任意の長い上欄本文", [180., 30., 188., 210.], 97);
    let lower = f.add("独立した下欄", [180., 230., 188., 250.], 58);
    go.push(upper);
    let mut mid = vec![mae.remove(0), lower];
    f.tiers = vec![go, std::mem::take(&mut mid), mae];
    f.rules.vertical = [210., 230.]
        .into_iter()
        .map(|x| VRule {
            x,
            y0: 20.,
            y1: 420.,
        })
        .collect();
    f.rules.horizontal = [20., 220., 420.]
        .into_iter()
        .map(|y| HRule {
            y,
            x0: 10.,
            x1: 230.,
        })
        .collect();
    f
}
fn snap(cols: &[Col]) -> String {
    format!(
        "{:?}",
        cols.iter()
            .map(|c| (
                c.id,
                [c.x.to_bits(), c.y.to_bits(), c.x1.to_bits(), c.y1.to_bits()],
                &c.text
            ))
            .collect::<Vec<_>>()
    )
}
fn unchanged(mut f: Fixture) {
    let before = f.tiers.iter().map(|t| snap(t)).collect::<Vec<_>>();
    assert_eq!(f.run(), 0);
    assert_eq!(f.tiers.iter().map(|t| snap(t)).collect::<Vec<_>>(), before);
}
#[test]
fn native_captions_rejoin_all_fragments_without_a_tier_count_dispatch() {
    for count in [2, 3, 4, 19, 53] {
        let mut f = fixture();
        for i in 0..count {
            let x = 170. - (i as f32) * 2.;
            let c = f.add(&format!("別の原文{i}"), [x, 30., x + 1., 210.], i + 500);
            f.tiers[0].push(c);
        }
        let all = f.tiers.into_iter().flatten().collect::<Vec<_>>();
        let before = snap(&all);
        f.tiers = if count == 3 {
            vec![all[..4].to_vec(), all[4..6].to_vec(), all[6..].to_vec()]
        } else if count == 2 {
            vec![all[..4].to_vec(), all[4..].to_vec()]
        } else {
            all.into_iter().map(|c| vec![c]).collect()
        };
        assert_eq!(f.run(), 1, "fragments {count}");
        assert_eq!(f.tiers.len(), 1);
        assert_eq!(snap(&f.tiers[0]), before);
    }
}
#[test]
fn exactly_touching_native_rail_fragments_define_the_same_fields() {
    let mut f = fixture();
    f.rules.horizontal = [20., 220., 420.]
        .into_iter()
        .flat_map(|y| {
            [
                HRule {
                    y,
                    x0: 10.,
                    x1: 123.,
                },
                HRule {
                    y,
                    x0: 123.,
                    x1: 230.,
                },
            ]
        })
        .collect();
    assert_eq!(f.run(), 1);
}
#[test]
fn missing_or_open_source_frame_edges_are_not_filled_with_a_distance_tolerance() {
    for i in 0..3 {
        let mut f = fixture();
        f.rules.horizontal.remove(i);
        unchanged(f);
    }
    for i in 0..2 {
        let mut f = fixture();
        f.rules.vertical.remove(i);
        unchanged(f);
    }
    let mut f = fixture();
    f.rules.horizontal[1].x0 = 10.1;
    unchanged(f);
    let mut f = fixture();
    f.rules.horizontal = [20., 220., 420.]
        .into_iter()
        .flat_map(|y| {
            [
                HRule {
                    y,
                    x0: 10.,
                    x1: 184.,
                },
                HRule {
                    y,
                    x0: 184.01,
                    x1: 230.,
                },
            ]
        })
        .collect();
    unchanged(f);
}
#[test]
fn source_roles_do_not_come_from_body_similarity_or_expected_vocabulary() {
    let mut f = fixture();
    f.words[6].text = "甲乙丙丁戊己庚辛壬癸".into();
    f.words[6].raw = f.words[6].text.clone();
    f.tiers[0].last_mut().unwrap().text = f.words[6].text.clone();
    assert_eq!(f.run(), 1);
}
#[test]
fn native_field_caption_evidence_cannot_borrow_unknown_or_foreign_source_data() {
    for id in 0..8 {
        let mut f = fixture();
        f.bounds[id] = NativeBounds::Unknown;
        unchanged(f);
    }
    for id in 0..6 {
        let mut f = fixture();
        f.axes[id] = NativeAxis::Right;
        unchanged(f);
        let mut f = fixture();
        f.styles[id] = ContentStyles::Unknown;
        unchanged(f);
    }
    let mut f = fixture();
    f.words[0].raw = "別".into();
    unchanged(f);
    let mut f = fixture();
    f.tiers[0][0].text = "別".into();
    unchanged(f);
    let mut f = fixture();
    let duplicate = f.tiers[0][0].clone();
    f.tiers[0].push(duplicate);
    unchanged(f);
}
#[test]
fn other_caption_words_and_intervening_content_do_not_authorize_a_pair() {
    for text in ["先", "后", "引用"] {
        let mut f = fixture();
        f.words[2].raw = text.into();
        f.words[2].text = text.into();
        f.tiers[0][2].text = text.into();
        unchanged(f);
    }
    let mut f = fixture();
    let extra = f.add("他", [216., 80., 224., 88.], 12);
    f.tiers[0].push(extra);
    unchanged(f);
}
#[test]
fn a_mixed_tier_cannot_lend_an_unrelated_article_to_the_native_frame() {
    let mut f = fixture();
    let extra = f.add("枠外の別記事", [-20., 30., 0., 210.], 6);
    f.tiers[0].push(extra);
    unchanged(f);
}
#[test]
fn native_caption_certificate_is_invariant_under_translation_scale_and_font_identifiers() {
    for scale in [0.125f32, 2., 16.] {
        let mut f = fixture();
        for i in 0..f.words.len() {
            let b = f.words[i].source_box;
            let b = [
                b[0] * scale - 320.,
                b[1] * scale + 17.,
                b[2] * scale - 320.,
                b[3] * scale + 17.,
            ];
            f.bounds[i] = NativeBounds::Glyphs(b);
            f.flows[i] = source_flow(b);
            f.words[i].source_box = b;
            f.words[i].x0 = b[0];
            f.words[i].y0 = b[1];
            f.words[i].x1 = b[2];
            f.words[i].y1 = b[3];
            f.styles[i] = ContentStyles::Glyphs(NativeStyles::from([(
                7000 + i,
                (8f64 * scale as f64).to_bits(),
            )]));
            for c in f.tiers.iter_mut().flatten().filter(|c| c.id == i) {
                c.x = b[0];
                c.y = b[1];
                c.x1 = b[2];
                c.y1 = b[3];
            }
        }
        for r in &mut f.rules.horizontal {
            r.y = r.y * scale + 17.;
            r.x0 = r.x0 * scale - 320.;
            r.x1 = r.x1 * scale - 320.;
        }
        for r in &mut f.rules.vertical {
            r.x = r.x * scale - 320.;
            r.y0 = r.y0 * scale + 17.;
            r.y1 = r.y1 * scale + 17.;
        }
        assert_eq!(f.run(), 1);
    }
}
#[test]
fn already_whole_native_fields_are_not_reordered_or_rewritten() {
    let mut f = fixture();
    f.tiers = vec![f.tiers.into_iter().flatten().collect()];
    unchanged(f);
}
